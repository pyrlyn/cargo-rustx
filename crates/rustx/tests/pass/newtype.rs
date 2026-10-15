// Problem 20 in full, plus a generic newtype, a `Vec` newtype, a parsed non-string newtype and a
// newtype inside a fn body.
rustx::sugar! {
    mod units {
        #[newtype(deref, as_ref, display, from_str, into, validate = check_email)]
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct Email(String);

        fn check_email(s: &String) -> Result<(), String> {
            if s.contains('@') { Ok(()) } else { Err(format!("invalid email: {s}")) }
        }

        #[newtype(add, sub, mul, div, rem, display = "{} m")]
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        pub struct Meters(pub f64);

        pub fn demo() -> Result<String, String> {
            let e: Email = "ann@example.com".parse()?;
            let total = Meters(1.5) + Meters(2.0);
            let domain = e.split_once('@').map(|(_, d)| d).unwrap_or("");
            Ok(format!("{e} ({domain}, {} chars), {total}", e.len()))
        }
    }

    mod ports {
        #[derive(Debug, PartialEq)]
        pub enum PortError { Parse(std::num::ParseIntError), Zero }

        impl From<std::num::ParseIntError> for PortError {
            fn from(e: std::num::ParseIntError) -> Self { PortError::Parse(e) }
        }

        fn nonzero(p: &u16) -> Result<(), PortError> {
            if *p == 0 { Err(PortError::Zero) } else { Ok(()) }
        }

        // The parse error converts into the validator's error with `From`.
        #[newtype(from_str, display, validate = nonzero)]
        #[derive(Debug)]
        pub struct Port(u16);
    }

    // Generated impls add no bounds, so the struct states what `display` and `from_str` need.
    #[newtype(deref, display, from_str)]
    #[derive(Debug, PartialEq)]
    struct Label<T: std::fmt::Display + std::str::FromStr>(T);

    #[newtype(deref, as_ref, into)]
    struct Bytes(Vec<u8>);

    fn local() -> u32 {
        #[newtype(deref, mul)]
        #[derive(Clone, Copy)]
        struct Count(u32);
        *(Count::new(6) * Count(7))
    }
}

use ports::{Port, PortError};
use units::{Email, Meters};

fn main() {
    assert_eq!(units::demo().unwrap(), "ann@example.com (example.com, 15 chars), 3.5 m");

    // `new` and `FromStr` both go through the validator.
    let e = Email::new("bob@corp.com".to_string()).unwrap();
    assert_eq!(e.to_string(), "bob@corp.com");
    assert_eq!(format!("{e:>14}"), "  bob@corp.com");
    assert_eq!(Email::new("bob".to_string()).unwrap_err(), "invalid email: bob");
    assert_eq!("nobody".parse::<Email>().unwrap_err(), "invalid email: nobody");
    let as_ref: &str = e.as_ref();
    assert_eq!(as_ref, "bob@corp.com");
    let inner: String = e.into();
    assert_eq!(inner, "bob@corp.com");

    let m = Meters::new(6.0);
    assert_eq!((m - Meters(1.0)).0, 5.0);
    assert_eq!((m * Meters(2.0)).0, 12.0);
    assert_eq!((m / Meters(4.0)).0, 1.5);
    assert_eq!((m % Meters(4.0)).0, 2.0);
    assert_eq!(m.to_string(), "6 m");

    assert_eq!("8080".parse::<Port>().unwrap().to_string(), "8080");
    assert_eq!("0".parse::<Port>().unwrap_err(), PortError::Zero);
    assert!(matches!("http".parse::<Port>(), Err(PortError::Parse(_))));
    assert!(Port::new(0).is_err());

    let label = Label::new(7);
    assert_eq!(*label, 7);
    assert_eq!(label.to_string(), "7");
    assert_eq!("12".parse::<Label<u8>>().unwrap(), Label(12));
    assert!("x".parse::<Label<u8>>().is_err());

    let bytes = Bytes::new(vec![1, 2, 3]);
    let slice: &[u8] = bytes.as_ref();
    assert_eq!((bytes.len(), slice), (3, &[1, 2, 3][..]));
    let back: Vec<u8> = bytes.into();
    assert_eq!(back, [1, 2, 3]);

    assert_eq!(local(), 42);
}
