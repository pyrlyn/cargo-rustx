rustx::sugar! {
    #[derive(Debug, Default)]
    struct Config {
        port: u16 = 8080,
        workers: usize,
    }
}

fn main() {}
