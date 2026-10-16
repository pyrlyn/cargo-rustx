# `#[widen]` lossless number conversions

Inside what `#[widen]` marks, a value of a written number type that meets a wider written type is converted with `From`. Problem: [13, explicit `as` and `into()`](../../problems/13-as-into.md).

```rust
rustx::sugar! {
    struct Totals {
        bytes: u64,
    }

    #[widen]
    fn add(totals: &mut Totals, chunk: u32) -> u64 {
        let small: u16 = 7;
        let wide: u64 = small;
        totals.bytes += chunk;
        totals.bytes + wide
    }
}

let mut totals = Totals { bytes: 1 };
assert_eq!(add(&mut totals, 2), 10);
```

Becomes:

```rust
struct Totals {
    bytes: u64,
}
fn add(totals: &mut Totals, chunk: u32) -> u64 {
    let small: u16 = 7;
    let wide: u64 = <::core::primitive::u64 as ::core::convert::From<
        ::core::primitive::u16,
    >>::from(small);
    totals.bytes
        += <::core::primitive::u64 as ::core::convert::From<
            ::core::primitive::u32,
        >>::from(chunk);
    totals.bytes + wide
}
```

`#[widen]` goes on a fn, an `impl` or a module; `#![widen]` at the start of the `sugar!` input covers all of it. Nothing is converted without it. It also works under [`#[sugared]`](sugared.md), written after it.

The problem file sketches an implicit `.into()` for every lossless widening, including `let x: u64 = a + b;`. The implementation is narrower: opt-in, only the conversions below, and written `<T as From<S>>::from(e)`, so a binding the macro cannot see fails to compile instead of converting silently.

## Limits

- Only pairs std converts with `From`: `u16 -> usize` but nothing from `usize` or `isize`, and no `u16 -> isize`, `u64 -> f64` or `i32 -> f32`.
- Targets: a `let` annotation, a local or field assigned or compound-assigned (not shifts), or a parameter of a fn of the input called by path (`f(x)`, `Type::f(x)`). Struct literal fields and method call arguments stay explicit.
- Values: a name bound with a written type (`let`, parameter, closure parameter, `for` over a slice, array or `Vec` of known type), a field of a struct of the input reached from one, or `*` of a known reference. Literals, arithmetic (`a + b`), method results and block values stay as written.
- Structs and fns are matched by name; a name declared twice with different types is left alone. `#[widen]` with arguments, twice, or on another item is an error.
