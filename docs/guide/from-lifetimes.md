# `&from(x) T` return lifetimes

`-> &from(a) T` says which parameters a returned reference borrows from; the lifetime is generated. Problem: [01, lifetimes in return types](../../problems/01-return-lifetimes.md).

```rust
rustx::sugar! {
    fn longest(a: &str, b: &str) -> &from(a, b) str {
        if a.len() >= b.len() { a } else { b }
    }

    fn first_word(text: &str, sep: &str) -> &from(text) str {
        text.split(sep).next().unwrap_or("")
    }
}

assert_eq!(longest("ab", "abc"), "abc");
assert_eq!(first_word("hello world", " "), "hello");
```

Becomes:

```rust
fn longest<'__rustx_from_a>(
    a: &'__rustx_from_a str,
    b: &'__rustx_from_a str,
) -> &'__rustx_from_a str {
    if a.len() >= b.len() { a } else { b }
}
fn first_word<'__rustx_from_text>(
    text: &'__rustx_from_text str,
    sep: &str,
) -> &'__rustx_from_text str {
    text.split(sep).next().unwrap_or("")
}
```

`from(a, b)` ties several parameters to one lifetime. The named parameter may be a reference, a `self` receiver, an `impl Trait` or a type parameter with a reference bound. Combined with [`iter T`](iter.md), `-> iter &from(xs) T` uses the same lifetime instead of `'_`. `from(Self)` and `from(Type)` belong to [`#[borrows]`](borrows.md).

## Limits

- Unknown names, parameters that are not references and `from` outside a return type are errors.
- Destructured parameters cannot be named.
- Only the outer reference of a parameter is tied: in `&Vec<&str>` the inner `&str` keeps its own lifetime.
