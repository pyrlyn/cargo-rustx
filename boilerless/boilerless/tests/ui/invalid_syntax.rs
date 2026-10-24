use boilerless::Boil;
#[derive(Boil)]
#[boil(config(prefix = 42))]
struct S { a: u8 }
fn main() {}
