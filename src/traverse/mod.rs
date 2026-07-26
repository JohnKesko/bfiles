pub mod crossbeam;
pub mod rayon;
pub mod scan;

use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

pub type Results = DashMap<PathBuf, u64>;

pub trait TraversalEngine {
    /// `errors` counts directories that could not be read; their contents are
    /// missing from `results`, so callers should warn when it is non-zero.
    /// Directories whose full path equals an entry in `excludes` are not descended into.
    fn run(
        &self, path: &std::path::Path, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64,
        excludes: &[PathBuf], results: &Results,
    );
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::Ordering;

    #[test]
    fn engines_count_unreadable_directories() {
        let base = std::env::temp_dir().join(format!("bfiles-locked-test-{}", std::process::id()));
        let locked = base.join("locked");

        fs::create_dir_all(&locked).unwrap();
        fs::write(base.join("visible.bin"), vec![0u8; 2048]).unwrap();
        fs::write(locked.join("hidden.bin"), vec![0u8; 1024]).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let engines: [&dyn TraversalEngine; 2] =
            [&crossbeam::CrossbeamTraversal, &rayon::RayonTraversal];

        for engine in engines {
            let counter = AtomicU64::new(0);
            let errors = AtomicU64::new(0);
            let results: Results = DashMap::new();

            engine.run(&base, usize::MAX, &counter, &errors, &[], &results);

            assert_eq!(errors.load(Ordering::Relaxed), 1, "unreadable directory must be counted");
            assert_eq!(results.get(&base).map(|e| *e.value()), Some(2048));
        }

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn engines_skip_excluded_directories() {
        let base = std::env::temp_dir().join(format!("bfiles-exclude-test-{}", std::process::id()));
        let keep = base.join("keep");
        let skip = base.join("skip");

        fs::create_dir_all(&keep).unwrap();
        fs::create_dir_all(&skip).unwrap();
        fs::write(keep.join("counted.bin"), vec![0u8; 100]).unwrap();
        fs::write(skip.join("ignored.bin"), vec![0u8; 999]).unwrap();

        let excludes = vec![skip.clone()];
        let engines: [&dyn TraversalEngine; 2] =
            [&crossbeam::CrossbeamTraversal, &rayon::RayonTraversal];

        for engine in engines {
            let counter = AtomicU64::new(0);
            let errors = AtomicU64::new(0);
            let results: Results = DashMap::new();

            engine.run(&base, usize::MAX, &counter, &errors, &excludes, &results);

            assert_eq!(errors.load(Ordering::Relaxed), 0);
            assert_eq!(results.get(&base).map(|e| *e.value()), Some(100));
            assert!(!results.contains_key(&skip), "excluded directory must not be scanned");
        }

        fs::remove_dir_all(&base).unwrap();
    }
}
