//! E2E `getters` + `setters` + `new` (rare-but-frequent): accessor boilerplate on one struct.
//! Manual boilerplate: a constructor, one getter and one setter per field.
//! Macro solution: `#[boil(new, getters, setters)]`.
//! Checks: same observable state through both APIs.

use boilerless::Boil;

// ---- manual boilerplate ----
struct ManualUser { name: String, age: u8 }
impl ManualUser {
    fn new(name: impl Into<String>, age: impl Into<u8>) -> Self { Self { name: name.into(), age: age.into() } }
    fn name(&self) -> &String { &self.name }
    fn age(&self) -> &u8 { &self.age }
    fn set_age(&mut self, v: impl Into<u8>) -> &mut Self { self.age = v.into(); self }
}

// ---- macro solution ----
#[derive(Boil)]
#[boil(new, getters, setters)]
struct User { name: String, age: u8 }

fn main() {
    let mut m = ManualUser::new("ann", 30u8);
    let mut g = User::new("ann", 30u8);
    m.set_age(31u8);
    g.set_age(31u8).set_name("ann");
    assert_eq!((m.name(), m.age()), (g.name(), g.age()));
}
