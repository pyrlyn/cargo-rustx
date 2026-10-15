use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

rustx::sugar! {
    mod pool {
        use std::sync::mpsc;
        use std::sync::Arc;
        use std::thread;

        #[data]
        pub struct Job { id: usize, input: String }

        pub fn run_pool(jobs: Vec<Job>, prefix: String, workers: usize) -> Vec<String> {
            let jobs = Arc::new(jobs);
            let (tx, rx) = mpsc::channel();
            var handles = Vec::new();
            for w in 0..workers {
                handles.push(thread::spawn(move[jobs.clone(), tx.clone(), prefix.clone()] || {
                    for job in jobs.iter().filter(|j| j.id % workers == w) {
                        let out = format!("{}[{}] {}", prefix, w, job.input.to_uppercase());
                        tx.send(out).unwrap();
                    }
                }));
            }
            drop(tx);

            let reporter = thread::spawn(move[prefix.clone()] || {
                var all = rx.iter().collect<Vec<String>>();
                all.sort();
                all.insert(0, format!("{} report", prefix));
                all
            });

            for h in handles { h.join().unwrap(); }
            reporter.join().unwrap()
        }
    }

    // Closure parameters, an async closure, and a capture list inside a closure body.
    pub fn adders(base: Vec<u32>) -> (impl Fn(u32) -> usize, impl AsyncFn(u32) -> u32) {
        let count = move[base.clone()] |x: u32| base.iter().filter(|&&b| b < x).count();
        let sum = async move[base.iter().sum<u32>()] |x| base + x;
        (count, sum)
    }

    pub fn nested(label: String) -> impl Fn() -> Box<dyn Fn() -> String> {
        move[label.clone()] || Box::new(move[label.clone()] || label.clone())
    }
}

/// Polls a future that never waits; enough for the async closure under test.
fn ready<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!("the future does not wait"),
    }
}

fn main() {
    let jobs = vec![pool::Job::new(0, "a".into()), pool::Job::new(1, "b".into())];
    let prefix = String::from("w");
    let report = pool::run_pool(jobs, prefix, 2);
    assert_eq!(report, ["w report", "w[0] A", "w[1] B"]);

    let (count, sum) = adders(vec![1, 2, 3]);
    assert_eq!(count(3), 2);
    assert_eq!(ready(sum(4)), 10);

    assert_eq!(nested("x".into())()(), "x");
}
