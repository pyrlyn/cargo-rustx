#[rustx::sugared(all)]
mod shapes {
    #[data]
    pub struct Square {
        side: f64,
    }
}

fn main() {
    // The arguments are reported, and the sugar inside still applies.
    assert_eq!(shapes::Square::new(2.0).side(), 2.0);
}
