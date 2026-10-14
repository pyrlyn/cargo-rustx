# Вездесущий `mut`

## Как есть (Rust)

```rust
let mut v = Vec::new();
let mut i = 0;
```

## Как хочется

```rust
var v = Vec::new();
var i = 0;
```

## Реализация

Макрос на функцию или блок переписывает `var x = ..` в `let mut x = ..`.

## Подробный пример

### Как есть (валидный Rust)

```rust
pub struct Stats {
    pub count: usize,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
}

pub fn summarize(samples: &[f64]) -> Option<Stats> {
    if samples.is_empty() {
        return None;
    }
    let mut count = 0;
    let mut sum = 0.0;
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for &s in samples {
        count += 1;
        sum += s;
        if s < min {
            min = s;
        }
        if s > max {
            max = s;
        }
    }
    Some(Stats { count, sum, min, max })
}

pub fn histogram(samples: &[f64], buckets: usize) -> Vec<usize> {
    let mut out = vec![0; buckets];
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = sorted[0];
    let hi = sorted[sorted.len() - 1];
    let width = (hi - lo) / buckets as f64;
    for s in sorted {
        let mut idx = ((s - lo) / width) as usize;
        if idx >= buckets {
            idx = buckets - 1;
        }
        out[idx] += 1;
    }
    out
}
```

### Как хочется (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod stats {
    #[data]
    pub struct Stats { count: usize, sum: f64, min: f64, max: f64 }

    pub fn summarize(samples: &[f64]) -> Option<Stats> {
        if samples.is_empty() {
            return None;
        }
        var count = 0;
        var sum = 0.0;
        var min = f64::MAX;
        var max = f64::MIN;
        for &s in samples {
            count += 1;
            sum += s;
            if s < min { min = s; }
            if s > max { max = s; }
        }
        Some(Stats::new(count, sum, min, max))
    }

    pub fn histogram(samples: &[f64], buckets: usize) -> Vec<usize> {
        var out = vec![0; buckets];
        var sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let lo = sorted[0];
        let hi = sorted[sorted.len() - 1];
        let width = (hi - lo) / buckets;      // implicit safe into()
        for s in sorted {
            var idx = ((s - lo) / width) as usize;
            if idx >= buckets { idx = buckets - 1; }
            out[idx] += 1;
        }
        out
    }
}
}
```

### Что изменилось

- Все `let mut` заменены на `var`; `let` теперь всегда означает неизменяемое.
- `#[data]` генерирует `Stats::new` и геттеры.
- `buckets as f64` стал неявным `into()` (расширяющее приведение).
