use std::ffi::OsStr;
use std::ops::Range;
use std::path::{Path, PathBuf};

pub const ROOT: u32 = 0;
const NO_PARENT: u32 = u32::MAX;

/// Column-oriented directory tree. Each discovered directory stores only its
/// parent's index and its own name, and all names live in one shared byte
/// arena, so memory stays proportional to the sum of name lengths instead of
/// the sum of full-path lengths. Children are always appended after their
/// parent, so a reverse index scan is a valid bottom-up traversal order.
#[derive(Debug)]
pub struct DirTree {
    parents: Vec<u32>,
    /// All names back to back; node `i` owns `name_bytes[name_ends[i-1]..name_ends[i]]`.
    name_bytes: Vec<u8>,
    name_ends: Vec<u32>,
    sizes: Vec<u64>,
}

impl DirTree {
    /// Node 0 is the scanned root; its name holds the full root path.
    pub fn new(root: &Path) -> Self {
        let root_bytes = root.as_os_str().as_encoded_bytes();

        Self {
            parents: vec![NO_PARENT],
            name_bytes: root_bytes.to_vec(),
            name_ends: vec![root_bytes.len() as u32],
            sizes: vec![0],
        }
    }

    pub fn len(&self) -> usize {
        self.parents.len()
    }

    pub fn parent(&self, id: u32) -> Option<u32> {
        let parent = self.parents[id as usize];
        (parent != NO_PARENT).then_some(parent)
    }

    pub fn name(&self, id: u32) -> &OsStr {
        let id = id as usize;
        let start = if id == 0 { 0 } else { self.name_ends[id - 1] as usize };
        let bytes = &self.name_bytes[start..self.name_ends[id] as usize];

        // SAFETY: the bytes were produced by `OsStr::as_encoded_bytes` in this
        // process and sliced on the same boundaries they were stored with.
        unsafe { OsStr::from_encoded_bytes_unchecked(bytes) }
    }

    pub fn size(&self, id: u32) -> u64 {
        self.sizes[id as usize]
    }

    pub fn set_size(&mut self, id: u32, size: u64) {
        self.sizes[id as usize] = size;
    }

    /// Append one child per name; returns the new ids.
    pub fn add_children<I, S>(&mut self, parent: u32, names: I) -> Range<u32>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let first = self.parents.len();

        for name in names {
            self.name_bytes.extend_from_slice(name.as_ref().as_encoded_bytes());
            self.name_ends.push(self.name_bytes.len() as u32);
            self.parents.push(parent);
            self.sizes.push(0);
        }

        assert!(self.parents.len() <= u32::MAX as usize, "directory count exceeds u32 range");
        assert!(self.name_bytes.len() <= u32::MAX as usize, "name arena exceeds u32 range");

        first as u32..self.parents.len() as u32
    }

    /// Rebuild the absolute path of `id` into `out`, reusing `chain` as scratch.
    pub fn path_of(&self, id: u32, chain: &mut Vec<u32>, out: &mut PathBuf) {
        chain.clear();

        let mut current = id;
        loop {
            chain.push(current);
            match self.parent(current) {
                Some(parent) => current = parent,
                None => break,
            }
        }

        out.clear();
        for &node in chain.iter().rev() {
            out.push(self.name(node));
        }
    }

    /// Path of `id` relative to the root node (root's own name excluded).
    pub fn relative_path_of(&self, id: u32) -> PathBuf {
        let mut chain = Vec::new();

        let mut current = id;
        while current != ROOT {
            chain.push(current);
            current = self.parents[current as usize];
        }

        let mut out = PathBuf::new();
        for &node in chain.iter().rev() {
            out.push(self.name(node));
        }

        out
    }

    /// Fold every directory's size into its parent. Valid exactly once, after
    /// the scan: children always have higher ids than their parents.
    pub fn aggregate(&mut self) {
        for id in (1..self.parents.len()).rev() {
            let size = self.sizes[id];
            self.sizes[self.parents[id] as usize] += size;
        }
    }

    /// Look up a node by path relative to the root. Test-only helper.
    #[cfg(test)]
    pub fn find(&self, relative: &Path) -> Option<u32> {
        let mut current = ROOT;

        'components: for component in relative.components() {
            let wanted = component.as_os_str();

            for id in 0..self.parents.len() as u32 {
                if self.parents[id as usize] == current && self.name(id) == wanted {
                    current = id;
                    continue 'components;
                }
            }

            return None;
        }

        Some(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn builds_paths_and_aggregates_bottom_up() {
        let mut tree = DirTree::new(Path::new("/scan/root"));

        let children = tree.add_children(ROOT, [OsString::from("a"), OsString::from("b")]);
        assert_eq!(children, 1..3);
        assert_eq!(tree.name(1), OsStr::new("a"));
        assert_eq!(tree.name(2), OsStr::new("b"));

        let grandchildren = tree.add_children(1, [OsString::from("deep")]);
        assert_eq!(grandchildren, 3..4);

        tree.set_size(ROOT, 5);
        tree.set_size(1, 10);
        tree.set_size(2, 20);
        tree.set_size(3, 40);

        let mut chain = Vec::new();
        let mut path = PathBuf::new();
        tree.path_of(3, &mut chain, &mut path);
        assert_eq!(path, PathBuf::from("/scan/root").join("a").join("deep"));
        assert_eq!(tree.relative_path_of(3), PathBuf::from("a").join("deep"));

        tree.aggregate();

        assert_eq!(tree.size(3), 40);
        assert_eq!(tree.size(1), 50, "child folds into parent");
        assert_eq!(tree.size(ROOT), 75, "root holds the grand total");

        assert_eq!(tree.find(&PathBuf::from("a").join("deep")), Some(3));
        assert_eq!(tree.find(Path::new("missing")), None);
    }
}
