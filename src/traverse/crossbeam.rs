use super::{Results, TraversalEngine};
use crossbeam::channel::{self, RecvTimeoutError};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

pub struct CrossbeamTraversal;

impl TraversalEngine for CrossbeamTraversal {
    fn run(&self, root: &Path, max_depth: usize, counter: &AtomicU64, results: &Results) {
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

                        let mut total = 0;

                        let read_dir = match fs::read_dir(&path) {
                            Ok(rd) => rd,
                            Err(_) => {
                                pending.fetch_sub(1, Ordering::AcqRel);
                                continue;
                            }
                        };

                        for entry in read_dir.filter_map(Result::ok) {
                            counter.fetch_add(1, Ordering::Relaxed);

                            let file_type = match entry.file_type() {
                                Ok(ft) => ft,
                                Err(_) => continue,
                            };

                            if file_type.is_symlink() {
                                continue;
                            }

                            if file_type.is_dir() {
                                pending.fetch_add(1, Ordering::AcqRel);
                                if tx.send((entry.path(), depth + 1)).is_err() {
                                    pending.fetch_sub(1, Ordering::AcqRel);
                                    break;
                                }
                            } else if file_type.is_file() {
                                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
                            }
                        }

                        // Store file-only size for this directory
                        results.insert(path.clone(), total);

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
