//! Remote scanning: `bfiles -p 'user@host:/path'` runs bfiles on the remote
//! machine over ssh and streams back a tiny summary instead of raw data.
//!
//! The remote side (`bfiles --serve`) scans and aggregates natively, then
//! emits a line protocol on stdout:
//!
//! ```text
//! bfiles-serve 1
//! P <items>              repeated while scanning; drives the local spinner
//! D <items> <millis>     scan finished
//! W <unreadable-count>   only when directories could not be read
//! S <note>               skipped-directory notes, preformatted
//! T <total-bytes>
//! G <bytes> <label>      one per group, followed by its child rows
//! R <bytes> <relative-path>
//! OK
//! ```
//!
//! Labels have backslashes and newlines escaped so every record is one line.

use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

use crate::cli::{Config, EngineType};
use crate::formatting::{DirectoryGroup, DirectoryRow, TreeSummary, format_tree_output};

pub const PROTOCOL_HEADER: &str = "bfiles-serve 1";

pub struct RemoteTarget {
        pub destination: String,
        pub path: String,
}

/// Detect scp-style remote paths: `host:/path` or `user@host:/path`.
/// Anything with a slash before the colon is local, as are single-letter
/// prefixes (Windows drive letters like `C:\`).
pub fn parse_remote_path(path: &Path) -> Option<RemoteTarget> {
        let raw = path.to_str()?;
        let (destination, remote_path) = raw.split_once(':')?;

        if destination.is_empty() || remote_path.is_empty() {
                return None;
        }

        if destination.len() == 1 || destination.contains('/') || destination.contains('\\') {
                return None;
        }

        Some(RemoteTarget { destination: destination.to_string(), path: remote_path.to_string() })
}

fn escape(label: &str) -> String {
        label.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(label: &str) -> String {
        let mut out = String::with_capacity(label.len());
        let mut chars = label.chars();

        while let Some(c) = chars.next() {
                if c != '\\' {
                        out.push(c);
                        continue;
                }

                match chars.next() {
                        Some('n') => out.push('\n'),
                        Some(other) => out.push(other),
                        None => out.push('\\'),
                }
        }

        out
}

fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', r"'\''"))
}

/// Progress emitter for `--serve`: one `P <items>` line every 100ms.
pub fn start_serve_progress(counter: Arc<AtomicU64>, done: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                        let mut out = io::stdout().lock();
                        let _ = writeln!(out, "P {}", counter.load(Ordering::Relaxed));
                        let _ = out.flush();
                        drop(out);

                        std::thread::sleep(Duration::from_millis(100));
                }
        })
}

/// Serve side: write everything after the scan finished.
pub fn write_serve_report(items: u64, duration: Duration, unreadable: u64, notes: &[String], summary: &TreeSummary) -> io::Result<()> {
        let mut out = io::stdout().lock();

        writeln!(out, "D {} {}", items, duration.as_millis())?;

        if unreadable > 0 {
                writeln!(out, "W {unreadable}")?;
        }

        for note in notes {
                writeln!(out, "S {}", escape(note))?;
        }

        writeln!(out, "T {}", summary.total_size)?;

        for group in &summary.groups {
                writeln!(out, "G {} {}", group.total_size, escape(&group.root.display().to_string()))?;

                for child in &group.children {
                        writeln!(out, "R {} {}", child.size, escape(&child.relative_path.display().to_string()))?;
                }
        }

        writeln!(out, "OK")?;
        out.flush()
}

#[derive(Debug, Default)]
pub struct ServeReport {
        pub items: u64,
        pub duration_ms: u64,
        pub unreadable: u64,
        pub notes: Vec<String>,
        pub summary: TreeSummary,
        pub complete: bool,
}

/// Client side: consume the protocol, feeding progress counts to `on_progress`.
pub fn read_serve_report(reader: impl BufRead, mut on_progress: impl FnMut(u64)) -> io::Result<ServeReport> {
        let mut report = ServeReport::default();
        let mut lines = reader.lines();

        match lines.next() {
                Some(Ok(header)) if header == PROTOCOL_HEADER => {}
                _ => {
                        return Err(io::Error::other(
                                "remote did not answer with a bfiles scan.\n\
                 Make sure bfiles is installed and up to date on the remote host:\n\
                 curl -fsSL https://raw.githubusercontent.com/johnkesko/bfiles/master/install.sh | sh",
                        ));
                }
        }

        for line in lines {
                let line = line?;
                let (tag, rest) = line.split_once(' ').unwrap_or((line.as_str(), ""));

                match tag {
                        "P" => {
                                if let Ok(items) = rest.parse() {
                                        on_progress(items);
                                }
                        }
                        "D" => {
                                let mut parts = rest.split(' ');
                                report.items = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                                report.duration_ms = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                        }
                        "W" => report.unreadable = rest.parse().unwrap_or(0),
                        "S" => report.notes.push(unescape(rest)),
                        "T" => report.summary.total_size = rest.parse().unwrap_or(0),
                        "G" => {
                                let (size, label) = rest.split_once(' ').unwrap_or(("0", rest));
                                report.summary.groups.push(DirectoryGroup {
                                        root: PathBuf::from(unescape(label)),
                                        total_size: size.parse().unwrap_or(0),
                                        children: Vec::new(),
                                });
                        }
                        "R" => {
                                let (size, label) = rest.split_once(' ').unwrap_or(("0", rest));
                                if let Some(group) = report.summary.groups.last_mut() {
                                        group.children.push(DirectoryRow { relative_path: PathBuf::from(unescape(label)), size: size.parse().unwrap_or(0) });
                                }
                        }
                        "OK" => {
                                report.complete = true;
                                break;
                        }
                        _ => {} // Unknown tags from newer protocol revisions are ignored.
                }
        }

        Ok(report)
}

fn remote_command(target: &RemoteTarget, config: &Config) -> String {
        let mut parts =
                vec!["bfiles".to_string(), "--serve".to_string(), "-p".to_string(), shell_quote(&target.path), "-t".to_string(), config.top_n.to_string()];

        if config.max_depth != usize::MAX {
                parts.push("-d".to_string());
                parts.push(config.max_depth.to_string());
        }

        if let EngineType::Rayon = config.engine {
                parts.push("-e".to_string());
                parts.push("rayon".to_string());
        }

        if config.include_cloud {
                parts.push("--include-cloud".to_string());
        }

        for exclude in &config.exclude {
                parts.push("--exclude".to_string());
                parts.push(shell_quote(&exclude.display().to_string()));
        }

        parts.join(" ")
}

/// Run the scan on the remote host and render the streamed summary locally.
pub fn run_remote(target: &RemoteTarget, config: &Config) -> io::Result<()> {
        let command = remote_command(target, config);

        // BFILES_REMOTE_SHELL overrides the transport (`<shell> -c <command>`),
        // used by tests and available for custom ssh wrappers.
        let mut child = match std::env::var("BFILES_REMOTE_SHELL") {
                Ok(shell) => Command::new(shell).arg("-c").arg(&command).stdout(Stdio::piped()).spawn(),
                Err(_) => Command::new("ssh").arg("--").arg(&target.destination).arg(&command).stdout(Stdio::piped()).spawn(),
        }
        .map_err(|e| io::Error::other(format!("could not start ssh: {e}")))?;

        let stdout = child.stdout.take().expect("stdout was piped");

        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::with_template("{spinner:.green} {pos} items [{elapsed_precise}] (remote)").unwrap());

        let result = read_serve_report(BufReader::new(stdout), |items| pb.set_position(items));
        pb.finish_and_clear();

        let status = child.wait()?;
        let report = result?;

        if !report.complete {
                return Err(io::Error::other(format!(
                        "remote scan on '{}' ended before finishing (exit: {})",
                        target.destination,
                        status.code().map_or_else(|| "killed".to_string(), |c| c.to_string()),
                )));
        }

        println!("Traversed {} items in {:.2?} on {}", report.items, Duration::from_millis(report.duration_ms), target.destination);

        if report.unreadable > 0 {
                let noun = if report.unreadable == 1 { "directory" } else { "directories" };
                eprintln!("warning: {} {noun} could not be read; reported sizes are underestimated", report.unreadable);
        }

        for note in &report.notes {
                eprintln!("note: {note}");
        }

        println!();

        if report.summary.groups.is_empty() {
                println!("No child directories found.");
                return Ok(());
        }

        println!("{}", format_tree_output(&report.summary));

        Ok(())
}

#[cfg(test)]
mod tests {
        use super::*;
        use std::io::Cursor;

        #[test]
        fn detects_remote_paths_and_leaves_local_ones_alone() {
                let remote = parse_remote_path(Path::new("andreas@pi01.local:/srv/storage")).unwrap();
                assert_eq!(remote.destination, "andreas@pi01.local");
                assert_eq!(remote.path, "/srv/storage");

                let bare_host = parse_remote_path(Path::new("pi01:data")).unwrap();
                assert_eq!(bare_host.destination, "pi01");
                assert_eq!(bare_host.path, "data");

                assert!(parse_remote_path(Path::new("/usr/local/bin")).is_none());
                assert!(parse_remote_path(Path::new("./notes:today")).is_none());
                assert!(parse_remote_path(Path::new("C:\\Users\\a")).is_none(), "drive letters are local");
                assert!(parse_remote_path(Path::new("host:")).is_none(), "empty remote path");
        }

        #[test]
        fn escaping_round_trips() {
                for label in ["plain", "with space", "back\\slash", "new\nline", "both\\\nmix"] {
                        assert_eq!(unescape(&escape(label)), label);
                }
        }

        #[test]
        fn quotes_shell_arguments() {
                assert_eq!(shell_quote("/plain/path"), "'/plain/path'");
                assert_eq!(shell_quote("with'quote"), r"'with'\''quote'");
        }

        #[test]
        fn serve_report_round_trips_through_protocol() {
                let input = format!("{PROTOCOL_HEADER}\n\
             P 10\n\
             P 250\n\
             D 300 1500\n\
             W 2\n\
             S skipped /x (cloud storage; pass --include-cloud to scan it)\n\
             T 5000\n\
             G 3000 photos\n\
             R 2000 photos/raw\n\
             G 2000 (files)\n\
             OK\n");

                let mut seen_progress = Vec::new();
                let report = read_serve_report(Cursor::new(input), |items| seen_progress.push(items)).unwrap();

                assert_eq!(seen_progress, vec![10, 250]);
                assert_eq!(report.items, 300);
                assert_eq!(report.duration_ms, 1500);
                assert_eq!(report.unreadable, 2);
                assert_eq!(report.notes.len(), 1);
                assert_eq!(report.summary.total_size, 5000);
                assert_eq!(report.summary.groups.len(), 2);
                assert_eq!(report.summary.groups[0].root, PathBuf::from("photos"));
                assert_eq!(report.summary.groups[0].children[0].size, 2000);
                assert!(report.complete);
        }

        #[test]
        fn missing_header_yields_install_hint() {
                let err = read_serve_report(Cursor::new("bash: bfiles: command not found\n"), |_| {}).unwrap_err();

                assert!(err.to_string().contains("install.sh"));
        }
}
