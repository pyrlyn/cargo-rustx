rustx::sugar! {
    #[newtype(deref)]
    pub struct User { name: String }

    #[newtype]
    pub struct Pair(u8, u8);

    #[newtype(display)]
    pub enum Kind { A }
}

fn main() {}
