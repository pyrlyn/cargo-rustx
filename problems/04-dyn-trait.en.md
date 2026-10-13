# `dyn Trait` in signatures

## As is (Rust)

```rust
fn handlers() -> Vec<Box<dyn Fn(&Req) -> Resp + Send + Sync>>
```

## Desired (sugar)

```rust
fn handlers() -> Vec<~Fn(&Req) -> Resp>
```

## Implementation

Desugar `~Trait` into `Box<dyn Trait + Send + Sync>` (configurable bounds).

## Detailed example

### As is (valid Rust)

```rust
use std::collections::HashMap;

pub struct Event {
    pub topic: String,
    pub payload: String,
}

pub trait Sink {
    fn deliver(&self, e: &Event);
}

pub struct Bus {
    handlers: HashMap<String, Vec<Box<dyn Fn(&Event) -> bool + Send + Sync>>>,
    sinks: Vec<Box<dyn Sink + Send + Sync>>,
}

impl Bus {
    pub fn new() -> Self {
        Bus { handlers: HashMap::new(), sinks: Vec::new() }
    }

    pub fn on(&mut self, topic: &str, h: Box<dyn Fn(&Event) -> bool + Send + Sync>) {
        self.handlers.entry(topic.to_string()).or_default().push(h);
    }

    pub fn add_sink(&mut self, s: Box<dyn Sink + Send + Sync>) {
        self.sinks.push(s);
    }

    pub fn publish(&self, e: Event) -> usize {
        let mut handled = 0;
        if let Some(hs) = self.handlers.get(&e.topic) {
            for h in hs {
                if h(&e) {
                    handled += 1;
                }
            }
        }
        for s in &self.sinks {
            s.deliver(&e);
        }
        handled
    }
}
```

### Desired (BetterDiffuseRust)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod bus {
    use std::collections::HashMap;

    #[data]
    pub struct Event { topic: String, payload: String }

    pub trait Sink {
        fn deliver(&self, e: &Event);
    }

    pub struct Bus {
        handlers: HashMap<String, Vec<~Fn(&Event) -> bool>>,
        sinks: Vec<~Sink>,
    }

    impl Bus {
        pub fn new() -> Self {
            Bus { handlers: HashMap::new(), sinks: Vec::new() }
        }

        pub fn on(&mut self, topic: &str, h: ~Fn(&Event) -> bool) {
            self.handlers.entry(topic.to_string()).or_default().push(h);
        }

        pub fn add_sink(&mut self, s: ~Sink) {
            self.sinks.push(s);
        }

        pub fn publish(&self, e: Event) -> usize {
            var handled = 0;
            if let Some(hs) = self.handlers.get(&e.topic) {
                for h in hs {
                    if h(&e) { handled += 1; }
                }
            }
            for s in &self.sinks { s.deliver(&e); }
            handled
        }
    }
}
}
```

### What changed

- `Box<dyn X + Send + Sync>` in all five places became `~X`.
- The desugaring is purely syntactic: `~T` → `Box<dyn T + Send + Sync>`.
- Also: `var` and `#[data]` for `Event`.
