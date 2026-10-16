rustx::sugar! {
    fn run(xs: &[u8]) -> () throws {
        let check = |x: u8| if x == 0 { throw "zero" } else { x };
        let task = async { throw "async blocks return on their own" };
        xs.iter().for_each(|x| { throw "closures return on their own" });
    }
}

fn main() {}
