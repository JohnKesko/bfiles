use std::path::PathBuf;

use crate::formatting::{convert_bytes, format_unit};

pub fn print_help() {
    println!(
        r#"Usage:
  bfiles --path <PATH> [OPTIONS]

Options:
  --path <PATH>        Path to analyze (required)
  --engine <ENGINE>    Traversal engine: rayon or crossbeam (default: crossbeam)
  --max_depth <N>      Limit traversal depth (default: unlimited)
  --top <N>            Show top N largest directories (default: 10)
  -h, --help           Show this help message

Examples:
  bfiles --path .
  bfiles --path . --engine rayon --top 20
  bfiles --path /Users --max_depth 2 --top 10
"#
    );
}

pub fn print_top(entries: &[(PathBuf, u64)]) {
    println!("\nTop {} largest directories:\n", entries.len());

    let max_path_width = entries.iter().map(|(path, _)| path.display().to_string().len()).max().unwrap_or(0);

    for (i, (path, size)) in entries.iter().enumerate() {
        let (val, unit) = convert_bytes(*size);
        let path_str = path.display().to_string();

        println!(
            "{:>2}. {:<width$} {:>8.2} {}",
            i + 1,
            path_str,
            val,
            format_unit(&unit),
            width = max_path_width
        );
    }
}
