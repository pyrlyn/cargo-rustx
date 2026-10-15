rustx::sugar! {
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

            pub fn modules(&self) -> iter &str {
                self.entries.iter().map(|e| e.module.as_str())
            }

            pub fn numbered(&self) -> iter (usize, &Entry) {
                self.entries.iter().enumerate().map(|(i, e)| (i + 1, e))
            }
        }

        pub fn errors_in(log: &Log, module: &str) -> usize {
            log.at_least(Level::Error).filter(|e| e.module == module).count()
        }
    }
}

use log::{Entry, Level, Log};

fn entry(level: Level, module: &str) -> Entry {
    Entry { level, module: module.to_string(), msg: String::new() }
}

fn main() {
    let log = Log {
        entries: vec![
            entry(Level::Info, "net"),
            entry(Level::Error, "net"),
            entry(Level::Error, "db"),
            entry(Level::Debug, "db"),
        ],
    };
    assert_eq!(log.at_least(Level::Warn).count(), 2);
    assert_eq!(log.modules().collect::<Vec<_>>(), ["net", "net", "db", "db"]);
    assert_eq!(log.numbered().last().map(|(i, _)| i), Some(4));
    assert_eq!(log::errors_in(&log, "net"), 1);
    assert!(log.entries.iter().all(|e| e.msg.is_empty()));
}
