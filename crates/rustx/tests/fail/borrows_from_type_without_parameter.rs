rustx::sugar! {
    #[borrows]
    pub struct Parser {
        src: &str,
    }

    pub fn first(text: &str) -> &from(Parser) str {
        text
    }
}

fn main() {}
