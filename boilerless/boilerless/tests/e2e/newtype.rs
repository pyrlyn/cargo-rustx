//! E2E `newtype` (rare-but-frequent): delegate a wrapper's API to the inner `Vec`.
//! Manual boilerplate: `Deref`, `DerefMut`, `AsRef`, `into_inner`.
//! Macro solution: `#[boil(newtype)]`.
//! Checks: inner methods are reachable and behave like the manual wrapper.

use boilerless::Boil;
use std::ops::{Deref, DerefMut};

// ---- manual boilerplate ----
struct ManualTags(Vec<String>);
impl Deref for ManualTags { type Target = Vec<String>; fn deref(&self) -> &Vec<String> { &self.0 } }
impl DerefMut for ManualTags { fn deref_mut(&mut self) -> &mut Vec<String> { &mut self.0 } }
impl AsRef<Vec<String>> for ManualTags { fn as_ref(&self) -> &Vec<String> { &self.0 } }

// ---- macro solution ----
#[derive(Boil)]
#[boil(newtype)]
struct Tags(Vec<String>);

fn main() {
    let mut m = ManualTags(vec![]);
    let mut g = Tags(vec![]);
    m.push("a".into());
    g.push("a".into());
    m.push("b".into());
    g.push("b".into());
    assert_eq!(m.len(), g.len());
    assert_eq!(m.as_ref(), g.as_ref());
    assert!(g.contains(&"b".to_string()));
    assert_eq!(g.into_inner(), m.0);
}
