rustx::sugar! {
    #[derive(Debug)]
    enum Error {
        #[display("missing key: {1}")]
        Missing(String),
        #[display("bad {value} in {line}")]
        Bad { value: String },
    }
}

fn main() {}
