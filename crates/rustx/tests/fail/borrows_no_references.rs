rustx::sugar! {
    #[borrows]
    pub struct Counter {
        count: usize,
        step: fn(&usize) -> usize,
    }
}

fn main() {}
