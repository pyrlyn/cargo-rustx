rustx::sugar! {
    #[data(builder)]
    pub struct Job { build: u32 }

    #[data(builder)]
    pub struct User { name: String }

    pub struct UserBuilder;
}

fn main() {}
