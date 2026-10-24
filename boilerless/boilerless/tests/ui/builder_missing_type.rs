use boilerless::Boil;
struct Value;
#[derive(Boil)]
#[boil(builder)]
struct S { a: Value }
fn main() { let _: S = S::builder().a("not a value").build().unwrap(); }
