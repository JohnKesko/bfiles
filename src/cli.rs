use crate::traverse::crossbeam::CrossbeamTraversal;
use crate::traverse::{Results, TraversalEngine, rayon::RayonTraversal};
use dashmap::DashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::{env, io, path::PathBuf, process::exit};

use indicatif::ProgressBar;

use crate::print::{self, print_help};
use crate::progress::start_progress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineType {
    Rayon,
    Crossbeam,
}

pub struct Config {
    pub path: PathBuf,
    pub max_depth: usize,
    pub top_n: usize,
    pub engine: EngineType,
}

pub fn parse_args() -> Config {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from<I>(args: I) -> Config
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().peekable();

    let mut path = None;
    let mut max_depth: Option<usize> = None;
    let mut top_n: Option<usize> = None;
    let mut engine: Option<EngineType> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--path" | "-p" => {
                if let Some(p) = args.next() {
                    path = Some(PathBuf::from(p));
                }
            }
            "--max_depth" | "-d" => {
                if let Some(d) = args.next() {
                    max_depth = d.parse::<usize>().ok();
                }
            }
            "--top" | "-t" => {
                if let Some(n) = args.next() {
                    top_n = n.parse::<usize>().ok();
                }
            }
            "--engine" | "-e" => {
                if let Some(e) = args.next() {
                    match e.as_str() {
                        "rayon" => engine = Some(EngineType::Rayon),
                        "crossbeam" => engine = Some(EngineType::Crossbeam),
                        _ => {
                            eprintln!("Invalid engine: {}", e);
                            exit(1);
                        }
                    }
                }
            }
            "--help" | "-h" => {
                print_help();
                exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", arg);
                exit(1);
            }
        }
    }

    let path = path.unwrap_or_else(|| {
        eprintln!("Error: --path/-p is required\n");
        print_help();
        exit(1);
    });

    Config {
        path,
        max_depth: max_depth.unwrap_or(usize::MAX),
        top_n: top_n.unwrap_or(10),
        engine: engine.unwrap_or(EngineType::Crossbeam),
    }
}

pub fn run(pb: ProgressBar) -> Result<(), io::Error> {
    let config = parse_args();

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

    print::print_tree(&config.path, results.as_ref(), config.top_n);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_flags() {
        let config = parse_args_from([
            "-p".to_string(),
            ".".to_string(),
            "-t".to_string(),
            "10".to_string(),
            "-d".to_string(),
            "2".to_string(),
            "-e".to_string(),
            "rayon".to_string(),
        ]);

        assert_eq!(config.path, PathBuf::from("."));
        assert_eq!(config.top_n, 10);
        assert_eq!(config.max_depth, 2);
        assert_eq!(config.engine, EngineType::Rayon);
    }

    #[test]
    fn parses_long_flags_unchanged() {
        let config = parse_args_from([
            "--path".to_string(),
            "./demo".to_string(),
            "--top".to_string(),
            "3".to_string(),
        ]);

        assert_eq!(config.path, PathBuf::from("./demo"));
        assert_eq!(config.top_n, 3);
        assert_eq!(config.max_depth, usize::MAX);
        assert_eq!(config.engine, EngineType::Crossbeam);
    }
}
