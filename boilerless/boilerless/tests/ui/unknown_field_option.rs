use boilerless::Boil;
#[derive(Boil)]
#[boil(builder)]
struct S { #[boil(defualt = 1)] a: u8 }
fn main() {}
