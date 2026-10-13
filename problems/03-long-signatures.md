# Длинные сигнатуры с generics и `where`

## Как есть (Rust)

```rust
fn run<T, E>(x: T) -> Result<T, E>
where
    T: Clone + Debug + Send,
    E: Error,
```

## Как хочется

```rust
fn run(x: T: Clone + Debug + Send) -> Result<T, E: Error>
```

## Реализация

Атрибутный макрос выносит inline-ограничения в список generics и `where`.

## Подробный пример

### Как есть (валидный Rust)

```rust
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

pub struct Cache<K, V> {
    map: HashMap<K, V>,
    hits: u64,
}

impl<K, V> Cache<K, V>
where
    K: Eq + Hash + Clone + Debug,
    V: Clone,
{
    pub fn new() -> Self {
        Cache { map: HashMap::new(), hits: 0 }
    }

    pub fn get_or_insert_with<F, E>(&mut self, key: K, make: F) -> Result<V, E>
    where
        F: FnOnce(&K) -> Result<V, E>,
        E: Debug,
    {
        if let Some(v) = self.map.get(&key) {
            self.hits += 1;
            return Ok(v.clone());
        }
        let v = make(&key)?;
        self.map.insert(key, v.clone());
        Ok(v)
    }
}

pub fn warm_up<K, V, I>(cache: &mut Cache<K, V>, items: I)
where
    K: Eq + Hash + Clone + Debug,
    V: Clone,
    I: IntoIterator<Item = (K, V)>,
{
    for (k, v) in items {
        cache.map.insert(k, v);
    }
}
```

### Как хочется (BetterDiffuseRust)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod cache {
    use std::collections::HashMap;
    use std::fmt::Debug;
    use std::hash::Hash;

    pub struct Cache<K: Eq + Hash + Clone + Debug, V: Clone> {
        map: HashMap<K, V>,
        hits: u64,
    }

    impl Cache<K, V> {   // bounds inherited from the struct
        pub fn new() -> Self {
            Cache { map: HashMap::new(), hits: 0 }
        }

        pub fn get_or_insert_with(
            &mut self,
            key: K,
            make: F: FnOnce(&K) -> Result<V, E>,
        ) -> Result<V, E: Debug> {
            if let Some(v) = self.map.get(&key) {
                self.hits += 1;
                return Ok(v.clone());
            }
            let v = make(&key)?;
            self.map.insert(key, v.clone());
            Ok(v)
        }
    }

    pub fn warm_up(cache: &mut Cache<K, V>, items: I: IntoIterator<Item = (K, V)>) {
        for (k, v) in items {
            cache.map.insert(k, v);
        }
    }
}
}
```

### Что изменилось

- Ограничения пишутся на месте использования: `make: F: FnOnce(..)`, `E: Debug`.
- Макрос собирает их в `<..>` и `where`; `impl` наследует ограничения структуры.
- Блоки `where` из 3-5 строк исчезли, сигнатура читается слева направо.
