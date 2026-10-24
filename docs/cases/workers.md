# Worker totals

Each chunk updates a shared counter. Ordinary Rust clones an `Arc<Mutex<_>>` into the closure, locks on every field, and widens `u32` to `u64` by hand. [`shared`](../guide/shared.md), [capture lists](../guide/capture-lists.md) and [`#[widen]`](../guide/widen.md) leave the update as field access.

## Plain Rust

```rust
use std::sync::{Arc, Mutex};

struct Bucket {
    done: u32,
    bytes: u64,
}

fn run(chunks: &[u32]) -> (u32, u64) {
    let bucket = Arc::new(Mutex::new(Bucket { done: 0, bytes: 0 }));
    for chunk in chunks {
        let bucket = bucket.clone();
        let step = move || {
            let mut bucket = bucket.lock().unwrap();
            bucket.done += 1;
            bucket.bytes += u64::from(*chunk);
        };
        step();
    }
    let bucket = bucket.lock().unwrap();
    (bucket.done, bucket.bytes)
}

assert_eq!(run(&[1, 2, 4]), (3, 7));
```

## With rustx

```rust
rustx::sugar! {
    struct Bucket {
        done: u32,
        bytes: u64,
    }

    #[widen]
    fn run(chunks: &[u32]) -> (u32, u64) {
        shared bucket = Bucket { done: 0, bytes: 0 };
        for chunk in chunks {
            let step = move[bucket.clone()] || {
                let n: u32 = *chunk;
                let bytes: u64 = n;
                bucket.done += 1;
                bucket.bytes += bytes;
            };
            step();
        }
        (bucket.done, bucket.bytes)
    }
}

assert_eq!(run(&[1, 2, 4]), (3, 7));
```
