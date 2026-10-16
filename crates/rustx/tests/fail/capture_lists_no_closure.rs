rustx::sugar! {
    async fn run(name: String) -> usize {
        async move[name.clone()] { name.len() }.await
    }
}

fn main() {}
