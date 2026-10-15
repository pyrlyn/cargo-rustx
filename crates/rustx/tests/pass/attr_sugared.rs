// `#[sugared]` on a module and on an impl: every attribute-shaped sugar, valid Rust only.
use rustx::sugared;

#[sugared]
mod lexer {
    #[borrows]
    #[data]
    #[derive(Debug)]
    pub struct Token {
        text: &str,
        line: usize,
    }

    pub trait Named {
        fn name(&self) -> String;
    }

    pub struct Ident(pub String);

    impl Named for Ident {
        fn name(&self) -> String {
            self.0.clone()
        }
    }

    // A trait declared in the module and `except(..)` both need the whole module in view.
    pub struct Counted {
        #[delegate(Named)]
        pub ident: Ident,
        #[delegate(std::io::Write, except(write))]
        pub out: Vec<u8>,
        pub writes: usize,
    }

    impl std::io::Write for Counted {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            self.out.write(buf)
        }
    }

    #[derive(Debug)]
    pub enum LexError {
        Utf8(#[from] std::str::Utf8Error),
    }

    pub fn first(src: &[u8]) -> Result<Token<'_>, LexError> {
        let text = std::str::from_utf8(src)?;
        Ok(Token::new(text.split_whitespace().next().unwrap_or(""), 1))
    }
}

pub struct Span {
    start: usize,
    end: usize,
}

// On an impl, the sugar reaches the items declared in its method bodies.
#[sugared]
impl Span {
    pub fn len(&self) -> usize {
        #[data]
        struct Bounds {
            start: usize,
            end: usize,
        }
        let bounds = Bounds::new(self.start, self.end);
        bounds.end() - bounds.start()
    }
}

fn main() {
    use lexer::Named;
    use std::io::Write;

    let token = lexer::first(b"let x").unwrap();
    assert_eq!((token.text(), token.line()), ("let", 1));
    assert!(matches!(lexer::first(&[0xff]), Err(lexer::LexError::Utf8(_))));

    let mut counted = lexer::Counted {
        ident: lexer::Ident("x".into()),
        out: Vec::new(),
        writes: 0,
    };
    counted.write_all(b"ab").unwrap();
    counted.flush().unwrap();
    assert_eq!((counted.name(), counted.out.as_slice(), counted.writes), ("x".to_string(), &b"ab"[..], 1));

    assert_eq!(Span { start: 2, end: 5 }.len(), 3);
}
