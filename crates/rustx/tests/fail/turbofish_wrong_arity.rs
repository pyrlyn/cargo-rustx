// rustc's own error on the desugared call must point at the user's tokens.
rustx::sugar! {
    fn parse(s: &str) -> u32 {
        s.parse<u32, u8>().unwrap_or(0)
    }
}

fn main() {}
