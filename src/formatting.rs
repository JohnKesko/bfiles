use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::traverse::tree::{DirTree, ROOT};

#[cfg(target_os = "macos")]
pub const BASE: f64 = 1000.0;

#[cfg(not(target_os = "macos"))]
pub const BASE: f64 = 1024.0;

pub const KB: f64 = BASE;
pub const MB: f64 = BASE * BASE;
pub const GB: f64 = BASE * MB;
pub const TB: f64 = BASE * GB;

#[derive(Debug)]
pub enum ByteFormat {
    Bytes,
    Kb,
    Mb,
    Gb,
    Tb,
}

pub fn convert_bytes(size: u64) -> (f64, ByteFormat) {
    let s = size as f64;

    if s >= TB {
        (s / TB, ByteFormat::Tb)
    } else if s >= GB {
        (s / GB, ByteFormat::Gb)
    } else if s >= MB {
        (s / MB, ByteFormat::Mb)
    } else if s >= KB {
        (s / KB, ByteFormat::Kb)
    } else {
        (s, ByteFormat::Bytes)
    }
}

pub fn format_unit(unit: &ByteFormat) -> &'static str {
    match unit {
        ByteFormat::Bytes => "B",
        ByteFormat::Kb => "KB",
        ByteFormat::Mb => "MB",
        ByteFormat::Gb => "GB",
        ByteFormat::Tb => "TB",
    }
}

pub const DEFAULT_CHILDREN_PER_GROUP: usize = 5;

/// Label for the pseudo-group holding files that sit directly in the scanned root.
pub const ROOT_FILES_LABEL: &str = "(files)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRow {
    pub relative_path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryGroup {
    pub root: PathBuf,
    pub total_size: u64,
    pub children: Vec<DirectoryRow>,
}

#[derive(Debug)]
pub struct TreeSummary {
    /// Size of everything under the scanned root, independent of `top_n` truncation.
    pub total_size: u64,
    pub groups: Vec<DirectoryGroup>,
}

/// Depth-1 ancestor of `id`, memoized. Iterative so pathologically deep trees
/// cannot overflow the stack.
fn anchor_of(tree: &DirTree, id: u32, memo: &mut [u32]) -> u32 {
    let mut chain = Vec::new();
    let mut current = id;

    let anchor = loop {
        if memo[current as usize] != u32::MAX {
            break memo[current as usize];
        }

        let parent = tree.parent(current).expect("anchor_of never receives the root");

        if parent == ROOT {
            break current;
        }

        chain.push(current);
        current = parent;
    };

    memo[id as usize] = anchor;
    for node in chain {
        memo[node as usize] = anchor;
    }

    anchor
}

/// Build the display summary: one group per directory directly under the root,
/// each listing its largest descendants, plus a pseudo-group for loose files.
pub fn summarize_tree(tree: &DirTree, top_n: usize, child_limit: usize) -> TreeSummary {
    if top_n == 0 {
        return TreeSummary { total_size: 0, groups: Vec::new() };
    }

    let node_count = tree.len();
    let total_size = tree.size(ROOT);

    let mut memo = vec![u32::MAX; node_count];
    // Anchor id -> deeper descendants as (size, id); rows stay numeric until
    // after truncation so only the displayed rows allocate path strings.
    let mut grouped = BTreeMap::<u32, Vec<(u64, u32)>>::new();
    let mut direct_children_total = 0u64;

    for id in 1..node_count as u32 {
        let anchor = anchor_of(tree, id, &mut memo);

        if id == anchor {
            grouped.entry(id).or_default();
            direct_children_total += tree.size(id);
        } else {
            grouped.entry(anchor).or_default().push((tree.size(id), id));
        }
    }

    let mut groups: Vec<DirectoryGroup> = grouped
        .into_iter()
        .map(|(anchor, mut rows)| {
            rows.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            rows.truncate(child_limit);

            let mut children: Vec<DirectoryRow> = rows
                .into_iter()
                .map(|(size, id)| DirectoryRow { relative_path: tree.relative_path_of(id), size })
                .collect();
            children
                .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.relative_path.cmp(&b.relative_path)));

            DirectoryGroup {
                root: PathBuf::from(tree.name(anchor)),
                total_size: tree.size(anchor),
                children,
            }
        })
        .collect();

    // Files sitting directly in the scanned root belong to no child directory;
    // surface them as their own pseudo-group instead of dropping them.
    let root_files = total_size.saturating_sub(direct_children_total);
    if root_files > 0 {
        groups.push(DirectoryGroup {
            root: PathBuf::from(ROOT_FILES_LABEL),
            total_size: root_files,
            children: Vec::new(),
        });
    }

    groups.sort_by(|a, b| b.total_size.cmp(&a.total_size).then_with(|| a.root.cmp(&b.root)));
    groups.truncate(top_n);

    TreeSummary { total_size, groups }
}

pub fn calculate_column_width(groups: &[DirectoryGroup]) -> usize {
    let mut width = 0;

    for group in groups {
        width = width.max(group.root.display().to_string().len());

        for child in &group.children {
            width = width.max(format!("- {}", child.relative_path.display()).len());
        }
    }

    width
}

pub fn format_tree_output(summary: &TreeSummary) -> String {
    let groups = &summary.groups[..];
    let total_label = "Total";
    let path_width = calculate_column_width(groups).max(total_label.len());
    let size_width = calculate_size_width(groups).max(format_size(summary.total_size).len());
    let mut lines = Vec::new();

    for (index, group) in groups.iter().enumerate() {
        lines.push(format_row(
            &group.root.display().to_string(),
            group.total_size,
            path_width,
            size_width,
        ));

        for child in &group.children {
            let label = format!("- {}", child.relative_path.display());
            lines.push(format_row(&label, child.size, path_width, size_width));
        }

        if index + 1 < groups.len() {
            lines.push(String::new());
        }
    }

    lines.push(String::new());
    lines.push(format_row(total_label, summary.total_size, path_width, size_width));

    lines.join("\n")
}

fn calculate_size_width(groups: &[DirectoryGroup]) -> usize {
    let mut width = 0;

    for group in groups {
        width = width.max(format_size(group.total_size).len());

        for child in &group.children {
            width = width.max(format_size(child.size).len());
        }
    }

    width
}

fn format_row(label: &str, size: u64, path_width: usize, size_width: usize) -> String {
    let size_str = format_size(size);

    format!(
        "{:<path_width$} {:>size_width$}",
        label,
        size_str,
        path_width = path_width,
        size_width = size_width
    )
}

fn format_size(size: u64) -> String {
    let (value, unit) = convert_bytes(size);
    format!("{value:.2} {}", format_unit(&unit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::Path;

    fn names<const N: usize>(values: [&str; N]) -> [OsString; N] {
        values.map(OsString::from)
    }

    #[test]
    fn groups_descendants_under_their_top_level_directory() {
        let root = PathBuf::from("root").join("projects");
        let mut tree = DirTree::new(&root);

        // Own sizes are set per directory; aggregate() folds them upward so the
        // expected displayed totals match the old fixture values.
        let top = tree.add_children(ROOT, names(["certbuddy", "andreasohlstrom.se", "my plugin"]));
        let (certbuddy, andreas, plugin) = (top.start, top.start + 1, top.start + 2);

        let certbuddy_next = tree.add_children(certbuddy, names([".next"])).start;
        let certbuddy_next_dev = tree.add_children(certbuddy_next, names(["dev"])).start;
        let certbuddy_cache = tree.add_children(certbuddy_next_dev, names(["cache"])).start;

        let trends = tree.add_children(andreas, names(["trends"])).start;
        let trends_next = tree.add_children(trends, names([".next"])).start;
        let trends_next_dev = tree.add_children(trends_next, names(["dev"])).start;

        let plugin_next = tree.add_children(plugin, names([".next"])).start;

        tree.set_size(certbuddy, 10);
        tree.set_size(certbuddy_next, 5);
        tree.set_size(certbuddy_next_dev, 105);
        tree.set_size(certbuddy_cache, 900);
        tree.set_size(andreas, 1_070);
        tree.set_size(trends, 18);
        tree.set_size(trends_next, 43);
        tree.set_size(trends_next_dev, 899);
        tree.set_size(plugin, 50);
        tree.set_size(plugin_next, 650);
        tree.aggregate();

        let summary = summarize_tree(&tree, 3, 2);
        let groups = summary.groups;

        assert_eq!(summary.total_size, 3_750);
        assert_eq!(groups.len(), 3);

        assert_eq!(groups[0].root, PathBuf::from("andreasohlstrom.se"));
        assert_eq!(groups[0].total_size, 2_030);
        assert_eq!(
            groups[0].children.iter().map(|child| child.relative_path.clone()).collect::<Vec<_>>(),
            vec![
                PathBuf::from("andreasohlstrom.se").join("trends"),
                PathBuf::from("andreasohlstrom.se").join("trends").join(".next"),
            ]
        );

        assert_eq!(groups[1].root, PathBuf::from("certbuddy"));
        assert_eq!(groups[1].total_size, 1_020);
        assert_eq!(
            groups[1].children.iter().map(|child| child.relative_path.clone()).collect::<Vec<_>>(),
            vec![
                PathBuf::from("certbuddy").join(".next"),
                PathBuf::from("certbuddy").join(".next").join("dev")
            ]
        );

        assert_eq!(groups[2].root, PathBuf::from("my plugin"));
        assert_eq!(groups[2].total_size, 700);
        assert_eq!(
            groups[2].children[0].relative_path,
            PathBuf::from("my plugin").join(".next")
        );
    }

    #[test]
    fn summarizing_handles_empty_roots_and_direct_children_without_descendants() {
        let empty_tree = DirTree::new(&PathBuf::from("root").join("empty"));
        assert!(summarize_tree(&empty_tree, 10, 5).groups.is_empty());

        let mut tree = DirTree::new(&PathBuf::from("root").join("files"));
        let plugins = tree.add_children(ROOT, names(["plugins"])).start;
        tree.set_size(plugins, 200);
        tree.aggregate();

        let summary = summarize_tree(&tree, 10, 5);

        assert_eq!(summary.total_size, 200);
        assert_eq!(summary.groups.len(), 1);
        assert_eq!(summary.groups[0].root, PathBuf::from("plugins"));
        assert_eq!(summary.groups[0].total_size, 200);
        assert!(summary.groups[0].children.is_empty());
    }

    #[test]
    fn root_files_become_pseudo_group_and_total_covers_everything() {
        let mut tree = DirTree::new(Path::new("root"));
        let sub = tree.add_children(ROOT, names(["sub"])).start;

        tree.set_size(ROOT, 50_000);
        tree.set_size(sub, 100);
        tree.aggregate();

        let summary = summarize_tree(&tree, 10, 5);

        assert_eq!(summary.total_size, 50_100);
        assert_eq!(summary.groups.len(), 2);
        assert_eq!(summary.groups[0].root, PathBuf::from(ROOT_FILES_LABEL));
        assert_eq!(summary.groups[0].total_size, 50_000);
        assert!(summary.groups[0].children.is_empty());
        assert_eq!(summary.groups[1].root, PathBuf::from("sub"));
    }

    #[test]
    fn tree_output_uses_platform_paths() {
        let child_path = PathBuf::from("certbuddy").join(".next");
        let summary = TreeSummary {
            total_size: 1_020,
            groups: vec![DirectoryGroup {
                root: PathBuf::from("certbuddy"),
                total_size: 1_020,
                children: vec![DirectoryRow { relative_path: child_path.clone(), size: 1_010 }],
            }],
        };

        let output = format_tree_output(&summary);

        assert!(output.contains(&PathBuf::from("certbuddy").display().to_string()));
        assert!(output.contains(&format!("- {}", child_path.display())));
        assert!(output.contains("Total"));
    }
}
