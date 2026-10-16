# `into T` parameters

`name: into String` accepts anything that converts into the type: the parameter becomes `impl Into<T>` and is converted at the top of the body. Problem: [15, `String` vs `&str` at API boundaries](../../problems/15-string-params.md).

```rust
rustx::sugar! {
    struct Contact {
        name: String,
    }

    impl Contact {
        fn new(name: into String) -> Self {
            Contact { name }
        }
    }
}

assert_eq!(Contact::new("Ann").name, "Ann");
assert_eq!(Contact::new(String::from("Bob")).name, "Bob");
```

Becomes:

```rust
struct Contact {
    name: String,
}
impl Contact {
    fn new(name: impl ::core::convert::Into<String>) -> Self {
        let name: String = ::core::convert::Into::into(name);
        Contact { name }
    }
}
```

`mut` and `ref` move to the generated `let`. Declarations without a body (trait items, `extern` fns) only get the `impl Into<T>` type.

## Limits

- `into` on `self`, on a pattern or without a type is an error.
- The other half of problem 15, string literals into `String` fields, is its own item: [string literals](string-literals.md).
