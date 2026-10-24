use boilerless::Boil;
#[derive(Boil, Debug)]
#[boil(error)]
enum E { #[boil(msg = "ok")] A, B }
fn main() {}
