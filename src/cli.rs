use crate::traverse::crossbeam::CrossbeamTraversal;
use crate::traverse::{TraversalEngine, rayon::RayonTraversal};
use clap::{Parser, Subcommand, ValueEnum};
use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{io, path::PathBuf};

use indicatif::{ProgressBar, ProgressStyle};

use crate::formatting::{DEFAULT_CHILDREN_PER_GROUP, summarize_tree};
use crate::print::print_tree;
use crate::progress::start_progress;
use crate::remote;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EngineType {
        Rayon,
        Crossbeam,
}

#[derive(Parser)]
#[command(
        name = "bfiles",
        version,
        about = "Fast parallel directory size analyzer",
        after_help = "Examples:\n  bfiles -p .\n  bfiles -p . -e rayon -t 20\n  bfiles --path ./my-folder --max_depth 2 --top 10\n  bfiles -p ~ --exclude ~/Library --exclude ~/.cache\n  bfiles -p ~ --include-cloud\n  bfiles -p 'user@host:/srv/storage'    (scan runs on the remote host over ssh)\n  bfiles upgrade"
)]
pub struct Config {
        #[command(subcommand)]
        pub command: Option<Command>,

        /// Path to analyze
        #[arg(short, long)]
        pub path: Option<PathBuf>,

        /// Traversal engine
        #[arg(short, long, value_enum, default_value = "crossbeam")]
        pub engine: EngineType,

        /// Limit traversal depth [default: unlimited]
        #[arg(short = 'd', long = "max_depth", default_value_t = usize::MAX, hide_default_value = true)]
        pub max_depth: usize,

        /// Show top N root groups
        #[arg(short = 't', long = "top", default_value_t = 10)]
        pub top_n: usize,

        /// Exclude a directory from the scan (repeatable)
        #[arg(long = "exclude", value_name = "PATH")]
        pub exclude: Vec<PathBuf>,

        /// Also scan cloud-synced folders (~/Library/CloudStorage), which are
        /// skipped by default because listing them is extremely slow
        #[arg(long = "include-cloud")]
        pub include_cloud: bool,

        /// Emit machine-readable results (used internally on the far end of a
        /// remote scan)
        #[arg(long = "serve", hide = true)]
        pub serve: bool,
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Command {
        /// Update bfiles in place to the latest GitHub release
        Upgrade,
}

pub fn run() -> Result<(), io::Error> {
        let config = Config::parse();

        if let Some(Command::Upgrade) = config.command {
                return upgrade();
        }

        let Some(path) = config.path.clone() else {
                // Bare `bfiles` shows the full help like `--help`, but still exits
                // non-zero (2, matching clap's missing-argument convention) so
                // scripts don't mistake it for a successful run.
                use clap::CommandFactory;
                Config::command().print_help().ok();
                std::process::exit(2);
        };

        // scp-style paths like user@host:/srv/data run the scan over ssh.
        if !config.serve
                && let Some(target) = remote::parse_remote_path(&path)
        {
                return remote::run_remote(&target, &config);
        }

        if !path.is_dir() {
                eprintln!("error: '{}' is not an accessible directory", path.display());
                std::process::exit(2);
        }

        // Canonicalize so exclude paths (always absolute) can match scanned paths.
        let path = std::fs::canonicalize(&path).unwrap_or(path);

        let cloud_excludes: Vec<PathBuf> = if config.include_cloud { Vec::new() } else { cloud_storage_dirs() };

        let user_excludes: Vec<PathBuf> = config.exclude.iter().map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())).collect();

        let excludes: Vec<PathBuf> = cloud_excludes.iter().chain(user_excludes.iter()).cloned().collect();

        let engine: Box<dyn TraversalEngine> = match config.engine {
                EngineType::Rayon => Box::new(RayonTraversal),
                EngineType::Crossbeam => Box::new(CrossbeamTraversal),
        };

        let counter = Arc::new(AtomicU64::new(0));
        let errors = Arc::new(AtomicU64::new(0));
        let done = Arc::new(AtomicBool::new(false));

        let pb = if config.serve {
                println!("{}", remote::PROTOCOL_HEADER);
                None
        } else {
                // Created only now, so help/error/upgrade paths never flash a spinner.
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::with_template("{spinner:.green} {pos} items [{elapsed_precise}]").unwrap());
                pb.set_message("Searching...");
                Some(pb)
        };

        let progress_handle = match &pb {
                Some(pb) => start_progress(pb, Arc::clone(&counter), Arc::clone(&done)),
                None => remote::start_serve_progress(Arc::clone(&counter), Arc::clone(&done)),
        };

        let start = std::time::Instant::now();
        let tree = engine.run(&path, config.max_depth, &counter, &errors, &excludes);
        let duration = start.elapsed();

        done.store(true, Ordering::Relaxed);
        progress_handle.join().ok();
        if let Some(pb) = &pb {
                pb.finish_and_clear();
        }

        let items = counter.load(Ordering::Relaxed);
        let failed = errors.load(Ordering::Relaxed);

        // Only report skips that were actually inside the scanned tree.
        let mut notes = Vec::new();
        for skipped in cloud_excludes.iter().filter(|e| e.starts_with(&path) && **e != path && e.is_dir()) {
                notes.push(format!("skipped {} (cloud storage; pass --include-cloud to scan it)", skipped.display()));
        }
        for skipped in user_excludes.iter().filter(|e| e.starts_with(&path) && **e != path && e.is_dir()) {
                notes.push(format!("skipped {} (--exclude)", skipped.display()));
        }

        if config.serve {
                let summary = summarize_tree(&tree, config.top_n, DEFAULT_CHILDREN_PER_GROUP);
                return remote::write_serve_report(items, duration, failed, &notes, &summary);
        }

        println!("Traversed {} items in {:.2?}", items, duration);

        if failed > 0 {
                let noun = if failed == 1 { "directory" } else { "directories" };
                eprintln!("warning: {failed} {noun} could not be read; reported sizes are underestimated");
        }

        for note in &notes {
                eprintln!("note: {note}");
        }

        print_tree(&tree, config.top_n);

        Ok(())
}

/// Cloud-synced folders that macOS backs with File Provider daemons. Listing
/// them goes through per-provider IPC instead of the disk and can take minutes.
fn cloud_storage_dirs() -> Vec<PathBuf> {
        let Some(home) = std::env::var_os("HOME") else {
                return Vec::new();
        };

        let library = PathBuf::from(home).join("Library");

        vec![library.join("CloudStorage"), library.join("Mobile Documents")]
}

/// Asset infix used in the release file names (e.g. `bfiles-macos-arm64.tar.gz`).
fn release_target() -> Option<&'static str> {
        Some(match (std::env::consts::OS, std::env::consts::ARCH) {
                ("macos", "aarch64") => "macos-arm64",
                ("macos", "x86_64") => "macos-x86_64",
                ("linux", "x86_64") => "linux-x86_64",
                ("linux", "aarch64") => "linux-aarch64",
                ("windows", "x86_64") => "windows-x86_64",
                _ => return None,
        })
}

fn upgrade() -> Result<(), io::Error> {
        let to_io = |e: self_update::errors::Error| io::Error::other(e.to_string());

        let target = release_target().ok_or_else(|| io::Error::other("no prebuilt bfiles release for this OS/architecture"))?;

        let status = self_update::backends::github::Update::configure()
                .repo_owner("JohnKesko")
                .repo_name("bfiles")
                .bin_name("bfiles")
                .target(target)
                .show_download_progress(true)
                .current_version(self_update::cargo_crate_version!())
                .build()
                .map_err(to_io)?
                .update()
                .map_err(to_io)?;

        if status.updated() {
                println!("Upgraded to {}", status.version());
        } else {
                println!("Already up to date ({})", status.version());
        }

        Ok(())
}

#[cfg(test)]
mod tests {
        use super::*;

        fn parse(args: &[&str]) -> Config {
                Config::try_parse_from(std::iter::once("bfiles").chain(args.iter().copied())).unwrap()
        }

        #[test]
        fn parses_short_flags() {
                let config = parse(&["-p", ".", "-t", "10", "-d", "2", "-e", "rayon"]);

                assert_eq!(config.command, None);
                assert_eq!(config.path, Some(PathBuf::from(".")));
                assert_eq!(config.top_n, 10);
                assert_eq!(config.max_depth, 2);
                assert_eq!(config.engine, EngineType::Rayon);
        }

        #[test]
        fn parses_long_flags_unchanged() {
                let config = parse(&["--path", "./demo", "--top", "3"]);

                assert_eq!(config.path, Some(PathBuf::from("./demo")));
                assert_eq!(config.top_n, 3);
                assert_eq!(config.max_depth, usize::MAX);
                assert_eq!(config.engine, EngineType::Crossbeam);
        }

        #[test]
        fn parses_exclude_and_include_cloud() {
                let config = parse(&["-p", ".", "--exclude", "/a", "--exclude", "/b", "--include-cloud"]);

                assert!(config.include_cloud);
                assert_eq!(config.exclude, vec![PathBuf::from("/a"), PathBuf::from("/b")]);

                let defaults = parse(&["-p", "."]);
                assert!(!defaults.include_cloud);
                assert!(defaults.exclude.is_empty());
        }

        #[test]
        fn parses_upgrade_subcommand() {
                let config = parse(&["upgrade"]);

                assert_eq!(config.command, Some(Command::Upgrade));
                assert_eq!(config.path, None);
        }

        #[test]
        fn verifies_arg_definitions() {
                use clap::CommandFactory;
                Config::command().debug_assert();
        }
}
