//! A connection helper with optional arguments, written without sugar.

pub struct Pool {
    size: usize,
}

impl Pool {
    /// A pool of `size` connections.
    pub fn new(size: Option<usize>) -> Self {
        let size: usize = size.unwrap_or(4);
        Pool { size }
    }
}

pub fn connect(host: &str, port: Option<u16>, name: Option<String>) -> String {
    let port: u16 = port.unwrap_or(5432);
    let name: String = name.unwrap_or_else(|| host.to_owned());
    // The name labels the connection in logs.
    format!("{name}@{host}:{port}")
}

pub fn demo() -> Vec<String> {
    let pool = Pool::new(None);
    vec![
        connect("db", None, None),
        connect("db", Some(6432), None),
        connect("db", None, Some(format!("pool of {}", pool.size))),
    ]
}

/// Unwraps without a `let` of its own, so it stays as written.
pub fn retries(limit: Option<u8>) -> u8 {
    limit.unwrap_or(3)
}
