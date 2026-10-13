# Trait boilerplate: delegation

## As is (Rust)

```rust
impl Read for Wrapper {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.inner.read(b)
    }
}
```

## Desired (sugar)

```rust
struct Wrapper {
    #[delegate(Read)]
    inner: File,
}
```

## Implementation

Delegation attribute macro (see `delegate`, `ambassador` crates).

## Detailed example

### As is (valid Rust)

```rust
use std::io::{self, Read, Write};

pub struct CountingWriter<W: Write> {
    inner: W,
    pub bytes: usize,
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.bytes += n;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub struct Tee<R: Read, W: Write> {
    reader: R,
    log: W,
}

impl<R: Read, W: Write> Read for Tee<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

impl<R: Read, W: Write> Write for Tee<R, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.log.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.log.flush()
    }
}

pub fn copy_all<R: Read, W: Write>(t: &mut Tee<R, W>) -> io::Result<u64> {
    let mut buf = Vec::new();
    t.read_to_end(&mut buf)?;
    t.write_all(&buf)?;
    Ok(buf.len() as u64)
}
```

### Desired (BetterDiffuseRust)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod io_wrappers {
    use std::io::{self, Read, Write};

    pub struct CountingWriter<W: Write> {
        #[delegate(Write, except(write))]
        inner: W,
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
        reader: R,
        #[delegate(Write)]
        log: W,
    }

    pub fn copy_all(t: &mut Tee<R: Read, W: Write>) -> io::Result<u64> {
        var buf = Vec::new();
        t.read_to_end(&mut buf)?;
        t.write_all(&buf)?;
        Ok(buf.len())   // implicit safe into() usize -> u64
    }
}
}
```

### What changed

- Three forwarding `impl` blocks became a `#[delegate(Trait)]` attribute on the field.
- `except(write)` keeps a hand-written implementation only for the method that matters.
- Also: inline bounds, `var`, implicit `into()`.
