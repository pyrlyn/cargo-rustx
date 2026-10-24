//! Defaults and capture lists must compose with the passes around them.
use rustx::sugar;

sugar! {
    fn greet(name: String = "admin") -> String { name }
    fn optional(name: Option<String> = Some("guest")) -> Option<String> { name }
    fn h(f: impl Fn(u8, u8) -> u8, extra: u8 = 0) -> u8 { f(1, 2) + extra }
    fn check() {
        assert_eq!(greet(), "admin");
        let ann = greet("ann");
        let bob = greet(name: "bob");
        assert_eq!(ann, "ann");
        assert_eq!(bob, "bob");
        assert_eq!(optional(), Some(String::from("guest")));
        let ann = optional(Some("ann"));
        assert_eq!(ann, Some(String::from("ann")));
        let c = 3;
        assert_eq!(h(move[c] |a: u8, b: u8| a + b + c, extra: 4), 10);
    }
}

fn main() { check(); }
