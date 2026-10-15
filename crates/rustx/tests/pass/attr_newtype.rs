// `#[newtype]` as an attribute macro outside `sugar!`, and `validate` through `#[sugared]`, which
// sees the validator next to the struct.
use rustx::newtype;

#[newtype(deref, display, from_str, into)]
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);

#[rustx::newtype(add, display = "{} m")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meters(f64);

#[rustx::sugared]
mod checked {
    #[newtype(deref, from_str, validate = check)]
    #[derive(Debug)]
    pub struct Email(String);

    fn check(s: &String) -> Result<(), &'static str> {
        if s.contains('@') { Ok(()) } else { Err("no @") }
    }
}

fn main() {
    let name: Name = "ann".parse().unwrap();
    assert_eq!(name.len(), 3);
    assert_eq!(name.to_string(), "ann");
    assert_eq!(String::from(Name::new("bob".to_string())), "bob");

    let total = Meters::new(1.5) + Meters::new(2.0);
    assert_eq!(total, Meters::new(3.5));
    assert_eq!(total.to_string(), "3.5 m");

    let email: checked::Email = "a@b.c".parse().unwrap();
    assert_eq!(&*email, "a@b.c");
    assert_eq!(checked::Email::new("nope".to_string()).unwrap_err(), "no @");
}
