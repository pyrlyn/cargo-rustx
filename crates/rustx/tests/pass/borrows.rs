// Problem 19 in full, plus a generic borrowing type and `iter &from(Self) T`.
rustx::sugar! {
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

    mod words {
        #[borrows]
        pub struct Split<T> { pub text: &str, pub sep: T }

        impl<T: Copy + Fn(char) -> bool> Split<T> {
            pub fn first(&self) -> Option<&from(Self) str> {
                self.text.split(self.sep).next()
            }
        }

        #[borrows]
        pub struct Words { pub text: &str }

        impl Words {
            pub fn all(&self) -> iter &from(Self) str {
                self.text.split(' ').filter(|w| !w.is_empty())
            }
        }
    }
}

use lex::{Lexer, Parser, Token};

fn main() {
    let tokens: Vec<Token> = Lexer::new("x1 = 42;").collect();
    assert_eq!(
        tokens,
        [Token::Ident("x1"), Token::Punct('='), Token::Number("42"), Token::Punct(';')]
    );

    // The names borrow from the source, not from the parser, which may end first.
    let src = String::from("let a = b + 1");
    let names = Parser::new(&src).idents();
    assert_eq!(names, ["let", "a", "b"]);

    let first = {
        let mut parser = Parser::new(&src);
        lex::first_ident(&mut parser)
    };
    assert_eq!(first, Some("let"));

    // Only the text is borrowed, so the result outlives the splitter.
    let head = {
        let split = words::Split { text: &src, sep: |c: char| c == '=' };
        split.first()
    };
    assert_eq!(head, Some("let a "));

    let all: Vec<&str> = words::Words { text: &src }.all().collect();
    assert_eq!(all, ["let", "a", "=", "b", "+", "1"]);
}
