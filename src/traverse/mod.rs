pub mod crossbeam;
pub mod rayon;
pub mod scan;

use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

pub type Results = DashMap<PathBuf, u64>;

pub trait TraversalEngine {
    fn run(&self, path: &std::path::Path, max_depth: usize, counter: &AtomicU64, results: &Results);
}
