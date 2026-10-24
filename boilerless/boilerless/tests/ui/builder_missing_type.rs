use boilerless::Boil;
#[derive(Boil)]
#[boil(builder)]
struct S { a: u8 }
fn main() { let _: S = S::builder().a("not a number").build().unwrap(); }
