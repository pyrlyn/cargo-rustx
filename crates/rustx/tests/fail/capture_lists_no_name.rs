rustx::sugar! {
    use std::sync::Arc;

    fn make(shared: Arc<String>) -> impl Fn() -> usize {
        move[Arc::clone(&shared)] || shared.len()
    }
}

fn main() {}
