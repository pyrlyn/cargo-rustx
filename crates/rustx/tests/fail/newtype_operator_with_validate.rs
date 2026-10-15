rustx::sugar! {
    fn positive(n: &i32) -> Result<(), String> {
        if *n > 0 { Ok(()) } else { Err(format!("{n} is not positive")) }
    }

    #[newtype(deref, sub, validate = positive)]
    pub struct Positive(i32);
}

fn main() {}
