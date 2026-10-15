#[rustx::newtype(deref, validate = check)]
pub struct Email(String);

fn check(s: &String) -> Result<(), &'static str> {
    if s.contains('@') { Ok(()) } else { Err("no @") }
}

fn main() {
    // The struct is kept, so using it adds no error of its own.
    let _ = Email(String::new());
}
