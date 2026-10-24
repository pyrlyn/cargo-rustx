use boilerless::Boil;
#[derive(Boil)]
#[boil(convert)]
struct W<T>(T);
fn main() {}
