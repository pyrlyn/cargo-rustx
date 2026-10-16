# `var` bindings

`var x = e;` is a mutable binding: it is `let mut x = e;`. Problem: [02, ubiquitous `mut`](../../problems/02-ubiquitous-mut.md).

```rust
rustx::sugar! {
    fn total(xs: &[u32]) -> u32 {
        var sum = 0;
        for x in xs {
            sum += x;
        }
        sum
    }
}

assert_eq!(total(&[1, 2, 3]), 6);
```

Becomes:

```rust
fn total(xs: &[u32]) -> u32 {
    let mut sum = 0;
    for x in xs {
        sum += x;
    }
    sum
}
```

`var x: T = e;` keeps its type. [`rustx-cli sugar`](cli.md) turns `let mut x` back into `var x`.

## Limits

- `var` means exactly `let mut <name>`: destructuring, `var mut`, `var ref` and `var _` are errors.
- `var` is sugar only at the start of a statement; elsewhere it is an ordinary identifier.
- It is rewritten inside macro-call arguments too, so a user macro that reads `var x` itself receives `let mut x`.
