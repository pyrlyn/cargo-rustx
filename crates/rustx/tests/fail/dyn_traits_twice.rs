rustx::sugar! {
    #[dyn]
    #[dyn(AnyStore)]
    pub trait Store {
        async fn get(&self) -> u8;
    }
}

fn main() {}
