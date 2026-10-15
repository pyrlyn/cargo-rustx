rustx::sugar! {
    trait Fail {}

    fn parse(s: &str) -> Result<u8, E<u8>: Fail> {
        s.len().try_into().map_err(|_| todo!())
    }
}

fn main() {}
