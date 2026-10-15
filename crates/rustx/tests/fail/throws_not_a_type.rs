rustx::sugar! {
    fn parse(text: &str) -> u32 throws "bad input" {
        text.parse()?
    }
}

fn main() {}
