use super::scan;
use super::tree::{DirTree, ROOT};
use super::{TraversalEngine, is_excluded};
use crossbeam::channel::{self, RecvTimeoutError};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

pub struct CrossbeamTraversal;

impl TraversalEngine for CrossbeamTraversal {
    fn run(
        &self, root: &Path, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64,
        excludes: &[PathBuf],
    ) -> DirTree {
        // Queue items are (node id, depth): 8 bytes each, so even a queue of
        // millions of pending directories stays small. Paths are rebuilt from
        // the tree on dequeue instead of being stored per work item.
        let (tx, rx) = channel::unbounded::<(u32, usize)>();

        let tree = Mutex::new(DirTree::new(root));

        // Track outstanding work (directories)
        let pending = AtomicUsize::new(1);

        tx.send((ROOT, 0)).unwrap();

        let num_workers = num_cpus::get();

        crossbeam::scope(|s| {
            for _ in 0..num_workers {
                let rx = rx.clone();
                let tx = tx.clone();

                let counter = counter;
                let errors = errors;
                let excludes = excludes;
                let tree = &tree;
                let pending = &pending;

                s.spawn(move |_| {
                    // Per-worker scratch buffers, reused across directories.
                    let mut chain = Vec::new();
                    let mut path = PathBuf::new();
                    let mut child_names = Vec::new();

                    loop {
                        // Use timeout so we can re-check pending and exit clean
                        let (id, depth) = match rx.recv_timeout(Duration::from_millis(5)) {
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

                        tree.lock().unwrap().path_of(id, &mut chain, &mut path);

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

                        child_names.clear();

                        for entry in entries {
                            if entry.is_symlink {
                                continue;
                            }

                            if entry.is_dir {
                                if !is_excluded(excludes, &path, &entry.name) {
                                    child_names.push(entry.name);
                                }
                            } else {
                                total += entry.size;
                            }
                        }

                        // One lock per directory: record its file-only size and
                        // register all of its children.
                        let children = {
                            let mut tree = tree.lock().unwrap();
                            tree.set_size(id, total);
                            tree.add_children(id, child_names.drain(..))
                        };

                        for child in children {
                            pending.fetch_add(1, Ordering::AcqRel);
                            if tx.send((child, depth + 1)).is_err() {
                                pending.fetch_sub(1, Ordering::AcqRel);
                                break;
                            }
                        }

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

        let mut tree = tree.into_inner().unwrap();
        tree.aggregate();
        tree
    }
}
