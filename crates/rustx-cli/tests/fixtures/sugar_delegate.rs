use std::fmt;
use std::io::{self, Write};

/// Something with an area.
pub trait Shape {
    fn area(&self) -> f64;
    // Provided, so not forwarded.
    fn double(&self) -> f64 {
        self.area() * 2.0
    }
}

/// A writer that also counts lines.
pub struct Logged<W> {
    // Every byte goes here.
    inner: W,
    lines: usize,
}

impl<W> Write for Logged<W> {
    #[inline]
    fn write(&mut self, __rustx_arg0: &[u8]) -> io::Result<usize> {
        Write::write(&mut self.inner, __rustx_arg0)
    }
    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.inner)
    }
}

pub struct Label(u8, String);

impl fmt::Display for Label {
    #[inline]
    fn fmt(&self, __rustx_arg0: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.1, __rustx_arg0)
    }
}
impl fmt::Debug for Label {
    #[inline]
    fn fmt(&self, __rustx_arg0: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.1, __rustx_arg0)
    }
}

pub struct Tile {
    shape: Square,
}

impl Shape for Tile {
    #[inline]
    fn area(&self) -> f64 {
        Shape::area(&self.shape)
    }
}

// Adapts the result, which `#[delegate]` would not: stays as written.
pub struct Scaled {
    shape: Square,
}

impl Shape for Scaled {
    fn area(&self) -> f64 {
        Shape::area(&self.shape) * 4.0
    }
}
