rustx::sugar! {
    fn add(a: i32, b: i32) -> i32 {
        a + b
    }
}

fn main() {
    assert_eq!(add(1, 2), 3);
}
