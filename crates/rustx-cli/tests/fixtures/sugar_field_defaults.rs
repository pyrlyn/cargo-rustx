//! A server config with a hand-written `Default`, written without sugar.

/// Where the server listens.
#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    // Off until certificates are set up.
    pub tls: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: Into::into("127.0.0.1"),
            port: 8080,
            tls: Default::default(),
        }
    }
}

pub fn local(port: u16) -> Config {
    let mut config = Config {
        port,
        ..Default::default()
    };
    config.tls = port == 443;
    config
}
