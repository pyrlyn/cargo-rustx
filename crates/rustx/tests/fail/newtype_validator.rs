rustx::sugar! {
    mod checks {
        pub fn check(s: &String) -> Result<(), String> {
            if s.is_empty() { Err("empty".into()) } else { Ok(()) }
        }
    }

    fn flag(s: &String) -> bool {
        !s.is_empty()
    }

    #[newtype(validate = checks::check)]
    pub struct Name(String);

    #[newtype(validate = flag)]
    pub struct Tag(String);
}

fn main() {}
