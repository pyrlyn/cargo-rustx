# Клонирование в замыканиях

## Как есть (Rust)

```rust
let a = a.clone();
let b = b.clone();
spawn(move || work(a, b));
```

## Как хочется

```rust
spawn(move[a.clone(), b.clone()] || work(a, b));
```

## Реализация

Макрос списка захвата разворачивается в `let`-привязки перед `move`-замыканием.

## Подробный пример

### Как есть (валидный Rust)

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

### Как хочется (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
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

### Что изменилось

- Блоки `let x = x.clone();` перед `move` заменены списком захвата `move[a.clone(), ..]`.
- Макрос разворачивает список в `{ let a = a.clone(); move || .. }`, имена не меняются (не нужен `report_prefix`).
- Попутно: `var`, `collect<Vec<String>>()`, `#[data]`.
