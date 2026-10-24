# One borrowed view

A row that only borrows the line it was built from. Ordinary Rust repeats `'a` on the struct, the `impl` and the method. [`#[borrows]`](../guide/borrows.md) and [`&from(Self)`](../guide/from-lifetimes.md) name that relationship once.

## Plain Rust

```rust
struct Row<'a> {
    line: &'a str,
}

impl<'a> Row<'a> {
    fn new(line: &'a str) -> Self {
        Row { line }
    }

    fn cell(&self, index: usize) -> &'a str {
        self.line.split(',').nth(index).unwrap_or("")
    }
}

fn city(line: &str) -> String {
    Row::new(line).cell(1).to_string()
}

assert_eq!(city("ann,paris,fr"), "paris");
assert_eq!(Row::new("ann").cell(3), "");
```

## With rustx

```rust
rustx::sugar! {
    #[borrows]
    struct Row {
        line: &str,
    }

    impl Row {
        fn new(line: &from(Self) str) -> Self {
            Row { line }
        }

        fn cell(&self, index: usize) -> &from(Self) str {
            self.line.split(',').nth(index).unwrap_or("")
        }
    }

    fn city(line: &str) -> String {
        Row::new(line).cell(1).to_string()
    }
}

assert_eq!(city("ann,paris,fr"), "paris");
assert_eq!(Row::new("ann").cell(3), "");
```
