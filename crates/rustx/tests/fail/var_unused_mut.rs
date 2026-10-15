// The generated `let mut` must carry the span of `var`, so the lint points at the user's token.
#![deny(unused_mut)]

rustx::sugar! {
    fn f() -> i32 {
        var x = 1;
        x
    }
}

fn main() {}
