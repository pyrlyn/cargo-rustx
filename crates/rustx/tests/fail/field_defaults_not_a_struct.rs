rustx::sugar! {
    enum Shape {
        Circle { radius: f64 = 1.0 },
        Square { side: f64 },
    }

    union Bits {
        int: u32 = 0,
        float: f32,
    }
}

fn main() {}
