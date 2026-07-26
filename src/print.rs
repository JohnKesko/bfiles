use crate::formatting::{DEFAULT_CHILDREN_PER_GROUP, format_tree_output, summarize_tree};
use crate::traverse::tree::DirTree;

pub fn print_tree(tree: &DirTree, top_n: usize) {
    if top_n == 0 {
        return;
    }

    let summary = summarize_tree(tree, top_n, DEFAULT_CHILDREN_PER_GROUP);

    println!();

    if summary.groups.is_empty() {
        println!("No child directories found.");
        return;
    }

    println!("{}", format_tree_output(&summary));
}
