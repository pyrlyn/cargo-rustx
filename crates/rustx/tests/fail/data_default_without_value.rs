rustx::sugar! {
    #[data]
    pub struct User {
        name: String,
        #[default]
        active: bool,
    }
}

fn main() {}
