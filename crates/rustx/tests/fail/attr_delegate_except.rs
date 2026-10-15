use std::io::{self, Write};

#[rustx::delegate]
pub struct Counting {
    #[delegate(Write, except(write))]
    pub inner: Vec<u8>,
}

impl Write for Counting {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
}

fn main() {}
