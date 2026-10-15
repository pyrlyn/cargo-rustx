rustx::sugar! {
    struct Point;

    impl Point {
        fn copy(self = Point) -> Self {
            self
        }
    }
}

fn main() {}
