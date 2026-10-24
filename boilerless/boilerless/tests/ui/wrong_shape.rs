use boilerless::Boil;
#[derive(Boil)]
#[boil(enum_str)]
enum E { A(u8) }
#[derive(Boil)]
#[boil(builder)]
struct T(u8);
fn main() {}
