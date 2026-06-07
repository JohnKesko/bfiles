use crate::traverse::crossbeam::CrossbeamTraversal;
use crate::traverse::{Results, TraversalEngine, rayon::RayonTraversal};
use clap::{Parser, ValueEnum};
use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{io, path::PathBuf};

use indicatif::ProgressBar;

use crate::print::print_tree;
use crate::progress::start_progress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EngineType {
    Rayon,
    Crossbeam,
}

#[derive(Parser)]
#[command(
    name = "bfiles",
    version,
    about = "Fast parallel directory size analyzer",
    after_help = "Examples:\n  bfiles -p .\n  bfiles -p . -e rayon -t 20\n  bfiles --path ./my-folder --max_depth 2 --top 10"
)]
pub struct Config {
    /// Path to analyze
    #[arg(short, long)]
    pub path: PathBuf,

    /// Traversal engine
    #[arg(short, long, value_enum, default_value = "crossbeam")]
    pub engine: EngineType,

    /// Limit traversal depth [default: unlimited]
    #[arg(short = 'd', long = "max_depth", default_value_t = usize::MAX, hide_default_value = true)]
    pub max_depth: usize,

    /// Show top N root groups
    #[arg(short = 't', long = "top", default_value_t = 10)]
    pub top_n: usize,
}

pub fn run(pb: ProgressBar) -> Result<(), io::Error> {
    let config = Config::parse();

    let engine: Box<dyn TraversalEngine> = match config.engine {
        EngineType::Rayon => Box::new(RayonTraversal),
        EngineType::Crossbeam => Box::new(CrossbeamTraversal),
    };

    let counter = Arc::new(AtomicU64::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let results: Arc<Results> = Arc::new(DashMap::new());

    let progress_handle = start_progress(&pb, Arc::clone(&counter), Arc::clone(&done));

    let start = std::time::Instant::now();
    engine.run(&config.path, config.max_depth, &counter, &results);
    let duration = start.elapsed();

    done.store(true, Ordering::Relaxed);
    progress_handle.join().ok();
    pb.finish_and_clear();

    let items = counter.load(Ordering::Relaxed);
    println!("Traversed {} items in {:.2?}", items, duration);

    print_tree(&config.path, results.as_ref(), config.top_n);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Config {
        Config::try_parse_from(std::iter::once("bfiles").chain(args.iter().copied())).unwrap()
    }

    #[test]
    fn parses_short_flags() {
        let config = parse(&["-p", ".", "-t", "10", "-d", "2", "-e", "rayon"]);

        assert_eq!(config.path, PathBuf::from("."));
        assert_eq!(config.top_n, 10);
        assert_eq!(config.max_depth, 2);
        assert_eq!(config.engine, EngineType::Rayon);
    }

    #[test]
    fn parses_long_flags_unchanged() {
        let config = parse(&["--path", "./demo", "--top", "3"]);

        assert_eq!(config.path, PathBuf::from("./demo"));
        assert_eq!(config.top_n, 3);
        assert_eq!(config.max_depth, usize::MAX);
        assert_eq!(config.engine, EngineType::Crossbeam);
    }

    #[test]
    fn verifies_arg_definitions() {
        use clap::CommandFactory;
        Config::command().debug_assert();
    }
}
