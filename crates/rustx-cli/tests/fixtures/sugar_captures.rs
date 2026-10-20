//! Workers that take their own handles, written without sugar.

use std::sync::{Arc, Mutex};
use std::thread;

pub fn spawn_workers(log: Arc<Mutex<Vec<String>>>, name: Arc<String>) -> Vec<thread::JoinHandle<()>> {
    (0..2)
        .map(|id| {
            // Each worker gets its own handles.
            thread::spawn({
                let log = log.clone();
                let name = name.clone();
                move || {
                    log.lock().unwrap().push(format!("{name} {id}"));
                }
            })
        })
        .collect()
}

pub fn counter(name: Arc<String>) -> impl Fn() -> usize {
    // The binding is renamed, so the pass would not build this block: it stays.
    let counted = {
        let other = name.clone();
        move || other.len()
    };
    counted
}
