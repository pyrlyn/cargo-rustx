# `#[delegate]` fields

`#[delegate(Trait)]` on a field implements the trait for the struct by forwarding to that field. Problem: [07, trait delegation](../../problems/07-trait-delegation.md).

```rust
rustx::sugar! {
    use std::io::Write;

    pub struct Logged {
        #[delegate(Write)]
        inner: Vec<u8>,
        lines: usize,
    }
}

let mut logged = Logged { inner: Vec::new(), lines: 0 };
writeln!(logged, "hello").unwrap();
assert_eq!(logged.inner, b"hello\n");
```

Becomes:

```rust
use std::io::Write;
pub struct Logged {
    inner: Vec<u8>,
    lines: usize,
}
impl ::std::io::Write for Logged {
    #[inline]
    fn write(&mut self, __rustx_arg0: &[u8]) -> ::std::io::Result<usize> {
        ::std::io::Write::write(&mut self.inner, __rustx_arg0)
    }
    #[inline]
    fn flush(&mut self) -> ::std::io::Result<()> {
        ::std::io::Write::flush(&mut self.inner)
    }
}
```

A field may list several traits. The methods come from a built-in table (`Read`, `Write`, `BufRead`, `Seek`, `Iterator`, `DoubleEndedIterator`, `Display`, `Debug`, `Hash`), from a trait declared in the same input, or from a list written out: `#[delegate(Area { fn area(&self) -> f64; })]`. `except(m, ..)` forwards everything but the listed methods and adds the rest to a hand-written `impl Trait for Type` next to the struct. On valid Rust the struct takes [`#[rustx::delegate]`](attribute-macros.md).

[`rustx-cli sugar`](cli.md) turns a struct followed by the forwarding impls this generates back into `#[delegate(..)]` on the field, for a trait in the table or declared in the file.

## Limits

- Only required items are forwarded (`Write::write`, not `write_all`), so provided methods go through the wrapper's own methods, hand-written overrides included.
- No cross-item info: a trait that is neither in the table nor declared in the input needs the list form, and so does a trait of the input with generic parameters.
- `except` matches the hand-written impl by last path segment. Enum and union fields, a trait delegated twice, fns without `self`, typed receivers and generic associated types are errors.
- As an attribute macro it knows only the std table and listed traits, and `except(..)` is not available; [`#[sugared]`](sugared.md) on a module brings both back.
