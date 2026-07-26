use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dashmap::DashMap;

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

#[derive(Debug, Default)]
struct GroupAccumulator {
    total_size: Option<u64>,
    children: Vec<DirectoryRow>,
}

pub fn get_relative_path(root: &Path, path: &Path) -> Option<PathBuf> {
    path.strip_prefix(root).ok().map(Path::to_path_buf)
}

pub fn get_root_group(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative_path = get_relative_path(root, path)?;
    let first = relative_path.components().next()?;

    Some(PathBuf::from(first.as_os_str()))
}

/// Label for the pseudo-group holding files that sit directly in the scanned root.
pub const ROOT_FILES_LABEL: &str = "(files)";

#[derive(Debug)]
pub struct TreeSummary {
    /// Size of everything under the scanned root, independent of `top_n` truncation.
    pub total_size: u64,
    pub groups: Vec<DirectoryGroup>,
}

pub fn group_entries_by_root(
    results: &DashMap<PathBuf, u64>, root: &Path, top_n: usize, child_limit: usize,
) -> TreeSummary {
    if top_n == 0 {
        return TreeSummary { total_size: 0, groups: Vec::new() };
    }

    let mut grouped = BTreeMap::<PathBuf, GroupAccumulator>::new();
    let mut root_total = 0u64;
    let mut direct_children_total = 0u64;

    for entry in results.iter() {
        let path = entry.key();
        let size = *entry.value();

        let Some(relative_path) = get_relative_path(root, path) else {
            continue;
        };

        if relative_path.as_os_str().is_empty() {
            root_total = size;
            continue;
        }

        let Some(root_group) = get_root_group(root, path) else {
            continue;
        };

        let group = grouped.entry(root_group).or_default();

        if relative_path.components().count() == 1 {
            group.total_size = Some(size);
            direct_children_total += size;
        } else {
            group.children.push(DirectoryRow { relative_path, size });
        }
    }

    let mut groups: Vec<_> = grouped
        .into_iter()
        .map(|(root, mut group)| {
            group
                .children
                .sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.relative_path.cmp(&b.relative_path)));
            group.children.truncate(child_limit);

            DirectoryGroup {
                total_size: group
                    .total_size
                    .unwrap_or_else(|| group.children.iter().map(|child| child.size).max().unwrap_or(0)),
                root,
                children: group.children,
            }
        })
        .collect();

    // Files sitting directly in the scanned root belong to no child directory;
    // surface them as their own pseudo-group instead of dropping them.
    let root_files = root_total.saturating_sub(direct_children_total);
    if root_files > 0 {
        groups.push(DirectoryGroup {
            root: PathBuf::from(ROOT_FILES_LABEL),
            total_size: root_files,
            children: Vec::new(),
        });
    }

    groups.sort_by(|a, b| b.total_size.cmp(&a.total_size).then_with(|| a.root.cmp(&b.root)));
    groups.truncate(top_n);

    TreeSummary { total_size: root_total, groups }
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

    fn build_results(entries: &[(PathBuf, u64)]) -> DashMap<PathBuf, u64> {
        let results = DashMap::new();

        for (path, size) in entries {
            results.insert(path.clone(), *size);
        }

        results
    }

    #[test]
    fn groups_entries_by_first_relative_component() {
        let root = PathBuf::from("root").join("projects");
        let certbuddy = root.join("certbuddy");
        let certbuddy_next = certbuddy.join(".next");
        let certbuddy_next_dev = certbuddy_next.join("dev");
        let andreas = root.join("andreasohlstrom.se");
        let andreas_trends = andreas.join("trends");
        let andreas_trends_next = andreas_trends.join(".next");
        let plugin = root.join("my plugin");

        let results = build_results(&[
            (root.clone(), 0),
            (certbuddy.clone(), 1_020),
            (certbuddy_next.clone(), 1_010),
            (certbuddy_next_dev.clone(), 1_005),
            (certbuddy_next_dev.join("cache"), 900),
            (andreas.clone(), 2_030),
            (andreas_trends.clone(), 960),
            (andreas_trends_next.clone(), 942),
            (andreas_trends_next.join("dev"), 899),
            (plugin.clone(), 700),
            (plugin.join(".next"), 650),
        ]);

        let groups = group_entries_by_root(&results, &root, 3, 2).groups;

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
        assert_eq!(
            groups[1].children.iter().map(|child| child.relative_path.clone()).collect::<Vec<_>>(),
            vec![
                PathBuf::from("certbuddy").join(".next"),
                PathBuf::from("certbuddy").join(".next").join("dev")
            ]
        );

        assert_eq!(groups[2].root, PathBuf::from("my plugin"));
        assert_eq!(
            groups[2].children[0].relative_path,
            PathBuf::from("my plugin").join(".next")
        );
    }

    #[test]
    fn relative_path_helpers_handle_root_and_spaces() {
        let root = PathBuf::from("root").join("path");
        let child = root.join("with spaces").join(".next");

        assert_eq!(get_relative_path(&root, &root), Some(PathBuf::new()));
        assert_eq!(get_root_group(&root, &root), None);
        assert_eq!(
            get_relative_path(&root, &child),
            Some(PathBuf::from("with spaces").join(".next"))
        );
        assert_eq!(get_root_group(&root, &child), Some(PathBuf::from("with spaces")));
    }

    #[test]
    fn grouping_handles_empty_roots_and_direct_children_without_descendants() {
        let empty_root = PathBuf::from("root").join("empty");
        let empty_results = build_results(&[(empty_root.clone(), 0)]);

        assert!(group_entries_by_root(&empty_results, &empty_root, 10, 5).groups.is_empty());

        let root = PathBuf::from("root").join("files");
        let results = build_results(&[
            (root.clone(), 200),
            (root.join("plugins"), 200),
        ]);
        let summary = group_entries_by_root(&results, &root, 10, 5);
        let groups = summary.groups;

        assert_eq!(summary.total_size, 200);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].root, PathBuf::from("plugins"));
        assert_eq!(groups[0].total_size, 200);
        assert!(groups[0].children.is_empty());
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

    #[test]
    fn root_files_become_pseudo_group_and_total_covers_everything() {
        let root = PathBuf::from("root");
        let results = build_results(&[
            // Root entry holds the grand total: 50_000 of loose files + sub's 100.
            (root.clone(), 50_100),
            (root.join("sub"), 100),
        ]);

        let summary = group_entries_by_root(&results, &root, 10, 5);

        assert_eq!(summary.total_size, 50_100);
        assert_eq!(summary.groups.len(), 2);
        assert_eq!(summary.groups[0].root, PathBuf::from(ROOT_FILES_LABEL));
        assert_eq!(summary.groups[0].total_size, 50_000);
        assert!(summary.groups[0].children.is_empty());
        assert_eq!(summary.groups[1].root, PathBuf::from("sub"));
    }
}
