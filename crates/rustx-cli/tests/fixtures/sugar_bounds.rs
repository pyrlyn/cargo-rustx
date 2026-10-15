//! Generic functions with their bounds in `where`, written without sugar.

use std::fmt::Debug;

/// Prints one value.
pub fn show<T>(value: T) where T: Debug {
    println!("{value:?}");
}

pub fn label<'a, K: Copy, V>(key: &'a K, value: V) -> String where K: Debug, V: Clone + Debug {
    let mut copy = value.clone();
    copy = value;
    format!("{key:?}: {copy:?}")
}

// `T` is also the return type, so it stays a generic.
pub fn same<T>(value: T) -> T where T: Clone {
    value.clone()
}
