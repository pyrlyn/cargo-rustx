rustx::sugar! {
    #[derive(Debug)]
    enum Error {
        #[display("io error: {0}")]
        Io(from std::io::Error),
        Missing(String),
    }
}

fn main() {}
