#[rustx::display]
#[derive(Debug)]
enum LoadError {
    #[display("pair")]
    Pair(#[from] u8, u16),
    #[display("named")]
    Named { #[from] e: std::io::Error },
}

fn main() {}
