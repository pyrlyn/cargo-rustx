# Method generics without turbofish

`.m<T>(..)` is `.m::<T>(..)`, and a bare std collection gets its missing arguments as `_`. Problem: [06, turbofish](../../problems/06-turbofish.md).

```rust
rustx::sugar! {
    fn parse_all(text: &str) -> Vec<u32> {
        text.split(',').map(|s| s.trim().parse<u32>().unwrap()).collect<Vec>()
    }
}

assert_eq!(parse_all("1, 2,3"), [1, 2, 3]);
```

Becomes:

```rust
fn parse_all(text: &str) -> Vec<u32> {
    text.split(',').map(|s| s.trim().parse::<u32>().unwrap()).collect::<Vec<_>>()
}
```

`Vec`, `VecDeque`, `LinkedList`, `BinaryHeap`, `HashSet` and `BTreeSet` get `<_>`; `HashMap` and `BTreeMap` get `<_, _>`. [`rustx-cli sugar`](cli.md) turns `.m::<T>(..)` back into `.m<T>(..)`.

## Limits

- Only method calls: path turbofish (`Vec::<u8>::new()`, `f::<u8>()`) stays as written.
- No type inference: other generic types are not filled in, since their arity is unknown.
- Inside a comma list, `(a.b < c, d > (e))` is valid Rust (two comparisons) but reads as `a.b::<c, d>(e)`. Parenthesise the comparison: `((a.b < c), d > (e))`.
