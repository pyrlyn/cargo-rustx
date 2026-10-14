# `dyn Trait` в сигнатурах

## Как есть (Rust)

```rust
fn handlers() -> Vec<Box<dyn Fn(&Req) -> Resp + Send + Sync>>
```

## Как хочется

```rust
fn handlers() -> Vec<~Fn(&Req) -> Resp>
```

## Реализация

Десугаринг `~Trait` в `Box<dyn Trait + Send + Sync>` (набор ограничений настраивается).

## Подробный пример

### Как есть (валидный Rust)

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

### Как хочется (cargo-rustx)

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

### Что изменилось

- `Box<dyn X + Send + Sync>` во всех пяти местах заменён на `~X`.
- Десугаринг чисто синтаксический: `~T` → `Box<dyn T + Send + Sync>`.
- Попутно: `var` и `#[data]` для `Event`.
