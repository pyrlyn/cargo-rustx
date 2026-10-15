// rustc's own error on the desugared call must point at the user's tokens.
// The method is local, not `str::parse`: rustc's note would otherwise quote std's source,
// which only exists where the `rust-src` component is installed, so the pinned stderr
// would differ between machines.
rustx::sugar! {
    struct Parser;

    impl Parser {
        fn parse<F: Default>(&self) -> F {
            F::default()
        }
    }

    fn parse(p: &Parser) -> u32 {
        p.parse<u32, u8>()
    }
}

fn main() {}
