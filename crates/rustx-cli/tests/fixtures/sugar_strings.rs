//! Contacts built from string literals, written without sugar.

pub struct Contact {
    pub name: String,
    pub email: Option<String>,
    pub tags: Vec<String>,
}

pub fn ann() -> Contact {
    // Every literal goes where the field writes `String`.
    Contact {
        name: String::from("Ann"),
        email: Some(String::from("ann@example.com")),
        tags: vec![String::from("work"), String::from("friend")],
    }
}

pub fn greeting(contact: &Contact) -> String {
    let mut text: String = String::from("Hello, ");
    text += &contact.name;
    // The literal expands to `String::from`, not `to_string`, so this stays as written.
    let suffix: String = "!".to_string();
    // No written type, so nothing tells the literal to become a `String`.
    let separator = String::from(" ");
    text + &separator + &suffix
}
