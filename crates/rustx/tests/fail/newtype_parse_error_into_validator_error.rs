rustx::sugar! {
    fn nonzero(n: &u16) -> Result<(), String> {
        if *n == 0 { Err("zero".into()) } else { Ok(()) }
    }

    // `u16`'s `ParseIntError` has no `From` into the validator's `String`.
    #[newtype(from_str, validate = nonzero)]
    pub struct Port(u16);
}

fn main() {}
