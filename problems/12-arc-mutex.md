# `Arc<Mutex<T>>` и lock-шум

## Как есть (Rust)

```rust
let s = Arc::new(Mutex::new(State::default()));
s.lock().unwrap().count += 1;
```

## Как хочется

```rust
shared s = State::default();
s.count += 1;
```

## Реализация

Тип-обёртка и макрос, вставляющий `.lock().unwrap()` (или замыкание `with`) при доступе.

## Подробный пример

### Как есть (валидный Rust)

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Default)]
pub struct Metrics {
    pub requests: u64,
    pub errors: u64,
    pub by_path: HashMap<String, u64>,
}

pub fn simulate(paths: Vec<String>, threads: usize) -> (u64, u64, usize) {
    let metrics = Arc::new(Mutex::new(Metrics::default()));
    let paths = Arc::new(paths);
    let mut handles = Vec::new();
    for t in 0..threads {
        let metrics = Arc::clone(&metrics);
        let paths = Arc::clone(&paths);
        handles.push(thread::spawn(move || {
            for (i, p) in paths.iter().enumerate() {
                let mut m = metrics.lock().unwrap();
                m.requests += 1;
                if (i + t) % 7 == 0 {
                    m.errors += 1;
                }
                *m.by_path.entry(p.clone()).or_insert(0) += 1;
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let m = metrics.lock().unwrap();
    (m.requests, m.errors, m.by_path.len())
}
```

### Как хочется (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
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
}
```

### Что изменилось

- `Arc::new(Mutex::new(..))` заменён на `shared x = ..`.
- Доступ к полям сам вставляет `.lock().unwrap()`; для неизменяемых данных — просто `Arc`.
- Внимание: каждое обращение — отдельная блокировка; для атомарной группы операций есть `metrics.with(|m| ..)`.
- Попутно: `move[..]`, `var`.
