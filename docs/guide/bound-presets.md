# `bound` presets

`bound Name = A + B;` names a set of bounds: it becomes a trait with those supertraits and a blanket impl. Problem: [21, repeated bound sets](../../problems/21-bound-aliases.md).

```rust
rustx::sugar! {
    use std::collections::HashMap;
    use std::hash::Hash;

    bound Key = Clone + Eq + Hash;

    fn count<K: Key>(keys: &[K]) -> HashMap<K, usize> {
        var counts = HashMap::new();
        for k in keys {
            *counts.entry(k.clone()).or_insert(0) += 1;
        }
        counts
    }
}

assert_eq!(count(&["a", "b", "a"])["a"], 2);
```

Becomes:

```rust
use std::collections::HashMap;
use std::hash::Hash;
trait Key: Clone + Eq + Hash {}
impl<__RustxSelf: ?::core::marker::Sized + Clone + Eq + Hash> Key for __RustxSelf {}
fn count<K: Key>(keys: &[K]) -> HashMap<K, usize> {
    let mut counts = HashMap::new();
    for k in keys {
        *counts.entry(k.clone()).or_insert(0) += 1;
    }
    counts
}
```

`pub bound` gives the trait that visibility, so a preset can be exported. `Self` in the bounds (`Add<Output = Self>`) maps to the impl's type parameter, and `#[cfg]` on the preset is copied to the impl.

## Limits

- A preset is used where a bound is written (`<K: Key>`, `where K: Key`, or an [inline bound](inline-bounds.md) `k: K: Key`); `&[K: Key]` as problem 21 sketches it is not supported.
- Generic presets, a missing `=` or `;` and an empty list are errors.
