use super::scan;
use super::tree::{DirTree, ROOT};
use super::{TraversalEngine, is_excluded};
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct RayonTraversal;

impl TraversalEngine for RayonTraversal {
        fn run(&self, path: &Path, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64, excludes: &[PathBuf]) -> DirTree {
                let tree = Mutex::new(DirTree::new(path));

                scan_dir(path, ROOT, 0, max_depth, counter, errors, excludes, &tree);

                let mut tree = tree.into_inner().unwrap();
                tree.aggregate();
                tree
        }
}

#[allow(clippy::too_many_arguments)]
fn scan_dir(query: &Path, id: u32, depth: usize, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64, excludes: &[PathBuf], tree: &Mutex<DirTree>) {
        if depth > max_depth {
                return;
        }

        let entries = match scan::read_dir_entries(query) {
                Ok(entries) => entries,
                Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        return;
                }
        };

        // One atomic update per directory instead of per entry.
        counter.fetch_add(entries.len() as u64, Ordering::Relaxed);

        let mut total = 0;
        let mut child_names = Vec::new();

        for entry in entries {
                if entry.is_symlink {
                        continue;
                }

                if entry.is_dir {
                        if !is_excluded(excludes, query, &entry.name) {
                                child_names.push(entry.name);
                        }
                } else {
                        total += entry.size;
                }
        }

        let children = {
                let mut tree = tree.lock().unwrap();
                tree.set_size(id, total);
                tree.add_children(id, child_names.iter())
        };

        child_names.into_par_iter().zip(children.into_par_iter()).for_each(|(name, child_id)| {
                let child_path = query.join(name);
                scan_dir(&child_path, child_id, depth + 1, max_depth, counter, errors, excludes, tree);
        });
}
