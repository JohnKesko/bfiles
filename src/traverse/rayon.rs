use super::{Results, TraversalEngine};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct RayonTraversal;

impl TraversalEngine for RayonTraversal {
    fn run(&self, path: &Path, max_depth: usize, counter: &AtomicU64, results: &Results) {
        file_or_dir(path, 0, max_depth, counter, results);
    }
}

fn file_or_dir(query: &Path, depth: usize, max_depth: usize, counter: &AtomicU64, results: &Results) -> u64 {
    if depth > max_depth {
        return 0;
    }

    let mut read_dir = match fs::read_dir(query) {
        Ok(rd) => rd,
        Err(_) => return 0,
    };

    let first = match read_dir.next() {
        Some(Ok(entry)) => entry,
        _ => {
            results.insert(query.to_path_buf(), 0);
            return 0;
        }
    };

    let entries: Vec<_> = std::iter::once(first).chain(read_dir.filter_map(Result::ok)).collect();

    let total = entries
        .par_iter()
        .map(|entry| {
            // Count entries
            counter.fetch_add(1, Ordering::Relaxed);

            let path = entry.path();

            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => return 0,
            };

            if file_type.is_symlink() {
                return 0;
            }

            if file_type.is_dir() {
                file_or_dir(&path, depth + 1, max_depth, counter, results)
            } else if file_type.is_file() {
                entry.metadata().map(|m| m.len()).unwrap_or(0)
            } else {
                0
            }
        })
        .sum();

    results.insert(query.to_path_buf(), total);

    total
}
