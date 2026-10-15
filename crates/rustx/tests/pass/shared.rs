// Problem 12's detailed example, threads included.
rustx::sugar! {
    mod metrics {
        use std::collections::HashMap;
        use std::thread;

        #[derive(Default)]
        #[data]
        pub struct Metrics { requests: u64, errors: u64, by_path: HashMap<String, u64> }

        pub fn simulate(paths: Vec<String>, threads: usize) -> (u64, u64, usize) {
            shared metrics = Metrics::default();
            shared paths = paths;   // read-only: becomes Arc<Vec<String>>
            var handles = Vec::new();
            for t in 0..threads {
                handles.push(thread::spawn(move[metrics.clone(), paths.clone()] || {
                    for (i, p) in paths.iter().enumerate() {
                        metrics.requests += 1;
                        if (i + t) % 7 == 0 { metrics.errors += 1; }
                        *metrics.by_path.entry(p.clone()).or_insert(0) += 1;
                    }
                }));
            }
            for h in handles { h.join().unwrap(); }
            (metrics.requests, metrics.errors, metrics.by_path.len())
        }
    }

    pub struct Board {
        scores: shared Vec<u32>,
    }

    pub fn play(rounds: u32) -> (u32, usize, bool) {
        shared log: Vec<u32> = Vec::new();
        let board = Board { scores: rustx::Shared::default() };
        for r in 0..rounds {
            log.push(r);
            board.scores.with_mut(|s| s.push(r * 2));
        }
        // A condition locks on its own, so the body can lock again.
        while log.len() > 2 {
            log.pop();
        }
        let total = board.scores.with(|s| s.iter().sum());
        // Two accesses in one statement share one guard instead of deadlocking.
        let first_is_zero = log.first() == Some(&0) && log.len() == 2;
        (total, log.len(), first_is_zero)
    }

    // `shared` stays usable as an ordinary identifier.
    fn shared(shared: u32) -> u32 {
        shared + 1
    }
}

fn main() {
    let paths = vec!["/a".to_owned(), "/b".to_owned(), "/a".to_owned()];
    assert_eq!(metrics::simulate(paths, 3), (9, 1, 2));
    assert_eq!(play(4), (12, 2, true));
    assert_eq!(shared(1), 2);
}
