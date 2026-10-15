rustx::sugar! {
    pub struct Tee<W: std::io::Write> {
        #[delegate(Write)]
        first: W,
        #[delegate(Write)]
        second: W,
    }
}

fn main() {}
