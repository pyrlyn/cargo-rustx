rustx::sugar! {
    #[borrows]
    pub struct Pair<'a> {
        key: &'a str,
        value: &str,
    }
}

fn main() {}
