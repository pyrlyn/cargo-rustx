rustx::sugar! {
    fn make(name: String) -> impl Fn() -> usize {
        move[name.clone(), name.to_uppercase()] || name.len()
    }
}

fn main() {}
