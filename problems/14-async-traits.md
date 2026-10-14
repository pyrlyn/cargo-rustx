# `async` с `Pin` и `Box` в трейтах

## Как есть (Rust)

```rust
fn call(&self) -> Pin<Box<dyn Future<Output = T> + Send + '_>>
```

## Как хочется

```rust
async fn call(&self) -> T
```

## Реализация

Нативный async fn в трейтах плюс макрос боксинга в стиле `async_trait` для `dyn`.

## Подробный пример

### Как есть (валидный Rust)

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

### Как хочется (cargo-rustx)

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

### Что изменилось

- Псевдоним `BoxFut`, `Pin<Box<dyn Future>>`, `Box::pin(async move ..)` и лайфтаймы `'a` исчезли: в трейте пишется обычный `async fn`.
- Для dyn-совместимых трейтов макрос генерирует боксинг (как `async_trait`), иначе — нативный async fn in traits.
- Попутно: `shared` в поле, `~Store` вместо `dyn Store + Send + Sync`.
