pub trait Shape {
    fn area(&self) -> f64;
}

#[rustx::delegate]
pub struct Tile {
    #[delegate(Shape)]
    pub inner: Box<dyn Shape>,
}

fn main() {}
