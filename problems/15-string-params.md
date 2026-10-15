# `String` vs `&str` at API boundaries

## As is (Rust)

```rust
fn new(name: impl Into<String>) -> Self { Self { name: name.into() } }
let c = Contact { name: "Ann".to_string(), tags: vec!["vip".to_string()] };
```

## Desired (sugar)

```rust
fn new(name: into String) -> Self { Self { name } }
let c = Contact { name: "Ann", tags: vec!["vip"] };
```

## Implementation

A parameter written as `x: into T` becomes a generic `impl Into<T>` parameter plus `let x: T = x.into();` as the first statement of the body. String literals get `.to_string()` only where the target type is written as `String` in the same `sugar!` input (struct fields, `Option<String>`, `Vec<String>`, `let x: String`), using the same machinery as implicit `into()` (item 13).

## Detailed example

### As is (valid Rust)

```rust
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Contact {
    pub name: String,
    pub email: Option<String>,
    pub tags: Vec<String>,
}

impl Contact {
    pub fn new(name: impl Into<String>) -> Self {
        Contact { name: name.into(), email: None, tags: Vec::new() }
    }

    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn email_domain(&self) -> Option<&str> {
        self.email.as_deref().and_then(|e| e.split_once('@')).map(|(_, d)| d)
    }
}

pub fn demo() -> HashMap<String, Contact> {
    let mut book = HashMap::new();
    let ann = Contact {
        name: "Ann".to_string(),
        email: Some("ann@example.com".to_owned()),
        tags: vec!["work".to_string(), String::from("vip")],
    };
    book.insert("ann".to_string(), ann);
    let bob = Contact::new("Bob").with_email("bob@example.org").tag("home");
    book.insert(String::from("bob"), bob);
    book
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugared] on an item cannot be used
sugar! {
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
}
```

### What changed

- `impl Into<String>` plus a `.into()` buried in the body became one marker on the parameter: `into String`. The conversion happens once, at the top of the function.
- `to_string()`, `to_owned()`, `String::from` and `.into()` are four spellings of the same conversion. In fields whose type is written as `String` they disappear.
- Where the type is not written in the input (the `HashMap` key above), the macro cannot know it, so the conversion stays explicit. See [ARCHITECTURE.md](../ARCHITECTURE.md), Limitations.
- Also: `var`.
