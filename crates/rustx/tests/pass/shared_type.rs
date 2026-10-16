// The problem's detailed example written by hand against `rustx::Shared`, the code the
// `shared` sugar will expand to.
use rustx::Shared;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

#[derive(Debug, Default)]
pub struct Metrics {
    pub requests: u64,
    pub errors: u64,
    pub by_path: HashMap<String, u64>,
}

pub struct Server {
    metrics: Shared<Metrics>,
}

pub fn simulate(paths: Vec<String>, threads: usize) -> (u64, u64, usize) {
    let metrics = Shared::new(Metrics::default());
    let paths = Arc::new(paths);
    let mut handles = Vec::new();
    for t in 0..threads {
        let metrics = metrics.clone();
        let paths = Arc::clone(&paths);
        handles.push(thread::spawn(move || {
            for (i, p) in paths.iter().enumerate() {
                metrics.lock().requests += 1;
                if (i + t) % 7 == 0 {
                    metrics.lock().errors += 1;
                }
                *metrics.lock().by_path.entry(p.clone()).or_insert(0) += 1;
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    metrics.with(|m| (m.requests, m.errors, m.by_path.len()))
}

fn main() {
    let paths = vec!["/a".to_owned(), "/b".to_owned(), "/a".to_owned()];
    assert_eq!(simulate(paths, 3), (9, 1, 2));

    let server = Server {
        metrics: Shared::default(),
    };
    let handle = server.metrics.clone();
    let total = handle.with_mut(|m| {
        m.requests += 2;
        m.errors += 1;
        m.requests + m.errors
    });
    assert_eq!(total, 3);
    assert_eq!(server.metrics.lock().requests, 2);
    assert!(format!("{:?}", server.metrics).starts_with("Shared(Metrics"));

    let from: Shared<u8> = 4.into();
    assert!(!from.is_poisoned());
}
