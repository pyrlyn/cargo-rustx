#[rustx::data]
fn build() -> u8 {
    1
}

fn main() {
    // The item is kept, so using it adds no error of its own.
    assert_eq!(build(), 1);
}
