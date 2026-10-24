# Scanning rows

Finding a row, collecting the warning texts and counting keys. Ordinary Rust names the lifetime on the way in and on the way out, repeats `Clone + Eq + Hash`, and writes `collect::<Vec<_>>()`. [`&from`](../guide/from-lifetimes.md), [`bound`](../guide/bound-presets.md), [method generics](../guide/turbofish.md) and [`var`](../guide/var.md) keep the signatures about the data.

## Plain Rust

```rust
use std::collections::HashMap;
use std::hash::Hash;

fn first_match<'a, T>(rows: &'a [T], pred: impl Fn(&T) -> bool) -> Option<&'a T> {
    rows.iter().find(|row| pred(*row))
}

fn warn_texts<'a>(lines: &'a [(&'a str, &'a str)]) -> Vec<&'a str> {
    lines
        .iter()
        .filter(|pair| pair.0 == "warn")
        .map(|pair| pair.1)
        .collect::<Vec<_>>()
}

fn tally<K: Clone + Eq + Hash>(keys: &[K]) -> HashMap<K, usize> {
    let mut counts = HashMap::new();
    for key in keys {
        *counts.entry(key.clone()).or_insert(0) += 1;
    }
    counts
}

assert_eq!(first_match(&[1, 2, 3], |n| *n > 1), Some(&2));
assert_eq!(warn_texts(&[("info", "a"), ("warn", "b"), ("warn", "c")]), ["b", "c"]);
assert_eq!(tally(&["a", "b", "a"])["a"], 2);
```

## With rustx

```rust
rustx::sugar! {
    use std::collections::HashMap;
    use std::hash::Hash;

    bound Key = Clone + Eq + Hash;

    fn first_match<T>(rows: &[T], pred: impl Fn(&T) -> bool) -> Option<&from(rows) T> {
        rows.iter().find(|row| pred(*row))
    }

    fn warn_texts(lines: &[(&str, &str)]) -> Vec<&from(lines) str> {
        lines.iter().filter(|pair| pair.0 == "warn").map(|pair| pair.1).collect<Vec>()
    }

    fn tally<K: Key>(keys: &[K]) -> HashMap<K, usize> {
        var counts = HashMap::new();
        for key in keys {
            *counts.entry(key.clone()).or_insert(0) += 1;
        }
        counts
    }
}

assert_eq!(first_match(&[1, 2, 3], |n| *n > 1), Some(&2));
assert_eq!(warn_texts(&[("info", "a"), ("warn", "b"), ("warn", "c")]), ["b", "c"]);
assert_eq!(tally(&["a", "b", "a"])["a"], 2);
```
