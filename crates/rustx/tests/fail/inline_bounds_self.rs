rustx::sugar! {
    struct Point;

    impl Point {
        fn copy(self: Self: Clone) -> Self {
            self
        }
    }
}

fn main() {}
