rustx::sugar! {
    pub enum Source {
        File(#[delegate(Read)] std::fs::File),
    }
}

fn main() {}
