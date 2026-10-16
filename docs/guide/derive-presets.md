# `derive` presets

`derive Name = A, B, C;` names a list of derives; `#[derive(Name)]` expands to it. Problem: [18, walls of `#[derive(..)]`](../../problems/18-derive-walls.md).

```rust
rustx::sugar! {
    derive Id = Debug, Clone, Copy, PartialEq, Eq, Hash;

    #[derive(Id, PartialOrd)]
    struct UserId(u64);
}

let id = UserId(7);
assert_eq!(id, id.clone());
assert!(UserId(1) < UserId(2));
```

Becomes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd)]
struct UserId(u64);
```

Presets mix with ordinary derives, and the result is deduplicated in order.

## Limits

- A preset is visible only inside the `sugar!` input that declares it and cannot be exported. The input has one flat namespace: a preset declared in a nested module is usable everywhere in it.
- Presets match by written path: `Debug` and `std::fmt::Debug` are different names. Derives inside macro bodies or nested `cfg_attr`s are not expanded.
- A preset that shadows a std derive name, names itself or is empty is an error.
