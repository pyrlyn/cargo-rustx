//! The outer macro consumes these tokens; sugar is not invoked.
macro_rules! inspect {
    ($name:ident ! { struct S { a: u8 } }) => { 1 };
    ($($other:tt)*) => { 2 };
}

fn main() { println!("{}", inspect!(sugar! { struct S{a:u8} })); }
