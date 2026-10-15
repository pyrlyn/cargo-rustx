# `~Trait` boxed trait objects

`~Trait` in a type is `Box<dyn Trait + Send + Sync>`. Problem: [04, `dyn Trait` in signatures](../../problems/04-dyn-trait.md).

```rust
rustx::sugar! {
    trait Shape {
        fn area(&self) -> f64;
    }

    struct Square(f64);

    impl Shape for Square {
        fn area(&self) -> f64 {
            self.0 * self.0
        }
    }

    fn largest(items: Vec<~Shape>) -> Option<~Shape> {
        items.into_iter().max_by(|a, b| a.area().total_cmp(&b.area()))
    }
}

let shapes: Vec<Box<dyn Shape + Send + Sync>> = vec![Box::new(Square(1.0)), Box::new(Square(3.0))];
assert_eq!(largest(shapes).map(|s| s.area()), Some(9.0));
```

Becomes:

```rust
trait Shape {
    fn area(&self) -> f64;
}
struct Square(f64);
impl Shape for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}
fn largest(
    items: Vec<
        ::std::boxed::Box<dyn Shape + ::core::marker::Send + ::core::marker::Sync>,
    >,
) -> Option<::std::boxed::Box<dyn Shape + ::core::marker::Send + ::core::marker::Sync>> {
    items.into_iter().max_by(|a, b| a.area().total_cmp(&b.area()))
}
```

Closure traits work the same way: `Vec<~Fn(&Req) -> Resp>`.

## Limits

- The added bounds are always `Send + Sync`; the "configurable bounds" of the problem file are not implemented. Write `Box<dyn Trait>` for anything else.
- `~` without a trait after it is an error.
- `~T` needs a recognisable type position (after `->`, `:`, `<`, `&`, ...). At the start of a group or after `,` outside `<..>` (tuple types, fn-pointer parameters), write the plain form.
- For traits with `async fn` used as trait objects, see [`#[dyn]` traits](dyn-traits.md); `~Trait` is not rewritten to the type generated there.
