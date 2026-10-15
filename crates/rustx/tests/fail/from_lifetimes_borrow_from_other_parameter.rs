rustx::sugar! {
    // Ties the result to `key`, but returns a borrow of `xs`.
    fn first(xs: &[String], key: &str) -> &from(key) str {
        &xs[key.len()]
    }
}

fn main() {}
