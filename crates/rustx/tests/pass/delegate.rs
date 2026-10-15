rustx::sugar! {
    mod io_wrappers {
        use std::io::{self, Read, Write};

        pub struct CountingWriter<W: Write> {
            #[delegate(Write, except(write))]
            pub inner: W,
            pub bytes: usize,
        }

        impl Write for CountingWriter<W> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let n = self.inner.write(buf)?;
                self.bytes += n;
                Ok(n)
            }
        }

        pub struct Tee<R: Read, W: Write> {
            #[delegate(Read)]
            pub reader: R,
            #[delegate(Write)]
            pub log: W,
        }

        pub fn copy_all(t: &mut Tee<R: Read, W: Write>) -> io::Result<u64> {
            var buf = Vec::new();
            t.read_to_end(&mut buf)?;
            t.write_all(&buf)?;
            // Implicit `usize -> u64` (problem 13) is not implemented yet.
            Ok(buf.len() as u64)
        }
    }

    mod others {
        use std::fmt;

        pub trait Shape {
            const SIDES: u8;
            fn area(&self) -> f64;
            fn scale(&mut self, by: f64);
            fn describe(&self) -> String {
                format!("{} sides, area {}", Self::SIDES, self.area())
            }
        }

        #[derive(Clone, Copy)]
        pub struct Square(pub f64);

        impl Shape for Square {
            const SIDES: u8 = 4;
            fn area(&self) -> f64 {
                self.0 * self.0
            }
            fn scale(&mut self, by: f64) {
                self.0 *= by;
            }
        }

        impl fmt::Display for Square {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "square {}", self.0)
            }
        }

        pub struct Tile(#[delegate(Shape, fmt::Display)] pub Square, pub u8);

        pub struct Countdown {
            #[delegate(Iterator, DoubleEndedIterator)]
            pub range: std::ops::Range<u32>,
            #[delegate(AsRef<str> { fn as_ref(&self) -> &str; })]
            pub label: String,
        }
    }

    mod borrowed {
        #[borrows]
        pub struct Words {
            #[delegate(Iterator)]
            pub it: std::str::SplitWhitespace<'_>,
            #[delegate(Debug)]
            pub src: &str,
        }
    }
}

use io_wrappers::{CountingWriter, Tee};
use others::{Countdown, Shape, Square, Tile};
use std::io::{Cursor, Write};

fn main() {
    let mut counting = CountingWriter { inner: Vec::new(), bytes: 0 };
    counting.write_all(b"hello ").unwrap();
    write!(counting, "{}", 42).unwrap();
    counting.flush().unwrap();
    assert_eq!(counting.bytes, 8);
    assert_eq!(counting.inner, b"hello 42");

    let mut tee = Tee { reader: Cursor::new(b"copied".to_vec()), log: Vec::new() };
    assert_eq!(io_wrappers::copy_all(&mut tee).unwrap(), 6);
    assert_eq!(tee.log, b"copied");

    let mut tile = Tile(Square(2.0), 0);
    tile.scale(1.5);
    assert_eq!(tile.area(), 9.0);
    assert_eq!(<Tile as Shape>::SIDES, 4);
    assert_eq!(tile.describe(), "4 sides, area 9");
    assert_eq!(tile.to_string(), "square 3");

    let mut countdown = Countdown { range: 1..4, label: String::from("go") };
    assert_eq!(countdown.next_back(), Some(3));
    assert_eq!(countdown.by_ref().collect::<Vec<_>>(), [1, 2]);
    assert_eq!(countdown.as_ref(), "go");

    let text = "to be";
    let mut words = borrowed::Words { it: text.split_whitespace(), src: text };
    assert_eq!(words.next(), Some("to"));
    assert_eq!(format!("{words:?}"), "\"to be\"");
}
