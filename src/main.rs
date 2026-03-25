use bfiles::cli::run;
use indicatif::{ProgressBar, ProgressStyle};

fn main() {
    let pb = ProgressBar::new_spinner();

    pb.set_style(ProgressStyle::with_template("{spinner:.green} {pos} items [{elapsed_precise}]").unwrap());
    pb.set_message("Searching...");

    if let Err(e) = run(pb) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
