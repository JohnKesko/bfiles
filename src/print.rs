use crate::formatting::{DEFAULT_CHILDREN_PER_GROUP, format_detailed_output, format_summary_table, summarize_tree};
use crate::traverse::tree::DirTree;

pub fn print_tree(tree: &DirTree, top_n: usize, details: bool) {
        if top_n == 0 {
                return;
        }

        let summary = summarize_tree(tree, top_n, DEFAULT_CHILDREN_PER_GROUP);

        println!();

        if summary.groups.is_empty() {
                println!("No child directories found.");
                return;
        }

        if details {
                println!("{}", format_detailed_output(&summary));
        } else {
                println!("{}", format_summary_table(&summary));
        }
}
