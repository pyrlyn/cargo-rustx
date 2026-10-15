// Problem 01 in full and the `from` parts of problem 09. `#[data]` is dropped until its pass
// exists, and `numbered` takes a plain generic instead of the inline bound `it: I: Iterator<..>`.
rustx::sugar! {
    mod config {
        use std::collections::HashMap;

        pub struct Config {
            entries: HashMap<String, String>,
        }

        impl Config {
            pub fn parse(text: &str) -> Config {
                var entries = HashMap::new();
                for line in text.lines() {
                    if let Some((k, v)) = line.split_once('=') {
                        entries.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
                Config { entries }
            }

            pub fn get(&self, key: &str) -> Option<&from(self) str> {
                self.entries.get(key).map(|s| s.as_str())
            }
        }

        pub fn longest_value(cfg: &Config, keys: &[&str]) -> Option<&from(cfg) str> {
            var best = None;
            for k in keys {
                if let Some(v) = cfg.get(k) {
                    if best.map_or(true, |b: &str| v.len() > b.len()) {
                        best = Some(v);
                    }
                }
            }
            best
        }

        pub fn section(name: &str, _sep: &str) -> &from(name) str {
            name.split('.').next().unwrap_or(name)
        }

        pub fn first(xs: &[String], key: &str) -> &from(xs) String {
            xs.iter().find(|x| x.starts_with(key)).unwrap_or(&xs[0])
        }
    }

    mod log {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
        pub enum Level { Debug, Info, Warn, Error }

        pub struct Entry {
            pub level: Level,
            pub module: String,
            pub msg: String,
        }

        pub struct Log {
            pub entries: Vec<Entry>,
        }

        impl Log {
            pub fn at_least(&self, min: Level) -> iter &Entry {
                self.entries.iter().filter(move |e| e.level >= min)
            }

            pub fn grep(&self, needle: &str) -> iter &from(self, needle) Entry {
                self.entries.iter().filter(move |e| e.msg.contains(needle))
            }
        }

        pub fn numbered<I: Iterator<Item = &Entry>>(it: I) -> iter (usize, &from(it) Entry) {
            it.enumerate().map(|(i, e)| (i + 1, e))
        }
    }
}

use config::Config;
use log::{Entry, Level, Log};

fn entry(level: Level, msg: &str) -> Entry {
    Entry { level, module: "net".to_string(), msg: msg.to_string() }
}

fn main() {
    let cfg = Config::parse("a = 1\nlong = 12345\nb=22");
    assert_eq!(cfg.get("b"), Some("22"));
    let best = {
        // The result borrows from `cfg` only, so the key list may end before it is used.
        let keys = vec!["a".to_string(), "long".to_string(), "none".to_string()];
        let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
        config::longest_value(&cfg, &keys)
    };
    assert_eq!(best, Some("12345"));

    let name = String::from("net.tcp.read");
    let head = {
        let sep = String::from(".");
        config::section(&name, &sep)
    };
    assert_eq!(head, "net");

    let xs = vec!["alpha".to_string(), "beta".to_string()];
    let found = {
        let key = String::from("be");
        config::first(&xs, &key)
    };
    assert_eq!(found, "beta");

    let log = Log {
        entries: vec![
            entry(Level::Info, "connect ok"),
            entry(Level::Error, "connect failed"),
            entry(Level::Warn, "slow"),
        ],
    };
    assert_eq!(log.at_least(Level::Warn).count(), 2);
    let needle = String::from("connect");
    let hits: Vec<&str> = log.grep(&needle).map(|e| e.msg.as_str()).collect();
    assert_eq!(hits, ["connect ok", "connect failed"]);
    let last = log::numbered(log.entries.iter()).last().map(|(i, e)| (i, e.level));
    assert_eq!(last, Some((3, Level::Warn)));
}
