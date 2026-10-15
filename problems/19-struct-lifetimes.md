# Lifetime parameters spreading through structs and `impl` blocks

## As is (Rust)

```rust
pub struct Lexer<'a> { src: &'a str, pos: usize }
impl<'a> Lexer<'a> { pub fn next_token(&mut self) -> Option<Token<'a>> { .. } }
impl<'a> Iterator for Lexer<'a> { type Item = Token<'a>; .. }
```

## Desired (sugar)

```rust
#[borrows] pub struct Lexer { src: &str, pos: usize }
impl Lexer { pub fn next_token(&mut self) -> Option<Token> { .. } }
impl Iterator for Lexer { type Item = Token; .. }
```

## Implementation

`#[borrows]` on a struct or enum adds one lifetime parameter, puts it on every reference in its fields, and passes it to other `#[borrows]` types used in fields (`Peekable<Lexer>` → `Peekable<Lexer<'a>>`). In `impl X` and `impl Trait for X` where `X` is a borrowing type, the macro adds `<'a>`. Inside such impls, borrowing types written without arguments (`Token`) get the same `'a`. For a plain reference that lives as long as the input rather than the receiver, write `&from(Self) str`. In free functions, `&from(Parser) str` names the lifetime of the `Parser` parameter's data (item 1 uses `from(param)` for the borrow of a parameter itself).

## Detailed example

### As is (valid Rust)

```rust
use std::iter::Peekable;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token<'a> {
    Ident(&'a str),
    Number(&'a str),
    Punct(char),
}

pub struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Lexer<'a> {
        Lexer { src, pos: 0 }
    }

    pub fn next_token(&mut self) -> Option<Token<'a>> {
        let src = self.src;
        let rest = src[self.pos..].trim_start();
        let c = rest.chars().next()?;
        let len = if c.is_alphanumeric() {
            rest.find(|ch: char| !ch.is_alphanumeric()).unwrap_or(rest.len())
        } else {
            c.len_utf8()
        };
        self.pos = src.len() - rest.len() + len;
        let text = &rest[..len];
        Some(if c.is_ascii_digit() {
            Token::Number(text)
        } else if c.is_alphabetic() {
            Token::Ident(text)
        } else {
            Token::Punct(c)
        })
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        self.next_token()
    }
}

pub struct Parser<'a> {
    tokens: Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str) -> Parser<'a> {
        Parser { tokens: Lexer::new(src).peekable() }
    }

    pub fn idents(&mut self) -> Vec<&'a str> {
        let mut out = Vec::new();
        while let Some(t) = self.tokens.next() {
            if let Token::Ident(name) = t {
                out.push(name);
            }
        }
        out
    }
}

pub fn first_ident<'a>(p: &mut Parser<'a>) -> Option<&'a str> {
    p.idents().into_iter().next()
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugared] on an item cannot be used
sugar! {
mod lex {
    use std::iter::Peekable;

    #[borrows]
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Token { Ident(&str), Number(&str), Punct(char) }

    #[borrows]
    pub struct Lexer { src: &str, pos: usize }

    impl Lexer {
        pub fn new(src: &from(Self) str) -> Lexer {
            Lexer { src, pos: 0 }
        }

        pub fn next_token(&mut self) -> Option<Token> {
            let src = self.src;
            let rest = src[self.pos..].trim_start();
            let c = rest.chars().next()?;
            let len = if c.is_alphanumeric() {
                rest.find(|ch: char| !ch.is_alphanumeric()).unwrap_or(rest.len())
            } else {
                c.len_utf8()
            };
            self.pos = src.len() - rest.len() + len;
            let text = &rest[..len];
            Some(if c.is_ascii_digit() {
                Token::Number(text)
            } else if c.is_alphabetic() {
                Token::Ident(text)
            } else {
                Token::Punct(c)
            })
        }
    }

    impl Iterator for Lexer {
        type Item = Token;
        fn next(&mut self) -> Option<Token> { self.next_token() }
    }

    #[borrows]
    pub struct Parser { tokens: Peekable<Lexer> }

    impl Parser {
        pub fn new(src: &from(Self) str) -> Parser {
            Parser { tokens: Lexer::new(src).peekable() }
        }

        pub fn idents(&mut self) -> Vec<&from(Self) str> {
            var out = Vec::new();
            while let Some(t) = self.tokens.next() {
                if let Token::Ident(name) = t { out.push(name); }
            }
            out
        }
    }

    pub fn first_ident(p: &mut Parser) -> Option<&from(Parser) str> {
        p.idents().into_iter().next()
    }
}
}
```

### What changed

- Twelve `'a` annotations across four type headers and four `impl` headers are gone. `#[borrows]` says once that the type holds borrowed data.
- The one place where the choice matters stays visible: `&from(Self) str` (lives as long as the input) versus a plain `&str` on a `&mut self` method (would be tied to the receiver).
- The rewrite is purely syntactic: the macro adds the same lifetime to every borrowing type it sees in the same input. Types with two independent lifetimes still need explicit parameters.
- Also: `var`.
