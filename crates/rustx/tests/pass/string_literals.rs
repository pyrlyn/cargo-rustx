// Problem 15 through `sugar!`: string literals going where the input writes `String`,
// `Option<String>` or `Vec<String>` need no conversion.
rustx::sugar! {
    mod contacts {
        use std::collections::HashMap;

        #[derive(Debug, Clone)]
        pub struct Contact {
            pub name: String,
            pub email: Option<String>,
            pub tags: Vec<String>,
        }

        impl Contact {
            pub fn new(name: into String) -> Self {
                Contact { name, email: None, tags: Vec::new() }
            }

            pub fn with_email(mut self, email: into String) -> Self {
                self.email = Some(email);
                self
            }

            pub fn tag(mut self, tag: into String) -> Self {
                self.tags.push(tag);
                self
            }

            pub fn email_domain(&self) -> Option<&str> {
                self.email.as_deref().and_then(|e| e.split_once('@')).map(|(_, d)| d)
            }
        }

        pub fn demo() -> HashMap<String, Contact> {
            var book = HashMap::new();
            let ann = Contact {
                name: "Ann",
                email: Some("ann@example.com"),
                tags: vec!["work", "vip"],
            };
            book.insert("ann".into(), ann);   // key type is not written here: stays explicit
            let bob = Contact::new("Bob").with_email("bob@example.org").tag("home");
            book.insert("bob".into(), bob);
            book
        }
    }

    mod positions {
        pub struct Label { pub text: String, pub alt: &'static str }

        pub const DEFAULT: &str = "default";

        pub fn greet(name: String, tags: Vec<String>) -> usize {
            name.len() + tags.len()
        }

        pub fn all(label: &mut Label) -> (usize, Vec<String>) {
            let title: String = "title";
            var note: Option<String> = None;
            assert!(note.is_none());
            note = Some(r"raw note");
            label.text = "changed";
            label.text += "!";
            let size = greet("Ann", vec!["a"; 2]);
            let kept = Label { text: title, alt: "alt" };
            let untyped = "untyped";
            let bytes: &[u8] = b"bytes";
            let all: Vec<String> = vec![
                "first",
                label.text.clone(),
                kept.text,
                kept.alt.to_owned(),
                untyped.to_owned(),
                DEFAULT.into(),
                note.unwrap_or_default(),
            ];
            (size + bytes.len(), all)
        }
    }
}

fn main() {
    let book = contacts::demo();
    assert_eq!(book["ann"].name, "Ann");
    assert_eq!(book["ann"].email_domain(), Some("example.com"));
    assert_eq!(book["ann"].tags, ["work", "vip"]);
    assert_eq!(book["bob"].email_domain(), Some("example.org"));
    assert_eq!(book["bob"].tags, ["home"]);

    let mut label = positions::Label { text: String::new(), alt: "" };
    let (size, all) = positions::all(&mut label);
    assert_eq!(size, 10);
    assert_eq!(
        all,
        ["first", "changed!", "title", "alt", "untyped", "default", "raw note"]
    );
}
