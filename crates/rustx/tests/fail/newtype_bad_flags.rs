rustx::sugar! {
    #[newtype(deref, deref_mut)]
    pub struct A(String);

    #[newtype(display, display)]
    pub struct B(String);

    #[newtype(deref = str)]
    pub struct C(String);

    #[newtype(validate)]
    pub struct D(String);

    #[newtype(display = "{name}")]
    pub struct E(String);
}

fn main() {}
