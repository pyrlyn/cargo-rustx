# Pluggable sinks

A function takes any sink. Ordinary Rust spells `Box<dyn Sink + Send + Sync>` at the binding and again at the parameter, and the binding is `let mut` only so the loop can write through it. [`~Trait`](../guide/boxed-traits.md) and [`var`](../guide/var.md) keep the type and the mutability obvious.

## Plain Rust

```rust
trait Sink {
    fn write(&mut self, line: &str);
    fn text(&self) -> &str;
}

struct Memory {
    buf: String,
}

impl Sink for Memory {
    fn write(&mut self, line: &str) {
        self.buf.push_str(line);
        self.buf.push('\n');
    }

    fn text(&self) -> &str {
        &self.buf
    }
}

fn run(sink: &mut Box<dyn Sink + Send + Sync>, lines: &[&str]) {
    for line in lines {
        sink.write(line);
    }
}

fn demo(lines: &[&str]) -> String {
    let mut sink: Box<dyn Sink + Send + Sync> = Box::new(Memory { buf: String::new() });
    run(&mut sink, lines);
    sink.text().to_string()
}

assert_eq!(demo(&["a", "b"]), "a\nb\n");
```

## With rustx

```rust
rustx::sugar! {
    trait Sink {
        fn write(&mut self, line: &str);
        fn text(&self) -> &str;
    }

    struct Memory {
        buf: String,
    }

    impl Sink for Memory {
        fn write(&mut self, line: &str) {
            self.buf.push_str(line);
            self.buf.push('\n');
        }

        fn text(&self) -> &str {
            &self.buf
        }
    }

    fn run(sink: &mut ~Sink, lines: &[&str]) {
        for line in lines {
            sink.write(line);
        }
    }

    fn demo(lines: &[&str]) -> String {
        var sink: ~Sink = Box::new(Memory { buf: String::new() });
        run(&mut sink, lines);
        sink.text().to_string()
    }
}

assert_eq!(demo(&["a", "b"]), "a\nb\n");
```
