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

        // The struct literal's conversions are no longer needed (string literals into `String`
        // fields of a struct of the input are converted, see `string_literals.rs`); they stay so
        // this test covers `into T` alone. The `insert` keys still need `.into()`: method call
        // arguments stay explicit.
        pub fn demo() -> HashMap<String, Contact> {
            var book = HashMap::new();
            let ann = Contact {
                name: "Ann".to_string(),
                email: Some("ann@example.com".to_owned()),
                tags: vec!["work".to_string(), String::from("vip")],
            };
            book.insert("ann".into(), ann);
            let bob = Contact::new("Bob").with_email("bob@example.org").tag("home");
            book.insert("bob".into(), bob);
            book
        }
    }

    mod mixed {
        pub trait Rename {
            fn rename(&mut self, name: into String);
        }

        impl Rename for String {
            fn rename(&mut self, name: into String) {
                *self = name;
            }
        }

        pub fn tagged(value: T: Clone, mut label: into String) -> (T, String) {
            label.push('!');
            (value.clone(), label)
        }

        pub fn strip(text: &str, prefix: into String) -> &from(text) str {
            text.strip_prefix(prefix.as_str()).unwrap_or(text)
        }

        pub fn widen(n: into u64, ref bytes: into Vec<u8>) -> u64 {
            n + bytes.len() as u64
        }
    }
}

use std::borrow::Cow;

use contacts::Contact;
use mixed::Rename;

fn main() {
    let book = contacts::demo();
    assert_eq!(book["bob"].email_domain(), Some("example.org"));
    assert_eq!(book["bob"].tags, ["home"]);
    assert_eq!(book["ann"].tags, ["work", "vip"]);

    let owned = Contact::new(String::from("Cy")).with_email(String::from("cy@example.net"));
    assert_eq!(owned.name, "Cy");
    assert_eq!(owned.email_domain(), Some("example.net"));

    let other = Contact::new('D').tag(Cow::Borrowed("cow")).tag(&String::from("ref"));
    assert_eq!(other.name, "D");
    assert_eq!(other.tags, ["cow", "ref"]);

    let mut s = String::new();
    s.rename("renamed");
    assert_eq!(s, "renamed");

    assert_eq!(mixed::tagged(7u8, "seven"), (7, String::from("seven!")));
    assert_eq!(mixed::strip("prefix-rest", "prefix-"), "rest");
    assert_eq!(mixed::widen(3u8, *b"ab"), 5);
    assert_eq!(mixed::widen(4u32, "abc"), 7);
}
