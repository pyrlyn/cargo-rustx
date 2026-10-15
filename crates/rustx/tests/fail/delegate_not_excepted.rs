rustx::sugar! {
    use std::io::{self, Write};

    pub struct Counting<W: Write> {
        #[delegate(Write)]
        inner: W,
    }

    impl Write for Counting<W> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.inner.write(buf)
        }
    }
}

fn main() {}
