rustx::sugar! {
    #[borrows]
    pub struct Lexer {
        src: &str,
    }

    impl Lexer {
        pub fn pick(&self, other: &str) -> &from(Self, other) str {
            if other.is_empty() { self.src } else { other }
        }
    }
}

fn main() {}
