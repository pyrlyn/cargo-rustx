//! Disabled types must not leave behind unconditional generated impls or builders.
rustx::sugar! {
    #[cfg(any())]
    #[data(builder)]
    struct Data { value: u8 }

    #[cfg_attr(all(), cfg(any()))]
    #[newtype(display)]
    struct Wrapped(u8);

    #[cfg(any())]
    struct Defaults { value: u8 = 1 }

    #[cfg(any())]
    struct Delegated { #[delegate(Display)] value: u8 }

    #[cfg(any())]
    #[derive(Debug)]
    #[display]
    enum Error { #[display("value {0}")] Value(from u8) }

    #[cfg_attr(all(), cfg_attr(all(), cfg(all()), derive(Debug)))]
    #[data(builder)]
    struct Enabled { value: u8 }

    #[cfg_attr(all(), derive(Debug))]
    struct EnabledDefaults {
        #[cfg_attr(all(), cfg(any()))]
        missing: Missing = missing(),
        value: u8 = 2,
    }
}

fn main() {
    assert_eq!(Enabled::new(1).value(), 1);
    assert_eq!(Enabled::builder().value(2).build().unwrap().value(), 2);
    assert_eq!(EnabledDefaults::default().value, 2);
}
