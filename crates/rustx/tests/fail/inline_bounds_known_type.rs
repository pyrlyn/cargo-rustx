rustx::sugar! {
    struct Meters(u32);

    fn twice(m: Meters: Clone) -> u32 {
        m.0 * 2
    }
}

fn main() {}
