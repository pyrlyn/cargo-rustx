rustx::sugar! {
    fn add_sink(s: ~&str) {
        drop(s);
    }
}

fn main() {}
