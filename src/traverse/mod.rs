pub mod crossbeam;
pub mod rayon;
pub mod scan;
pub mod tree;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use tree::DirTree;

pub trait TraversalEngine {
    /// Scans `path` and returns the aggregated directory tree. `errors` counts
    /// directories that could not be read; their contents are missing from the
    /// tree, so callers should warn when it is non-zero. Directories matching
    /// an entry in `excludes` are not descended into.
    fn run(
        &self, path: &Path, max_depth: usize, counter: &AtomicU64, errors: &AtomicU64,
        excludes: &[PathBuf],
    ) -> DirTree;
}

/// True when `parent`/`name` is one of the excluded paths. Compares components
/// so no joined path has to be allocated per directory.
pub(crate) fn is_excluded(excludes: &[PathBuf], parent: &Path, name: &OsStr) -> bool {
    excludes
        .iter()
        .any(|excluded| excluded.file_name() == Some(name) && excluded.parent() == Some(parent))
}

#[cfg(test)]
mod tests {
    use super::tree::ROOT;
    use super::*;
    use std::fs;
    use std::sync::atomic::Ordering;

    fn run_engines(base: &Path, excludes: &[PathBuf], check: impl Fn(&DirTree, u64)) {
        let engines: [&dyn TraversalEngine; 2] =
            [&crossbeam::CrossbeamTraversal, &rayon::RayonTraversal];

        for engine in engines {
            let counter = AtomicU64::new(0);
            let errors = AtomicU64::new(0);

            let tree = engine.run(base, usize::MAX, &counter, &errors, excludes);

            check(&tree, errors.load(Ordering::Relaxed));
        }
    }

    #[cfg(unix)]
    #[test]
    fn engines_count_unreadable_directories() {
        use std::os::unix::fs::PermissionsExt;

        let base = std::env::temp_dir().join(format!("bfiles-locked-test-{}", std::process::id()));
        let locked = base.join("locked");

        fs::create_dir_all(&locked).unwrap();
        fs::write(base.join("visible.bin"), vec![0u8; 2048]).unwrap();
        fs::write(locked.join("hidden.bin"), vec![0u8; 1024]).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        run_engines(&base, &[], |tree, errors| {
            assert_eq!(errors, 1, "unreadable directory must be counted");
            assert_eq!(tree.size(ROOT), 2048, "only readable bytes are measured");
        });

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

        run_engines(&base, &excludes, |tree, errors| {
            assert_eq!(errors, 0);
            assert_eq!(tree.size(ROOT), 100);
            assert!(tree.find(Path::new("keep")).is_some());
            assert!(tree.find(Path::new("skip")).is_none(), "excluded directory must not be scanned");
        });

        fs::remove_dir_all(&base).unwrap();
    }
}
