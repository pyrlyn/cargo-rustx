// `#[display]` with `#[from]` fields as an attribute macro, problem 05 outside `sugar!`.
use std::io;
use std::num::ParseIntError;

#[rustx::display]
#[derive(Debug)]
pub enum LoadError {
    #[display("io error: {0}")]
    Io(#[from] io::Error),
    #[display("parse error: {0}")]
    Parse(#[from] ParseIntError),
    #[display("missing key: {key}")]
    Missing { key: String },
}

fn parse_port(text: &str) -> Result<u16, LoadError> {
    let line = text
        .lines()
        .find(|l| l.starts_with("port="))
        .ok_or_else(|| LoadError::Missing { key: "port".into() })?;
    Ok(line["port=".len()..].trim().parse()?)
}

fn open(path: &str) -> Result<String, LoadError> {
    Ok(std::fs::read_to_string(path)?)
}

fn main() {
    assert_eq!(parse_port("port= 8080\n").unwrap(), 8080);
    assert_eq!(
        parse_port("host=x").unwrap_err().to_string(),
        "missing key: port"
    );
    assert!(parse_port("port=x").unwrap_err().to_string().starts_with("parse error: "));
    assert!(matches!(open("/no/such/rustx/file"), Err(LoadError::Io(_))));
    let boxed: Box<dyn std::error::Error> = Box::new(LoadError::Missing { key: "k".into() });
    assert_eq!(boxed.to_string(), "missing key: k");
}
