//! E2E `config` (rare-but-frequent): typed settings from environment variables.
//! Manual boilerplate: per field lookup, parse, default or error.
//! Macro solution: `#[boil(config(prefix = "APP_"))]` with `default`.
//! Checks: identical results for full, partial, missing and invalid input.

use boilerless::Boil;
use std::collections::HashMap;

// ---- manual boilerplate ----
#[derive(Debug, PartialEq)]
struct ManualCfg { host: String, port: u16 }
impl ManualCfg {
    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        Ok(Self {
            host: match get("APP_HOST") {
                Some(v) => v.parse().map_err(|e| format!("invalid `APP_HOST`: {e}"))?,
                None => return Err("missing config key `APP_HOST`".into()),
            },
            port: match get("APP_PORT") {
                Some(v) => v.parse().map_err(|e| format!("invalid `APP_PORT`: {e}"))?,
                None => 8080,
            },
        })
    }
}

// ---- macro solution ----
#[derive(Boil, Debug)]
#[boil(config(prefix = "APP_"))]
struct Cfg {
    host: String,
    #[boil(default = 8080)]
    port: u16,
}

fn main() {
    let cases: [&[(&str, &str)]; 4] = [
        &[("APP_HOST", "h"), ("APP_PORT", "1")],
        &[("APP_HOST", "h")],
        &[],
        &[("APP_HOST", "h"), ("APP_PORT", "x")],
    ];
    for case in cases {
        let env: HashMap<&str, &str> = case.iter().copied().collect();
        let get = |k: &str| env.get(k).map(|v| (*v).to_string());
        let m = ManualCfg::from_lookup(get).map(|c| (c.host, c.port));
        let g = Cfg::from_lookup(get).map(|c| (c.host, c.port));
        assert_eq!(m, g, "case {case:?}");
    }
}
