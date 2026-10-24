# String literals into `String`

A string literal where the input writes the type `String` becomes `String::from(..)`. Always on, since such code does not compile as plain Rust. Problem: [15, `String` vs `&str` at API boundaries](../../problems/15-string-params.md).

```rust
rustx::sugar! {
    struct Contact {
        name: String,
        email: Option<String>,
        tags: Vec<String>,
    }

    fn greet(name: String) -> String {
        format!("hi {name}")
    }

    fn demo() -> (Contact, String) {
        let mut title: String = "guest";
        title += "!";
        let contact = Contact { name: "Ann", email: Some("ann@example.com"), tags: vec!["work", "vip"] };
        (contact, greet(title))
    }
}

let (contact, greeting) = demo();
assert_eq!((contact.name.as_str(), contact.tags.len()), ("Ann", 2));
assert_eq!(contact.email.as_deref(), Some("ann@example.com"));
assert_eq!(greeting, "hi guest!");
```

Becomes:

```rust
struct Contact {
    name: String,
    email: Option<String>,
    tags: Vec<String>,
}
fn greet(name: String) -> String {
    format!("hi {name}")
}
fn demo() -> (Contact, String) {
    let mut title: String = ::std::string::String::from("guest");
    title += "!";
    let contact = Contact {
        name: ::std::string::String::from("Ann"),
        email: Some(::std::string::String::from("ann@example.com")),
        tags: vec![
            ::std::string::String::from("work"), ::std::string::String::from("vip")
        ],
    };
    (contact, greet(title))
}
```

`title += "!"` stays: `String += &str` is already valid Rust. The problem file writes the conversion as `.to_string()`; the pass emits `::std::string::String::from(..)`, which means the same and fails to compile, instead of converting, when the guess about the type is wrong.

## Limits

- The type must be written, as `String`, `Option<String>` (the literal in `Some(..)`) or `Vec<String>` (the literals of `vec![..]`), and only in fn bodies: a `let` annotation, a local or field assigned, a parameter of a fn of the input called by path, and the fields of a literal of a struct of the input (`Self { .. }` too).
- Return values, method call arguments, `HashMap` keys, `const` and `static` items, struct literals inside macro arguments and anything that is not a literal stay explicit. So do byte and C strings.
- [Parameter defaults](param-defaults.md) also convert literals: `user: String = "admin"` allocates only when omitted. Supplied literals convert in ordinary call expressions; macro arguments remain opaque to this typed conversion.
- `Some` may be qualified as `Option::Some`, `std::option::Option::Some` or `core::option::Option::Some`. Nested `Option` wrappers are followed, including the wrapper introduced for a defaulted parameter.
- Types are recognised by their written path (`String`, `std::string::String`), so an alias or a renamed import is not; structs and fns are matched by name.
