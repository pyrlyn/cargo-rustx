# Attribute macros

`#[rustx::data]`, `#[rustx::display]`, `#[rustx::delegate]` and `#[rustx::newtype]` run one sugar item on valid Rust, outside `sugar!`.

```rust
#[rustx::data(builder)]
#[derive(Debug)]
pub struct User {
    name: String,
    #[default(18)]
    age: u8,
}

#[rustx::display]
#[derive(Debug)]
pub enum LoadError {
    #[display("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[rustx::delegate]
pub struct Logged {
    #[delegate(std::io::Write)]
    inner: Vec<u8>,
}

#[rustx::newtype(deref, display, from_str)]
pub struct Name(String);

let user = User::builder().name("ann".to_string()).build().unwrap();
assert_eq!((user.name(), user.age()), ("ann", 18));
let err: LoadError = std::io::Error::other("disk").into();
assert_eq!(err.to_string(), "io error: disk");
let mut logged = Logged { inner: Vec::new() };
std::io::Write::write_all(&mut logged, b"hi").unwrap();
assert_eq!(logged.inner, b"hi");
let name: Name = "bob".parse().unwrap();
assert_eq!((name.len(), name.to_string()), (3, "bob".to_string()));
```

Each generates what its page shows: [`#[data]`](data.md), [`#[display]` and `from`](from-display.md), [`#[delegate]`](delegate.md), [`#[newtype]`](newtype.md). The differences from `sugar!`:

- `from` is spelled `#[from]` on the variant's field, `Io(#[from] io::Error)`.
- `#[delegate]` goes on the struct, with `#[delegate(..)]` on its fields; `#[display]` needs `#[display("..")]` on its variants. An attribute that would generate nothing is an error.
- `#[data]` reads field defaults as `#[default(expr)]`, since `field: T = expr` does not parse on stable Rust.
- Errors are emitted next to the item, which is kept, so code using it adds no cascade of errors.

## Limits

- An attribute macro sees only its item. `#[delegate]` knows only the std table and listed traits, and has no `except(..)`; `#[newtype(validate = f)]` cannot see `f`; `#[data(builder)]` cannot see a type its builder's name collides with. [`#[sugared]`](sugared.md) on a module sees the whole module.
- Token sugar (`var`, `~Trait`, `throws`, field defaults, `derive` presets, ...) needs [`sugar!`](sugar-macro.md).
