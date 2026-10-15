// rustc's error for a type that lacks one of the preset's bounds must point at user code.
// The traits are local: std traits would make rustc quote std's source, which only exists
// where the `rust-src` component is installed, so the pinned stderr would differ.
rustx::sugar! {
    trait Named {
        fn name(&self) -> String;
    }

    trait Tagged {}

    bound Label = Named + Tagged;

    fn show<L: Label>(item: &L) -> String {
        item.name()
    }

    struct Plain;

    impl Named for Plain {
        fn name(&self) -> String {
            "plain".to_string()
        }
    }

    fn run() -> String {
        show(&Plain)
    }
}

fn main() {
    run();
}
