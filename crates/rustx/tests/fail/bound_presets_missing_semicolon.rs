rustx::sugar! {
    bound Key = Clone + Eq

    fn same<K: Key>(a: &K, b: &K) -> bool {
        a == b
    }
}

fn main() {}
