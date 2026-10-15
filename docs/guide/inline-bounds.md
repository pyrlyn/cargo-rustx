# Inline bounds

`x: T: Bound` in a parameter declares the generic `T` and moves the bound into `where`. Problem: [03, long signatures](../../problems/03-long-signatures.md).

```rust
rustx::sugar! {
    use std::fmt::Debug;

    fn repeat(item: T: Clone + Debug, times: usize) -> Vec<T> {
        vec![item; times]
    }

    fn pick(value: T: Clone, ok: bool, err: E) -> Result<T, E: Debug> {
        if ok { Ok(value.clone()) } else { Err(err) }
    }
}

assert_eq!(repeat('a', 2), ['a', 'a']);
assert_eq!(pick(1, false, "no"), Err("no"));
```

Becomes:

```rust
use std::fmt::Debug;
fn repeat<T>(item: T, times: usize) -> Vec<T>
where
    T: Clone + Debug,
{
    vec![item; times]
}
fn pick<T, E>(value: T, ok: bool, err: E) -> Result<T, E>
where
    T: Clone,
    E: Debug,
{
    if ok { Ok(value.clone()) } else { Err(err) }
}
```

`E: Bound` also works inside the return type, as `Result<T, E: Debug>` shows. A generic used twice is declared once; existing generics and `where` clauses are kept. Structs, enums and unions declared in the same input lend their bounds to `impl` headers and signatures that use them with undeclared parameters.

## Limits

- The bounded type must be a bare generic name: `items: &[T]: Debug` is an error, and so is `items: &[K: Key]` as problem 21 sketches it. Write such generics in `<..>` as in plain Rust.
- "New generic or existing type" is decided from the input only: a name already in scope gets the bound, a type declared or imported in the same input is an error, anything else becomes a new generic, so a glob-imported type is shadowed by a fresh generic.
- Signatures inside macro bodies and closure parameters are not lowered.
