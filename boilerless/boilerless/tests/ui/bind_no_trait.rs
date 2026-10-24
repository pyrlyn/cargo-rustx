use boilerless::bind;
struct D;
#[bind]
impl D { fn f(&self) {} }
fn main() {}
