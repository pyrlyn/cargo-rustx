rustx::sugar! {
    fn sum((a, b): into (u32, u32)) -> u32 {
        a + b
    }
}

fn main() {}
