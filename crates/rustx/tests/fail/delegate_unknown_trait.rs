rustx::sugar! {
    pub struct Wrapper {
        #[delegate(Shape)]
        inner: u8,
    }
}

fn main() {}
