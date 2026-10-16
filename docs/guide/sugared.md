# `#[sugared]`

`#[rustx::sugared]` on an item runs every sugar item that is valid Rust across it: `#[data]`, `#[delegate(..)]` fields, `#[display("..")]`, `#[from]`, `#[newtype]`, `#[borrows]` and `#[widen]`. On an inline module it sees the whole module.

```rust
#[rustx::sugared]
mod lexer {
    #[borrows]
    #[data]
    #[derive(Debug)]
    pub struct Token {
        text: &str,
        line: usize,
    }

    #[newtype(deref, from_str, validate = check)]
    #[derive(Debug)]
    pub struct Email(String);

    fn check(s: &String) -> Result<(), &'static str> {
        if s.contains('@') { Ok(()) } else { Err("no @") }
    }

    pub fn first(src: &str) -> Token<'_> {
        Token::new(src.split_whitespace().next().unwrap_or(""), 1)
    }
}

let token = lexer::first("let x");
assert_eq!((token.text(), token.line()), ("let", 1));
let email: lexer::Email = "a@b.c".parse().unwrap();
assert_eq!(&*email, "a@b.c");
assert!("nope".parse::<lexer::Email>().is_err());
```

Seeing the module is what the single [attribute macros](attribute-macros.md) lack: `#[delegate]` can use the module's traits and `except(..)`, `#[newtype(validate = f)]` finds `f`, and `#[borrows]` reaches the module's impls. `#[widen]` works when written after `#[sugared]`. It is not called `#[sugar]` because `sugar!` holds that name: a crate cannot export a bang macro and an attribute macro under one name.

## Limits

- Valid Rust only: token sugar (`var`, `~Trait`, `throws`, field defaults, `derive` presets, `#[dyn]`, ...) needs [`sugar!`](sugar-macro.md).
- It sees only the item it sits on: `#[borrows]` reaches only the impls inside that item.
