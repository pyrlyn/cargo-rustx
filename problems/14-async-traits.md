# `async` with `Pin` and `Box` in traits

## As is (Rust)

```rust
fn call(&self) -> Pin<Box<dyn Future<Output = T> + Send + '_>>
```

## Desired (sugar)

```rust
async fn call(&self) -> T
```

## Implementation

Native async fn in traits, plus `async_trait`-style boxing macro for `dyn` use.

Upstream: `async fn` in traits is stable (Rust 1.75), so the `Pin<Box<..>>` form above is only needed when the trait is used as `dyn`. Native `dyn` support for async traits is an accepted 2026–2027 project goal but is not available yet. Return type notation for `Send` bounds is still nightly-only. For the `dyn` case, generate code for the maintained `dynosaur` crate instead of hand-rolled boxing. See [research.md](../research.md).

## Detailed example

### As is (valid Rust)

```rust
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Store: Send + Sync {
    fn get<'a>(&'a self, key: &'a str) -> BoxFut<'a, Option<Vec<u8>>>;
    fn put<'a>(&'a self, key: &'a str, value: Vec<u8>) -> BoxFut<'a, Result<(), String>>;
}

pub struct MemStore {
    data: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl Store for MemStore {
    fn get<'a>(&'a self, key: &'a str) -> BoxFut<'a, Option<Vec<u8>>> {
        Box::pin(async move { self.data.lock().unwrap().get(key).cloned() })
    }

    fn put<'a>(&'a self, key: &'a str, value: Vec<u8>) -> BoxFut<'a, Result<(), String>> {
        Box::pin(async move {
            if value.len() > 1024 {
                return Err(format!("{}: value too large", key));
            }
            self.data.lock().unwrap().insert(key.to_string(), value);
            Ok(())
        })
    }
}

pub async fn copy_key(
    src: &(dyn Store + Send + Sync),
    dst: &(dyn Store + Send + Sync),
    key: &str,
) -> Result<bool, String> {
    match src.get(key).await {
        Some(v) => dst.put(key, v).await.map(|_| true),
        None => Ok(false),
    }
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod store {
    use std::collections::HashMap;

    pub trait Store: Send + Sync {
        async fn get(&self, key: &str) -> Option<Vec<u8>>;
        async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), String>;
    }

    pub struct MemStore {
        data: shared HashMap<String, Vec<u8>>,
    }

    impl Store for MemStore {
        async fn get(&self, key: &str) -> Option<Vec<u8>> {
            self.data.get(key).cloned()
        }

        async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), String> {
            if value.len() > 1024 {
                return Err(format!("{}: value too large", key));
            }
            self.data.insert(key.to_string(), value);
            Ok(())
        }
    }

    pub async fn copy_key(src: &~Store, dst: &~Store, key: &str) -> Result<bool, String> {
        match src.get(key).await {
            Some(v) => dst.put(key, v).await.map(|_| true),
            None => Ok(false),
        }
    }
}
}
```

### What changed

- The `BoxFut` alias, `Pin<Box<dyn Future>>`, `Box::pin(async move ..)` and `'a` lifetimes are gone: the trait uses plain `async fn`.
- For dyn-compatible traits the macro generates boxing (like `async_trait`), otherwise native async fn in traits.
- Also: `shared` in a field, `~Store` instead of `dyn Store + Send + Sync`.
