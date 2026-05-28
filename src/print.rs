use std::path::Path;

use crate::formatting::{DEFAULT_CHILDREN_PER_GROUP, format_tree_output, group_entries_by_root};
use crate::traverse::Results;

pub fn print_help() {
    println!(
        r#"Usage:
  bfiles --path <PATH> [OPTIONS]
  bfiles -p <PATH> [OPTIONS]

Options:
  -p, --path <PATH>        Path to analyze (required)
  -e, --engine <ENGINE>    Traversal engine: rayon or crossbeam (default: crossbeam)
  -d, --max_depth <N>      Limit traversal depth (default: unlimited)
  -t, --top <N>            Show top N root groups (default: 10)
  -h, --help               Show this help message

Examples:
  bfiles -p .
  bfiles -p . -e rayon -t 20
  bfiles --path ./my-folder --max_depth 2 --top 10
"#
    );
}

pub fn print_tree(root: &Path, results: &Results, top_n: usize) {
    if top_n == 0 {
        return;
    }

    let groups = group_entries_by_root(results, root, top_n, DEFAULT_CHILDREN_PER_GROUP);

    println!();

    if groups.is_empty() {
        println!("No child directories found.");
        return;
    }

    println!("{}", format_tree_output(&groups));
}
