rustx::sugar! {
    struct Name(String);

    impl Name {
        fn get(self: into Box<Self>) -> String {
            self.0
        }
    }
}

fn main() {}
