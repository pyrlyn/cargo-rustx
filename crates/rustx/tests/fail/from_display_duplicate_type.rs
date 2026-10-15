rustx::sugar! {
    use std::num::ParseIntError;

    enum Error {
        Port(from ParseIntError),
        Count(from ParseIntError),
    }
}

fn main() {}
