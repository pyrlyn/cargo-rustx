#[cfg(any())]
sugar! {
    fn value() -> u8 { 1 }
    struct Disabled(Missing);
}

#[cfg_attr(all(), cfg(all()))]
rustx::sugar! { fn value() -> u8 { 2 } }

struct S;
impl S {
    #[cfg(any())]
    sugar! { fn missing() -> Missing {} }
}

trait T {
    #[cfg(any())]
    sugar! { fn missing() -> Missing {} }
}

fn main() {
    #[cfg(any())]
    sugar! {
        fn value() -> u8 { 3 }
        sugar! { struct Nested(Missing); }
    }
    println!("{}", value());
}
