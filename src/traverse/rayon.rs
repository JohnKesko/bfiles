use super::scan;
use super::{Results, TraversalEngine};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct RayonTraversal;

impl TraversalEngine for RayonTraversal {
    fn run(
        &self, path: &Path, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64,
        excludes: &[PathBuf], results: &Results,
    ) {
        file_or_dir(path, 0, max_depth, counter, errors, excludes, results);
    }
}

fn file_or_dir(
    query: &Path, depth: usize, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64,
    excludes: &[PathBuf], results: &Results,
) -> u64 {
    if depth > max_depth {
        return 0;
    }

    let entries = match scan::read_dir_entries(query) {
        Ok(entries) => entries,
        Err(_) => {
            errors.fetch_add(1, Ordering::Relaxed);
            return 0;
        }
    };

    if entries.is_empty() {
        results.insert(query.to_path_buf(), 0);
        return 0;
    }

    // One atomic update per directory instead of per entry.
    counter.fetch_add(entries.len() as u64, Ordering::Relaxed);

    let total = entries
        .par_iter()
        .map(|entry| {
            if entry.is_symlink {
                return 0;
            }

            if entry.is_dir {
                let child = query.join(&entry.name);

                if excludes.iter().any(|excluded| excluded == &child) {
                    return 0;
                }

                file_or_dir(&child, depth + 1, max_depth, counter, errors, excludes, results)
            } else {
                entry.size
            }
        })
        .sum();

    results.insert(query.to_path_buf(), total);

    total
}
