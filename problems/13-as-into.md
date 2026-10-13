# Явные `as` и `into()`

## Как есть (Rust)

```rust
let x: u64 = (a as u64) + b.into();
```

## Как хочется

```rust
let x: u64 = a + b;
```

## Реализация

Макрос на выражение вставляет `into()` только для безопасных расширяющих приведений.

## Подробный пример

### Как есть (валидный Rust)

```rust
pub struct Sample {
    pub timestamp_ms: u64,
    pub bytes: u32,
    pub latency_us: u16,
}

pub struct Summary {
    pub total_bytes: u64,
    pub avg_latency_us: f64,
    pub throughput_bps: f64,
    pub window_s: f32,
}

pub fn summarize(samples: &[Sample]) -> Option<Summary> {
    let first = samples.first()?;
    let last = samples.last()?;
    let mut total_bytes: u64 = 0;
    let mut latency_sum: u64 = 0;
    for s in samples {
        total_bytes += s.bytes as u64;
        latency_sum += u64::from(s.latency_us);
    }
    let n = samples.len() as f64;
    let window_ms = (last.timestamp_ms - first.timestamp_ms).max(1);
    let window_s = window_ms as f64 / 1000.0;
    Some(Summary {
        total_bytes,
        avg_latency_us: latency_sum as f64 / n,
        throughput_bps: total_bytes as f64 * 8.0 / window_s,
        window_s: window_s as f32,
    })
}

pub fn bucket(latency_us: u16) -> usize {
    let v: u32 = latency_us.into();
    (v / 100) as usize
}
```

### Как хочется (BetterDiffuseRust)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod summary {
    #[data]
    pub struct Sample { timestamp_ms: u64, bytes: u32, latency_us: u16 }

    #[data]
    pub struct Summary { total_bytes: u64, avg_latency_us: f64, throughput_bps: f64, window_s: f32 }

    pub fn summarize(samples: &[Sample]) -> Option<Summary> {
        let first = samples.first()?;
        let last = samples.last()?;
        var total_bytes: u64 = 0;
        var latency_sum: u64 = 0;
        for s in samples {
            total_bytes += s.bytes;          // u32 -> u64
            latency_sum += s.latency_us;     // u16 -> u64
        }
        let n = samples.len() as f64;        // lossy: stays explicit
        let window_ms = (last.timestamp_ms - first.timestamp_ms).max(1);
        let window_s = window_ms as f64 / 1000.0;
        Some(Summary::new(
            total_bytes,
            latency_sum as f64 / n,
            total_bytes as f64 * 8.0 / window_s,
            window_s as f32,                 // narrowing: stays explicit
        ))
    }

    pub fn bucket(latency_us: u16) -> usize {
        let v: u32 = latency_us;             // u16 -> u32
        (v / 100) as usize
    }
}
}
```

### Что изменилось

- Расширяющие приведения без потерь (`u32 → u64`, `u16 → u64`, `u16 → u32`) стали неявными.
- С потерями (`u64 → f64`, `f64 → f32`, `usize`) остаются явным `as` — это правильно.
- Ограничение: макрос не знает типов, поэтому вставляет `.into()` только там, где тип цели виден синтаксически (аннотация `let`, `+=` в поле известного типа) — см. ARCHITECTURE.md.
