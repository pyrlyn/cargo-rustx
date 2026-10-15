# Turbofish

## As is (Rust)

```rust
let n = s.parse::<u32>()?;
let v = it.collect::<Vec<_>>();
```

## Desired (sugar)

```rust
let n = s.parse<u32>()?;
let v = it.collect<Vec>();
```

## Implementation

Token rewrite of `.method<T>(` into `.method::<T>(`, filling missing `<_>` params.

## Detailed example

### As is (valid Rust)

```rust
use std::collections::{BTreeMap, HashSet};

pub struct Row {
    pub id: u32,
    pub tags: Vec<String>,
    pub score: f32,
}

pub fn parse_row(line: &str) -> Option<Row> {
    let parts = line.split(';').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    let id = parts[0].parse::<u32>().ok()?;
    let tags = parts[1]
        .split(',')
        .map(|t| t.to_lowercase())
        .collect::<Vec<String>>();
    let score = parts[2].parse::<f32>().ok()?;
    Some(Row { id, tags, score })
}

pub fn all_tags(rows: &[Row]) -> Vec<String> {
    let set = rows
        .iter()
        .flat_map(|r| r.tags.iter().cloned())
        .collect::<HashSet<_>>();
    let mut v = set.into_iter().collect::<Vec<_>>();
    v.sort();
    v
}

pub fn by_id(rows: Vec<Row>) -> BTreeMap<u32, Row> {
    rows.into_iter().map(|r| (r.id, r)).collect::<BTreeMap<_, _>>()
}

pub fn total(rows: &[Row]) -> f32 {
    rows.iter().map(|r| r.score).sum::<f32>()
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugared] on an item cannot be used
sugar! {
mod rows {
    use std::collections::{BTreeMap, HashSet};

    #[data]
    pub struct Row { id: u32, tags: Vec<String>, score: f32 }

    pub fn parse_row(line: &str) -> Option<Row> {
        let parts = line.split(';').map(str::trim).collect<Vec>();
        if parts.len() != 3 {
            return None;
        }
        let id = parts[0].parse<u32>().ok()?;
        let tags = parts[1].split(',').map(|t| t.to_lowercase()).collect<Vec<String>>();
        let score = parts[2].parse<f32>().ok()?;
        Some(Row::new(id, tags, score))
    }

    pub fn all_tags(rows: &[Row]) -> Vec<String> {
        let set = rows.iter().flat_map(|r| r.tags.iter().cloned()).collect<HashSet>();
        var v = set.into_iter().collect<Vec>();
        v.sort();
        v
    }

    pub fn by_id(rows: Vec<Row>) -> BTreeMap<u32, Row> {
        rows.into_iter().map(|r| (r.id, r)).collect<BTreeMap>()
    }

    pub fn total(rows: &[Row]) -> f32 {
        rows.iter().map(|r| r.score).sum<f32>()
    }
}
}
```

### What changed

- `::<T>` became `<T>` right after the method name.
- Missing parameters are filled with `_`: `collect<Vec>` → `collect::<Vec<_>>`, `collect<BTreeMap>` → `collect::<BTreeMap<_, _>>`.
- Also: `var` and `#[data]` with `Row::new`.
