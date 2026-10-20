//! Key and number bounds named once with the trait-alias workaround, written without sugar.

use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Add;

/// What a map key needs.
pub trait Key: Clone + Eq + Hash {}
impl<__RustxSelf: ?Sized + Clone + Eq + Hash> Key for __RustxSelf {}

// `Self` in a bound is the blanket impl's parameter there.
pub trait Num: Copy + Add<Output = Self> {}
impl<__RustxSelf: ?Sized + Copy + Add<Output = __RustxSelf>> Num for __RustxSelf {}

// A trait with a method of its own is more than its bounds, so it stays a trait.
pub trait Named: Clone {
    fn name(&self) -> String;
}

pub fn count<K: Key>(keys: &[K]) -> HashMap<K, usize> {
    let mut counts = HashMap::new();
    for k in keys {
        *counts.entry(k.clone()).or_insert(0) += 1;
    }
    counts
}

pub fn sum<N: Num>(a: N, b: N) -> N {
    a + b
}
