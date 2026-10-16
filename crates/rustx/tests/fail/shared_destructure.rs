rustx::sugar! {
    fn f(pair: (u8, u8)) {
        shared (a, b) = pair;
    }
}

fn main() {}
