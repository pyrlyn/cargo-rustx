rustx::sugar! {
    #[newtype(deref)]
    pub struct Email(String);
}

fn main() {
    let mut email = Email::new("ann@example.com".to_string());
    // Only `Deref` is generated, so the inner value cannot be changed through it.
    email.push('!');
}
