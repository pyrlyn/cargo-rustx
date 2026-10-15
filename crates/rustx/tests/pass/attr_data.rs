// `#[data]` as an attribute macro on plain Rust, outside `sugar!`.
use rustx::data;

#[data(setters, builder)]
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    id: u64,
    name: String,
    #[default(true)]
    active: bool,
}

mod generic {
    #[rustx::data(with)]
    #[derive(Debug)]
    pub struct Pair<A: Clone, B>
    where
        B: Default,
    {
        left: A,
        right: B,
    }
}

fn main() {
    let mut user = User::new(1, "ann".to_string());
    assert_eq!(user.id(), 1);
    assert_eq!(user.name(), "ann");
    assert!(user.active());
    user.set_active(false);
    assert!(!user.active());

    let built = User::builder().id(2).name("bob".to_string()).build().unwrap();
    assert_eq!(built, User::new(2, "bob".to_string()));
    assert_eq!(User::builder().id(3).build().unwrap_err(), "`User` builder is missing `name`");

    let pair = generic::Pair::new('a', 1u8).with_right(7);
    assert_eq!((*pair.left(), *pair.right()), ('a', 7));
}
