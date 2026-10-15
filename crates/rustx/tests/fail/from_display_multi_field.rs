rustx::sugar! {
    enum Error {
        Pair(from std::io::Error, u8),
        Named { source: from std::fmt::Error },
    }
}

fn main() {}
