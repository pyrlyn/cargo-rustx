rustx::sugar! {
    #[derive(Debug)]
    enum Error {
        #[display(missing)]
        Missing(String),
        #[display("width {0:>1$}")]
        Wide(String, usize),
    }
}

fn main() {}
