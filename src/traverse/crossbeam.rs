use super::scan;
use super::{Results, TraversalEngine};
use crossbeam::channel::{self, RecvTimeoutError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

pub struct CrossbeamTraversal;

impl TraversalEngine for CrossbeamTraversal {
    fn run(
        &self, root: &Path, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64,
        excludes: &[PathBuf], results: &Results,
    ) {
        let (tx, rx) = channel::unbounded::<(PathBuf, usize)>();

        // Track outstanding work (directories)
        let pending = AtomicUsize::new(1);

        tx.send((root.to_path_buf(), 0)).unwrap();

        let num_workers = num_cpus::get();

        crossbeam::scope(|s| {
            for _ in 0..num_workers {
                let rx = rx.clone();
                let tx = tx.clone();

                let counter = counter;
                let errors = errors;
                let excludes = excludes;
                let results = results;
                let pending = &pending;

                s.spawn(move |_| {
                    loop {
                        // Use timeout so we can re-check pending and exit clean
                        let (path, depth) = match rx.recv_timeout(Duration::from_millis(5)) {
                            Ok(v) => v,
                            Err(RecvTimeoutError::Timeout) => {
                                if pending.load(Ordering::Acquire) == 0 {
                                    break;
                                }
                                continue;
                            }
                            Err(RecvTimeoutError::Disconnected) => break,
                        };

                        if depth > max_depth {
                            pending.fetch_sub(1, Ordering::AcqRel);
                            continue;
                        }

                        let entries = match scan::read_dir_entries(&path) {
                            Ok(entries) => entries,
                            Err(_) => {
                                errors.fetch_add(1, Ordering::Relaxed);
                                pending.fetch_sub(1, Ordering::AcqRel);
                                continue;
                            }
                        };

                        let mut total = 0;
                        let local_count = entries.len() as u64;

                        for entry in entries {
                            if entry.is_symlink {
                                continue;
                            }

                            if entry.is_dir {
                                let child = path.join(&entry.name);

                                if excludes.iter().any(|excluded| excluded == &child) {
                                    continue;
                                }

                                pending.fetch_add(1, Ordering::AcqRel);
                                if tx.send((child, depth + 1)).is_err() {
                                    pending.fetch_sub(1, Ordering::AcqRel);
                                    break;
                                }
                            } else {
                                total += entry.size;
                            }
                        }

                        // Store file-only size for this directory
                        results.insert(path.clone(), total);

                        // One atomic update per directory instead of per entry,
                        // to avoid hammering the shared counter's cache line.
                        counter.fetch_add(local_count, Ordering::Relaxed);

                        // Mark this directory as processed
                        pending.fetch_sub(1, Ordering::AcqRel);
                    }
                });
            }
        })
        .unwrap();

        // -------- Aggregation (bottom-up) --------

        // Precompute depth to avoid repeated work
        let mut paths: Vec<(PathBuf, usize)> =
            results.iter().map(|e| (e.key().clone(), e.key().components().count())).collect();

        // Sort deepest paths first
        paths.sort_by(|a, b| b.1.cmp(&a.1));

        for (path, _) in &paths {
            let size = results.get(path).map(|e| *e.value()).unwrap_or(0);

            if let Some(parent) = path.parent() {
                if let Some(mut parent_entry) = results.get_mut(parent) {
                    *parent_entry += size;
                }
            }
        }
    }
}
