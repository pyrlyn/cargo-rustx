//! E2E `bind` (rare-but-frequent): expose inherent methods through a trait.
//! Manual boilerplate: a trait impl whose every method forwards to the inherent one.
//! Macro solution: `#[bind(Storage)]` on the inherent impl block.
//! Checks: dynamic dispatch through the trait gives the same results as the manual impl.

use boilerless::bind;
use std::collections::HashMap;

trait Storage {
    fn put(&mut self, k: &str, v: i32);
    fn get(&self, k: &str) -> Option<i32>;
}

// ---- manual boilerplate ----
#[derive(Default)]
struct ManualMem(HashMap<String, i32>);
impl ManualMem {
    fn put(&mut self, k: &str, v: i32) { self.0.insert(k.into(), v); }
    fn get(&self, k: &str) -> Option<i32> { self.0.get(k).copied() }
}
impl Storage for ManualMem {
    fn put(&mut self, k: &str, v: i32) { ManualMem::put(self, k, v) }
    fn get(&self, k: &str) -> Option<i32> { ManualMem::get(self, k) }
}

// ---- macro solution ----
#[derive(Default)]
struct Mem(HashMap<String, i32>);
#[bind(Storage)]
impl Mem {
    fn put(&mut self, k: &str, v: i32) { self.0.insert(k.into(), v); }
    fn get(&self, k: &str) -> Option<i32> { self.0.get(k).copied() }
}

fn exercise(s: &mut dyn Storage) -> (Option<i32>, Option<i32>) {
    s.put("a", 1);
    (s.get("a"), s.get("b"))
}

fn main() {
    assert_eq!(exercise(&mut ManualMem::default()), exercise(&mut Mem::default()));
}
