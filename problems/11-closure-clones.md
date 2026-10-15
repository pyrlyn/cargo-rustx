# Cloning for closures

## As is (Rust)

```rust
let a = a.clone();
let b = b.clone();
spawn(move || work(a, b));
```

## Desired (sugar)

```rust
spawn(move[a.clone(), b.clone()] || work(a, b));
```

## Implementation

Capture-list macro expanding into `let` bindings before a `move` closure.

Upstream: this is the most active ergonomics area. RFC 3680 (`.use`) is still open and contested. The accepted 2026 project goal "ergonomic ref-counting" moved to a `Share` trait plus `move(expr)` inside closures (RFC 3968, nightly `move_expr`). Neither is stable. The capture-list form is kept close to `move(expr)`, so the desugaring can switch to the native syntax once it lands without changing user code. See [research.md](../research.md).

## Detailed example

### As is (valid Rust)

```rust
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

pub struct Job {
    pub id: usize,
    pub input: String,
}

pub fn run_pool(jobs: Vec<Job>, prefix: String, workers: usize) -> Vec<String> {
    let jobs = Arc::new(jobs);
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for w in 0..workers {
        let jobs = jobs.clone();
        let tx = tx.clone();
        let prefix = prefix.clone();
        handles.push(thread::spawn(move || {
            for job in jobs.iter().filter(|j| j.id % workers == w) {
                let out = format!("{}[{}] {}", prefix, w, job.input.to_uppercase());
                tx.send(out).unwrap();
            }
        }));
    }
    drop(tx);

    let report_prefix = prefix.clone();
    let reporter = thread::spawn(move || {
        let mut all: Vec<String> = rx.iter().collect();
        all.sort();
        all.insert(0, format!("{} report", report_prefix));
        all
    });

    for h in handles {
        h.join().unwrap();
    }
    reporter.join().unwrap()
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugared] on an item cannot be used
sugar! {
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
}
```

### What changed

- `let x = x.clone();` blocks before `move` became a capture list `move[a.clone(), ..]`.
- The macro expands it into `{ let a = a.clone(); move || .. }`, keeping the names (no `report_prefix` needed).
- Also: `var`, `collect<Vec<String>>()`, `#[data]`.
