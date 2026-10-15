rustx::sugar! {
    #[widen(all)]
    fn total(n: u32) -> u64 {
        let wide: u64 = n;
        wide
    }
}

fn main() {}
