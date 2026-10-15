# Repeated bound sets (no stable trait aliases)

## As is (Rust)

```rust
fn dedup_count<K>(items: &[K]) -> HashMap<K, usize>
where K: Clone + Debug + Eq + Hash + Send + Sync + 'static,
fn first_dup<K>(items: &[K]) -> Option<K>
where K: Clone + Debug + Eq + Hash + Send + Sync + 'static,
```

## Desired (sugar)

```rust
bound Key = Clone + Debug + Eq + Hash + Send + Sync + 'static;

fn dedup_count(items: &[K: Key]) -> HashMap<K, usize>
fn first_dup(items: &[K: Key]) -> Option<K>
```

## Implementation

`bound Name = B1 + B2 + ..;` desugars into the usual stable workaround: `pub trait Name: B1 + B2 + .. {}` plus a blanket `impl<T: ?Sized + B1 + B2 + ..> Name for T {}`. `Self` in the bounds (`Add<Output = Self>`) stays `Self` in the trait and becomes `T` in the impl. Supertrait bounds, including associated type bindings, are implied by `T: Name`, so `a + b` type-checks in a function bounded by `Num`. Native `trait Name = ..;` aliases are still unstable upstream; the pass can switch to them later without changing user code.

## Detailed example

### As is (valid Rust)

```rust
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use std::ops::{Add, Mul};

pub fn dedup_count<K>(items: &[K]) -> HashMap<K, usize>
where
    K: Clone + Debug + Eq + Hash + Send + Sync + 'static,
{
    let mut out = HashMap::new();
    for k in items {
        *out.entry(k.clone()).or_insert(0) += 1;
    }
    out
}

pub fn first_dup<K>(items: &[K]) -> Option<K>
where
    K: Clone + Debug + Eq + Hash + Send + Sync + 'static,
{
    let mut seen = HashSet::new();
    items.iter().find(|k| !seen.insert(*k)).cloned()
}

pub fn dot<T>(a: &[T], b: &[T]) -> T
where
    T: Copy + Default + Add<Output = T> + Mul<Output = T>,
{
    a.iter().zip(b).fold(T::default(), |acc, (&x, &y)| acc + x * y)
}

pub fn norm_sq<T>(a: &[T]) -> T
where
    T: Copy + Default + Add<Output = T> + Mul<Output = T>,
{
    dot(a, a)
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugared] on an item cannot be used
sugar! {
mod generic {
    use std::collections::{HashMap, HashSet};
    use std::fmt::Debug;
    use std::hash::Hash;
    use std::ops::{Add, Mul};

    pub bound Key = Clone + Debug + Eq + Hash + Send + Sync + 'static;
    pub bound Num = Copy + Default + Add<Output = Self> + Mul<Output = Self>;

    pub fn dedup_count(items: &[K: Key]) -> HashMap<K, usize> {
        var out = HashMap::new();
        for k in items {
            *out.entry(k.clone()).or_insert(0) += 1;
        }
        out
    }

    pub fn first_dup(items: &[K: Key]) -> Option<K> {
        var seen = HashSet::new();
        items.iter().find(|k| !seen.insert(*k)).cloned()
    }

    pub fn dot(a: &[T: Num], b: &[T]) -> T {
        a.iter().zip(b).fold(T::default(), |acc, (&x, &y)| acc + x * y)
    }

    pub fn norm_sq(a: &[T: Num]) -> T {
        dot(a, a)
    }
}
}
```

### What changed

- Four copies of a long `where` clause became two named bounds. A change to the set happens in one place.
- `pub bound` is exported like a trait, so other crates can use `K: Key` too.
- Also: inline bounds (item 3), `var`.
