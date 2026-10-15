#[rustx::display]
#[derive(Debug)]
enum LoadError {
    Io(#[from] std::io::Error),
}

fn main() {
    // `#[from]` still gives `From`, so only the misuse is reported.
    let _: LoadError = std::io::Error::other("x").into();
}
