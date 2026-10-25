# `#[data]` constructors and getters

`#[data]` on a struct with named fields generates `new`, getters and, with flags, setters and a builder. Problem: [08, constructors and getters](../../problems/08-constructors-getters.md).

Conditions on the type also gate its generated impls and builder. This applies to `newtype`,
`delegate`, `from` and `display` impls too. Nested `cfg_attr` conditions are preserved; other
branches such as `derive` and `doc` stay on the original type:

```rust
rustx::sugar! {
    #[cfg_attr(all(), cfg(any()))]
    #[data(builder)]
    struct Disabled { value: u8 }

    #[cfg_attr(all(), derive(Debug))]
    #[data]
    struct Enabled { value: u8 }
}
assert_eq!(Enabled::new(7).value(), 7);
```

```rust
rustx::sugar! {
    #[data(setters, builder)]
    #[derive(Debug)]
    pub struct User {
        id: u64,
        name: String,
        #[default(true)]
        active: bool,
    }
}

let mut user = User::new(1, "ann".to_string());
assert_eq!((user.id(), user.name(), user.active()), (1, "ann", true));
user.set_active(false);
assert!(!user.active());
let built = User::builder().id(2).name("bob".to_string()).build().unwrap();
assert_eq!(built.name(), "bob");
assert_eq!(User::builder().id(3).build().unwrap_err(), "`User` builder is missing `name`");
```

Becomes:

```rust
#[derive(Debug)]
pub struct User {
    id: u64,
    name: String,
    active: bool,
}
#[allow(dead_code)]
impl User {
    pub fn new(id: u64, name: String) -> Self {
        Self { id, name, active: true }
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn active(&self) -> bool {
        self.active
    }
    pub fn set_id(&mut self, id: u64) {
        self.id = id;
    }
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }
    pub fn builder() -> UserBuilder {
        UserBuilder {
            id: ::core::option::Option::None,
            name: ::core::option::Option::None,
            active: ::core::option::Option::None,
        }
    }
}
pub struct UserBuilder {
    id: ::core::option::Option<u64>,
    name: ::core::option::Option<String>,
    active: ::core::option::Option<bool>,
}
#[allow(dead_code)]
impl UserBuilder {
    pub fn id(mut self, id: u64) -> Self {
        self.id = ::core::option::Option::Some(id);
        self
    }
    pub fn name(mut self, name: String) -> Self {
        self.name = ::core::option::Option::Some(name);
        self
    }
    pub fn active(mut self, active: bool) -> Self {
        self.active = ::core::option::Option::Some(active);
        self
    }
    pub fn build(self) -> ::core::result::Result<User, &'static str> {
        ::core::result::Result::Ok(User {
            id: self.id.ok_or("`User` builder is missing `id`")?,
            name: self.name.ok_or("`User` builder is missing `name`")?,
            active: self.active.unwrap_or_else(|| true),
        })
    }
}
```

Flags: `setters` (`set_*`), `with` (`with_*`, by value) and `builder` (`NameBuilder`, whose `build` reports a missing field). A field with `#[default(expr)]`, or inside `sugar!` a [field default](field-defaults.md) `field: T = expr`, is left out of `new`. `#[data]` is valid Rust, so it also works as an [attribute macro](attribute-macros.md) outside `sugar!`.

[`rustx-cli sugar`](cli.md) turns a struct followed by exactly what `#[data]` generates for one flag set back into `#[data(..)]` with those flags; anything else after it, such as a doc comment on a method or one more method, stays as written. Field defaults are not recovered: a `new` that leaves a field out, as `#[default(..)]` or `field: T = expr` generates, stays as written.

## Limits

- Getters follow the written type, not the problem file's "by value for `Copy`": primitives and shared references are returned by value, `String` as `&str`, anything else as `&T`, so a user `Copy` type gets a `&T` getter.
- Only structs with named fields. Unknown, repeated or valued flags are errors, as is a builder whose name collides with a field or a type declared in the input.
- The generated methods carry no docs, so a crate with `#![deny(missing_docs)]` fails on them.
- As an attribute macro it cannot see other items, so a builder name colliding with a type elsewhere is not detected.
