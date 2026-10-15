// `#[delegate]` on a struct outside `sugar!`: std traits from the built-in table and a listed
// trait.
use std::io::{self, Write};

pub trait Area {
    fn area(&self) -> f64;
}

pub struct Square(pub f64);

impl Area for Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
}

#[rustx::delegate]
pub struct Logged<W: Write> {
    #[delegate(Write)]
    pub inner: W,
    #[delegate(Area { fn area(&self) -> f64; }, std::fmt::Debug)]
    pub shape: Square,
}

impl std::fmt::Debug for Square {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "square {}", self.0)
    }
}

fn main() -> io::Result<()> {
    let mut logged = Logged { inner: Vec::new(), shape: Square(3.0) };
    logged.write_all(b"hello ")?;
    write!(logged, "{}", 42)?;
    logged.flush()?;
    assert_eq!(logged.inner, b"hello 42");
    assert_eq!(logged.area(), 9.0);
    assert_eq!(format!("{logged:?}"), "square 3");
    Ok(())
}
