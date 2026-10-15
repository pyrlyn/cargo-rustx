//! Settings read through `Result` return types, written without sugar.

use std::collections::HashMap;

#[derive(Debug)]
pub enum ConfigError {
    Missing(&'static str),
    Bad(String),
}

pub fn parse_kv(text: &str) -> Result<HashMap<String, String>, ConfigError> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let (k, v) = line
            .split_once('=')
            .ok_or_else(|| ConfigError::Bad(line.to_string()))?;
        map.insert(k.trim().to_string(), v.trim().to_string());
    }
    Ok(map)
}

pub fn check(kv: &HashMap<String, String>) -> Result<(), ConfigError> {
    for key in ["host", "port"] {
        if !kv.contains_key(key) {
            return Err(ConfigError::Missing(key));
        }
    }
    // Every key is there.
    Ok(())
}

pub trait Source {
    fn read(&self) -> Result<String, ConfigError>;

    fn settings(&self) -> Result<HashMap<String, String>, ConfigError> {
        let kv = parse_kv(&self.read()?)?;
        check(&kv)?;
        Ok(kv)
    }
}

pub fn port(kv: &HashMap<String, String>) -> Result<u16, ConfigError> {
    let text = kv.get("port").ok_or(ConfigError::Missing("port"))?;
    // The tail is already a `Result`, so this fn stays as written.
    text.parse().map_err(|_| ConfigError::Bad(text.clone()))
}
