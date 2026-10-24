//! Forward type and const arguments even when arguments cannot infer them.
use boilerless::bind;

trait Measure {
    fn size<T: Copy, const N: usize>(&self) -> usize;
    fn borrow<'a>(&self, text: &'a str) -> &'a str;
}

struct Meter;

#[bind(Measure)]
impl Meter {
    fn borrow<'a>(&self, text: &'a str) -> &'a str { text }
    fn size<T: Copy, const N: usize>(&self) -> usize { std::mem::size_of::<T>() * N }
}

fn main() {
    assert_eq!(Measure::borrow(&Meter, "borrowed"), "borrowed");
    assert_eq!(Measure::size::<u16, 3>(&Meter), 6);
}
