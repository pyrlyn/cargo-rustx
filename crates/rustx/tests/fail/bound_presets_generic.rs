rustx::sugar! {
    bound Num<Rhs> = Copy + std::ops::Add<Rhs, Output = Self>;
}

fn main() {}
