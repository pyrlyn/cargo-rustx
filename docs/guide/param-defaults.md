# Parameter defaults and named arguments

`port: u16 = 5432` gives a parameter a default; calls in the same input may leave it out or name it, `connect("db", user: ..)`. Problem: [17, default values](../../problems/17-default-values.md).

```rust
rustx::sugar! {
    fn connect(host: &str, port: u16 = 5432, user: String = String::from("admin")) -> String {
        format!("{user}@{host}:{port}")
    }

    fn demo() -> Vec<String> {
        vec![connect("db"), connect("db", 6432), connect("db", user: String::from("ann"))]
    }
}

assert_eq!(demo(), ["admin@db:5432", "admin@db:6432", "ann@db:5432"]);
```

Becomes:

```rust
fn connect(
    host: &str,
    port: ::core::option::Option<u16>,
    user: ::core::option::Option<String>,
) -> String {
    let port: u16 = port.unwrap_or(5432);
    let user: String = user.unwrap_or_else(|| String::from("admin"));
    format!("{user}@{host}:{port}")
}
fn demo() -> Vec<String> {
    vec![
        connect("db", ::core::option::Option::None, ::core::option::Option::None),
        connect("db", ::core::option::Option::Some(6432), ::core::option::Option::None),
        connect("db", ::core::option::Option::None,
        ::core::option::Option::Some(String::from("ann")))
    ]
}
```

A constant default uses `unwrap_or`, any other `unwrap_or_else(|| ..)`, so a default can use earlier parameters. Named arguments are allowed only for defaulted parameters. Field defaults are a separate item: [field defaults](field-defaults.md).

## Limits

- Defaults and named arguments work only for calls inside the same `sugar!` input. Callers elsewhere see the real `Option<T>` signature.
- Calls are matched by name: a path call by its last segment (and the qualifying type, when it is declared in the input), a method call by its name alone, so a defaulted method `len` also rewrites `vec.len()`. Same-named fns that would read a call differently are an error; `<T as Trait>::f(..)` is not resolved.
- Named arguments are evaluated in declaration order, not in the order written, and a call with generic arguments (`f::<T>(..)`) takes none.
- Defaults must be trailing. A bare `<` in a default reads as generics: write `ok: bool = (a < b)`. A non-constant default runs in a closure, so `?`, `return` and `.await` in it act on that closure.
- A string literal is not converted into a `String` default or argument (see [string literals](string-literals.md)).

## Owned string defaults

A string literal in a `String` default becomes an owned string. Allocation happens only
when the argument is omitted. Supplied literals in ordinary call expressions are converted
too, including named arguments. As with other typed string conversions, macro token bodies
are not typed positions: bind the call result before passing it to an assertion macro.

```rust
use rustx::sugar;
sugar! {
    fn greet(name: String = "admin") -> String { name }
}
assert_eq!(greet(None), "admin");
assert_eq!(greet(Some(String::from("ann"))), "ann");
```

Inside the sugar body, the calls are `greet()`, `greet("ann")`, or `greet(name: "ann")`.
Outside it, use the expanded `Option<String>` signature as above. The compiled
`tests/pass/default_strings_and_captures.rs` example exercises all three sugar spellings.
