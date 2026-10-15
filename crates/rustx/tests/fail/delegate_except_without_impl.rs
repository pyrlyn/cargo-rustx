rustx::sugar! {
    pub struct Counting<W: std::io::Write> {
        #[delegate(Write, except(write))]
        inner: W,
    }
}

fn main() {}
