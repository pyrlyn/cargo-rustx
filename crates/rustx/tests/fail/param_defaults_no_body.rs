rustx::sugar! {
    trait Connect {
        fn connect(&self, port: u16 = 5432);
    }
}

fn main() {}
