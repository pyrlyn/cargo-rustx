# `shared` bindings

`shared s = e;` puts the value behind `rustx::Shared<T>` (an `Arc<Mutex<T>>`), and field access on `s` goes through the lock. `shared T` in a type is `Shared<T>`. Problem: [12, `Arc<Mutex<T>>` and lock noise](../../problems/12-arc-mutex.md).

```rust
rustx::sugar! {
    struct Stats {
        hits: u32,
        misses: u32,
    }

    fn run() -> u32 {
        shared stats = Stats { hits: 0, misses: 0 };
        let worker = move[stats.clone()] || {
            stats.hits += 1;
        };
        worker();
        stats.misses += 1;
        stats.hits + stats.misses
    }
}

assert_eq!(run(), 2);
```

Becomes:

```rust
struct Stats {
    hits: u32,
    misses: u32,
}
fn run() -> u32 {
    let stats = ::rustx::Shared::new(Stats { hits: 0, misses: 0 });
    let worker = {
        let stats = stats.clone();
        move || {
            stats.lock().hits += 1;
        }
    };
    worker();
    stats.lock().misses += 1;
    {
        let __rustx_stats = &mut *stats.lock();
        __rustx_stats.hits + __rustx_stats.misses
    }
}
```

`Shared::lock` recovers a poisoned lock instead of the problem file's `.lock().unwrap()`; `is_poisoned` reports it. A binding whose scope only reads becomes a plain `Arc` instead. For several operations under one lock, call `s.with(|v| ..)` or `s.with_mut(|v| ..)`; the handle methods (`lock`, `with`, `with_mut`, `is_poisoned`, `clone`) are left as written.

## Limits

- One lock per statement: a statement (or block tail, or `if` / `while` condition) with one access locks for it; one with more takes one guard for the whole statement. Nothing is atomic across statements: use `with` / `with_mut`.
- The handle is picked by syntax. A read-only binding is a plain `Arc`, so a user `&mut self` method on it fails with "cannot borrow data in an `Arc` as mutable"; write `s.lock().m()`.
- Only `shared` bindings are tracked: parameters and fields typed `shared T`, and clones bound under another name, call `lock()` or `with` by hand.
- Macro bodies that are not expression lists are not rewritten, and a `shared` binding cannot be declared inside a macro call. `shared mut`, a pattern instead of a name and a missing value are errors.
