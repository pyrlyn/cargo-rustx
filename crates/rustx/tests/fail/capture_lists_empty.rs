rustx::sugar! {
    fn make() -> impl Fn() -> u8 {
        move[] || 1
    }
}

fn main() {}
