//! A tour of every boilerless pattern. Run: `cargo run --example showcase`.

use std::collections::HashMap;

use boilerless::{Boil, bind};

#[derive(Boil, Debug)]
#[boil(builder, getters, setters)]
struct Request {
    url: String,
    #[boil(default = 30)]
    timeout_secs: u32,
}

#[derive(Boil, Debug)]
#[boil(error)]
enum AppError {
    #[boil(msg = "not found: {key}")]
    NotFound { key: String },
    #[boil(msg = "parse: {_0}", from)]
    Parse(std::num::ParseIntError),
}

#[derive(Boil, Debug, Clone, Copy, PartialEq)]
#[boil(enum_str, registry)]
enum Level {
    Debug,
    #[boil(rename = "information")]
    Info,
}

#[derive(Boil, Debug)]
#[boil(convert, newtype)]
struct UserId(u64);

#[derive(Boil, Debug)]
#[boil(config(prefix = "APP_"))]
struct Settings {
    name: String,
    #[boil(default = 4)]
    workers: usize,
}

trait Speak {
    fn speak(&self) -> String;
}
struct Dog;
#[bind(Speak)]
impl Dog {
    fn speak(&self) -> String {
        "woof".into()
    }
}

fn main() -> Result<(), AppError> {
    let mut r = Request::builder()
        .url("https://x")
        .build()
        .map_err(|key| AppError::NotFound { key })?;
    r.set_timeout_secs(5u32);
    println!("{} {}", r.url(), r.timeout_secs());
    let n: i32 = "42".parse()?;
    println!("{n} {}", AppError::NotFound { key: "k".into() });
    for l in Level::ALL {
        println!("{} #{}", l, l.index());
    }
    let id = UserId::from(7);
    println!("{} {}", *id + 1, u64::from(id));
    let env = HashMap::from([("APP_NAME", "demo")]);
    let s = Settings::from_lookup(|k| env.get(k).map(|v| (*v).to_string()))
        .map_err(|key| AppError::NotFound { key })?;
    println!("{} {}", s.name, s.workers);
    println!("{}", Speak::speak(&Dog));
    Ok(())
}
