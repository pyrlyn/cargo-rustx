rustx::sugar! {
    #[dyn(pub DynStore)]
    pub trait Store {
        async fn get(&self) -> u8;
    }
}

fn main() {}
