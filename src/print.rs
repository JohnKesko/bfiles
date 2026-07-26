use std::path::Path;

use crate::formatting::{DEFAULT_CHILDREN_PER_GROUP, format_tree_output, group_entries_by_root};
use crate::traverse::Results;

pub fn print_tree(root: &Path, results: &Results, top_n: usize) {
    if top_n == 0 {
        return;
    }

    let summary = group_entries_by_root(results, root, top_n, DEFAULT_CHILDREN_PER_GROUP);

    println!();

    if summary.groups.is_empty() {
        println!("No child directories found.");
        return;
    }

    println!("{}", format_tree_output(&summary));
}
