rustx::sugar! {
    pub struct Config {
        text: String,
    }

    impl Config {
        pub fn first(&self, key: &str) -> &from(Self) str {
            &self.text[..key.len()]
        }
    }
}

fn main() {}
