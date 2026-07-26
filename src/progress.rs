use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
};

use indicatif::ProgressBar;

pub fn start_progress(pb: &ProgressBar, counter: Arc<AtomicU64>, done: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
        let pb_clone = pb.clone();

        std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                        let value = counter.load(Ordering::Relaxed);
                        pb_clone.set_position(value);

                        std::thread::sleep(std::time::Duration::from_millis(100));
                }
        })
}
