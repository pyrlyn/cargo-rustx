//! Raw identifier escapes are Rust syntax, not part of keys or variant strings.
#![allow(non_camel_case_types)]

use boilerless::Boil;

#[derive(Boil)]
#[boil(config(prefix = "APP_"))]
struct Config {
    r#type: String,
}

#[derive(Boil, Debug, PartialEq)]
#[boil(enum_str, registry)]
enum Keyword {
    r#type,
    #[boil(rename = "r#explicit")]
    r#match,
}

fn main() {
    let cfg = Config::from_lookup(|key| {
        assert_eq!(key, "APP_TYPE");
        Some("worker".into())
    }).unwrap();
    assert_eq!(cfg.r#type, "worker");
    assert_eq!(Keyword::r#type.as_str(), "type");
    assert_eq!(Keyword::r#type.variant_name(), "type");
    assert_eq!("type".parse::<Keyword>().unwrap(), Keyword::r#type);
    assert_eq!(Keyword::r#match.as_str(), "r#explicit");
    assert_eq!(Keyword::r#match.variant_name(), "match");
}
