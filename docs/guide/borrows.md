# `#[borrows]` types

`#[borrows]` on a struct or enum adds one lifetime to the type, its references and the `impl` headers that name it; `&from(Self) T` and `&from(Type) T` return that lifetime. Problem: [19, lifetime parameters spreading through structs](../../problems/19-struct-lifetimes.md).

```rust
rustx::sugar! {
    #[borrows]
    struct Lexer {
        src: &str,
        pos: usize,
    }

    impl Lexer {
        fn new(src: &from(Self) str) -> Self {
            Lexer { src, pos: 0 }
        }

        fn rest(&self) -> &from(Self) str {
            &self.src[self.pos..]
        }
    }

    fn tail(lexer: &Lexer) -> &from(Lexer) str {
        lexer.rest()
    }
}

let lexer = Lexer::new("let x");
assert_eq!(tail(&lexer), "let x");
```

Becomes:

```rust
struct Lexer<'__rustx_borrows> {
    src: &'__rustx_borrows str,
    pos: usize,
}
impl<'__rustx_borrows> Lexer<'__rustx_borrows> {
    fn new(src: &'__rustx_borrows str) -> Self {
        Lexer { src, pos: 0 }
    }
    fn rest(&self) -> &'__rustx_borrows str {
        &self.src[self.pos..]
    }
}
fn tail<'__rustx_borrows>(lexer: &Lexer<'__rustx_borrows>) -> &'__rustx_borrows str {
    lexer.rest()
}
```

`&from(Self) T` also works in a parameter, as in `new`: an elided `&` in a signature keeps its own lifetime. Other `#[borrows]` types used in fields get the same lifetime. `#[borrows]` is valid Rust, so it also works under [`#[sugared]`](sugared.md).

## Limits

- Exactly one lifetime: a type with two independent lifetimes keeps explicit parameters. A type that already has its own lifetime, or has no reference to fill, is an error.
- Borrowing types are matched by their last path segment across the whole input, ignoring modules.
- Fn bodies are not rewritten, and `fn(&T)` keeps its own elision. `from(Self)` outside a borrowing impl and `from(Type)` without such a parameter are errors.
- Under `#[sugared]`, only impls inside the same item get the lifetime.
