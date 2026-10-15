rustx::sugar! {
    fn plain(n: u8) -> u8 {
        if n == 0 { throw "zero"; }
        n
    }

    fn outer() -> () throws {
        fn inner() -> u8 { throw "inner has no throws"; }
    }
}

fn main() {}
