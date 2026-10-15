#[rustx::data(getters)]
struct User {
    name: String,
}

fn main() {
    let _ = User { name: String::new() };
}
