# `#[newtype]`

`#[newtype(flags)]` on a one-field tuple struct generates `new` and one impl per flag. Problem: [20, newtype boilerplate](../../problems/20-newtypes.md).

```rust
rustx::sugar! {
    #[newtype(deref, display, from_str, validate = check)]
    #[derive(Debug)]
    pub struct Email(String);

    fn check(s: &String) -> Result<(), &'static str> {
        if s.contains('@') { Ok(()) } else { Err("no @") }
    }
}

let email: Email = "a@b.c".parse().unwrap();
assert_eq!((email.len(), email.to_string()), (5, "a@b.c".to_string()));
assert_eq!(Email::new("nope".to_string()).unwrap_err(), "no @");
```

Becomes:

```rust
#[derive(Debug)]
pub struct Email(String);
#[allow(dead_code)]
impl Email {
    pub fn new(__rustx_inner: String) -> ::core::result::Result<Self, &'static str> {
        check(&__rustx_inner)?;
        ::core::result::Result::Ok(Self(__rustx_inner))
    }
}
impl ::core::ops::Deref for Email {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}
impl ::core::fmt::Display for Email {
    fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        ::core::fmt::Display::fmt(&self.0, __rustx_f)
    }
}
impl ::core::str::FromStr for Email {
    type Err = &'static str;
    fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {
        Self::new(::core::convert::From::from(__rustx_s))
    }
}
fn check(s: &String) -> Result<(), &'static str> {
    if s.contains('@') { Ok(()) } else { Err("no @") }
}
```

Flags, applied in the order written: `deref`, `as_ref`, `display` (forwarding, or `display = "{} m"`), `from_str`, `into` (`From<Newtype> for Inner`), and the operators `add`, `sub`, `mul`, `div`, `rem`. With `validate = f`, `new` returns `Result<Self, E>` and `from_str` goes through it; without it `new` is infallible. On valid Rust the same is [`#[rustx::newtype]`](attribute-macros.md).

## Limits

- The validator's error type is read from its signature, so `validate = f` needs `fn f(..) -> Result<(), E>` in the same item list as the struct. A standalone `#[rustx::newtype]` cannot see it: use `sugar!` or [`#[sugared]`](sugared.md) on the module.
- `deref` and `as_ref` lend `str` or `[T]` only for a type written `String` or `Vec<T>`; there is no `DerefMut`. `display` forwards the formatter flags to the inner value.
- The impls add no bounds: a generic newtype states `T: Display` itself.
- Operator flags cannot be combined with `validate`, because they would build unchecked values.
