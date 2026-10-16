rustx::sugar! {
    // A tuple type starts a group, so `rc` there reads as the value sugar.
    fn pair() -> (rc dyn std::fmt::Debug, u8) {
        todo!()
    }
}

fn main() {}
