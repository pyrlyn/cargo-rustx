rustx::sugar! {
    pub trait Make {
        fn make() -> Self;
    }

    pub struct Wrapper {
        #[delegate(Make)]
        inner: u8,
    }
}

fn main() {}
