//! Runtime integration tests: one per pattern plus combined/nested cases.

use std::collections::HashMap;

use boilerless::{Boil, bind};

#[derive(Boil, Debug, PartialEq)]
#[boil(builder)]
struct Req {
    url: String,
    #[boil(default = 30)]
    timeout: u32,
    #[boil(skip)]
    retries: u8,
}

#[test]
fn builder() {
    let r = Req::builder().url("u").build().unwrap();
    assert_eq!(
        r,
        Req {
            url: "u".into(),
            timeout: 30,
            retries: 0
        }
    );
    assert_eq!(
        Req::builder()
            .url("u")
            .timeout(5u32)
            .build()
            .unwrap()
            .timeout,
        5
    );
    assert_eq!(Req::builder().build().unwrap_err(), "missing field `url`");
}

#[derive(Boil)]
#[boil(getters)]
struct Person {
    name: String,
    age: u8,
}

#[test]
fn getters() {
    let p = Person {
        name: "Ann".into(),
        age: 3,
    };
    assert_eq!(p.name(), "Ann");
    assert_eq!(*p.age(), 3);
}

#[derive(Boil, Default)]
#[boil(setters)]
struct Opts {
    verbose: bool,
    name: String,
}

#[test]
fn setters() {
    let mut o = Opts::default();
    o.set_verbose(true).set_name("x");
    assert!(o.verbose);
    assert_eq!(o.name, "x");
}

#[derive(Boil, Debug, PartialEq)]
#[boil(new)]
struct Point {
    x: i32,
    y: i32,
    #[boil(default = "origin".to_string())]
    label: String,
    #[boil(skip)]
    hits: u32,
}

#[test]
fn new() {
    assert_eq!(
        Point::new(1, 2i8),
        Point {
            x: 1,
            y: 2,
            label: "origin".into(),
            hits: 0
        }
    );
}

#[derive(Boil, Debug)]
#[boil(error)]
enum AppError {
    #[boil(msg = "not found: {key}")]
    NotFound { key: String },
    #[boil(msg = "bad number: {_0}", from)]
    Parse(std::num::ParseIntError),
    #[boil(msg = "timeout")]
    Timeout,
}

fn parse(s: &str) -> Result<i32, AppError> {
    Ok(s.parse::<i32>()?)
}

#[test]
fn error() {
    assert_eq!(
        AppError::NotFound { key: "a".into() }.to_string(),
        "not found: a"
    );
    assert_eq!(AppError::Timeout.to_string(), "timeout");
    assert_eq!(
        parse("x").unwrap_err().to_string(),
        "bad number: invalid digit found in string"
    );
    let e: Box<dyn std::error::Error> = Box::new(AppError::Timeout);
    assert_eq!(e.to_string(), "timeout");
}

#[derive(Boil, Debug, PartialEq, Clone, Copy)]
#[boil(enum_str, registry)]
enum Color {
    Red,
    #[boil(rename = "dark-blue")]
    Blue,
}

#[test]
fn enum_str() {
    assert_eq!(Color::Blue.as_str(), "dark-blue");
    assert_eq!(Color::Red.to_string(), "Red");
    assert_eq!("dark-blue".parse::<Color>(), Ok(Color::Blue));
    assert_eq!(
        "x".parse::<Color>().unwrap_err(),
        "unknown Color variant: x"
    );
}

#[test]
fn registry() {
    assert_eq!(Color::ALL, &[Color::Red, Color::Blue]);
    assert_eq!(Color::COUNT, 2);
    assert_eq!(Color::Blue.index(), 1);
    assert_eq!(Color::Blue.variant_name(), "Blue");
}

#[derive(Boil, Debug, PartialEq)]
#[boil(convert, newtype)]
struct Meters(f64);

#[test]
fn convert() {
    let m: Meters = 2.5.into();
    assert_eq!(m, Meters(2.5));
    let f: f64 = m.into();
    assert!((f - 2.5).abs() < f64::EPSILON);
}

#[test]
fn newtype() {
    let mut m = Meters(1.0);
    *m += 1.0;
    assert!((m.sqrt() - 2f64.sqrt()).abs() < 1e-9);
    assert!((*m.as_ref() - 2.0).abs() < f64::EPSILON);
    assert!((m.into_inner() - 2.0).abs() < f64::EPSILON);
}

#[derive(Boil, Debug)]
#[boil(config(prefix = "SVC_"))]
struct Settings {
    host: String,
    #[boil(default = 8080)]
    port: u16,
}

#[test]
fn config() {
    let env = HashMap::from([("SVC_HOST", "h"), ("SVC_PORT", "1")]);
    let get = |k: &str| env.get(k).map(|v| (*v).to_string());
    let s = Settings::from_lookup(get).unwrap();
    assert_eq!((s.host.as_str(), s.port), ("h", 1));
    let s = Settings::from_lookup(|k| (k == "SVC_HOST").then(|| "h".to_string())).unwrap();
    assert_eq!(s.port, 8080);
    assert_eq!(
        Settings::from_lookup(|_| None).unwrap_err(),
        "missing config key `SVC_HOST`"
    );
    let bad = Settings::from_lookup(|k| Some(if k == "SVC_PORT" { "x" } else { "h" }.into()));
    assert!(bad.unwrap_err().starts_with("invalid `SVC_PORT`"));
    // from_env reads the real environment; the key is absent here.
    assert!(Settings::from_env().is_err());
}

trait Shape {
    fn area(&self) -> f64;
    fn scale(&mut self, k: f64);
}
struct Square(f64);

#[bind(Shape)]
impl Square {
    fn area(&self) -> f64 {
        self.0 * self.0
    }
    fn scale(&mut self, k: f64) {
        self.0 *= k;
    }
}

#[test]
fn bind() {
    let mut s = Square(2.0);
    Shape::scale(&mut s, 2.0);
    let d: &dyn Shape = &s;
    assert!((d.area() - 16.0).abs() < f64::EPSILON);
}

/// Several patterns on one struct, with nested derived structs.
#[derive(Boil, Debug, PartialEq, Clone)]
#[boil(builder, getters, setters, new)]
struct Inner {
    id: u32,
}

#[derive(Boil, Debug, PartialEq)]
#[boil(builder, getters, setters)]
struct Outer {
    inner: Inner,
    #[boil(default = Inner::new(0u32))]
    backup: Inner,
    tags: Vec<String>,
}

#[test]
fn nested_patterns() {
    let o = Outer::builder()
        .inner(Inner::builder().id(1u32).build().unwrap())
        .tags(vec!["a".to_string()])
        .build()
        .unwrap();
    assert_eq!(*o.inner().id(), 1);
    assert_eq!(o.backup(), &Inner::new(0u32));
    let mut o = o;
    o.set_inner(Inner::new(9u32));
    assert_eq!(o.inner.id, 9);
}

#[derive(Boil)]
#[boil(builder, getters)]
struct Generic<T: Clone> {
    value: T,
}

#[test]
fn generics() {
    let g = Generic::<i32>::builder().value(3).build().unwrap();
    assert_eq!(*g.value(), 3);
}

#[derive(Boil)]
#[boil(builder, new)]
struct Empty {}

#[test]
fn empty_struct() {
    let _ = Empty::new();
    assert!(Empty::builder().build().is_ok());
}
