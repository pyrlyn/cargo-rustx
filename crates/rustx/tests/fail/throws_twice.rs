rustx::sugar! {
    fn parse(text: &str) -> u32 throws throws {
        text.parse()?
    }
}

fn main() {}
