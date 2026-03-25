use std::path::PathBuf;

use dashmap::DashMap;

#[cfg(target_os = "macos")]
pub const BASE: f64 = 1000.0;

#[cfg(target_os = "windows")]
pub const BASE: f64 = 1024.0;

pub const KB: f64 = BASE;
pub const MB: f64 = BASE * BASE;
pub const GB: f64 = BASE * MB;
pub const TB: f64 = BASE * GB;

#[derive(Debug)]
pub enum ByteFormat {
    Bytes,
    Kb,
    Mb,
    Gb,
    Tb,
}

pub fn convert_bytes(size: u64) -> (f64, ByteFormat) {
    let s = size as f64;

    if s >= TB {
        (s / TB, ByteFormat::Tb)
    } else if s >= GB {
        (s / GB, ByteFormat::Gb)
    } else if s >= MB {
        (s / MB, ByteFormat::Mb)
    } else if s >= KB {
        (s / KB, ByteFormat::Kb)
    } else {
        (s, ByteFormat::Bytes)
    }
}

pub fn format_unit(unit: &ByteFormat) -> &'static str {
    match unit {
        ByteFormat::Bytes => "B",
        ByteFormat::Kb => "KB",
        ByteFormat::Mb => "MB",
        ByteFormat::Gb => "GB",
        ByteFormat::Tb => "TB",
    }
}

pub fn top_n_entries(results: &DashMap<PathBuf, u64>, n: usize) -> Vec<(PathBuf, u64)> {
    let mut entries: Vec<_> = results.iter().map(|e| (e.key().clone(), *e.value())).collect();
    let limit = n.min(entries.len());

    entries.select_nth_unstable_by(limit, |a, b| b.1.cmp(&a.1));
    entries[..limit].sort_by(|a, b| b.1.cmp(&a.1));
    entries.into_iter().take(limit).collect()
}
