//! E2E `builder`: an HTTP request builder with a required field and a default.
//! Manual boilerplate: a builder struct of `Option`s, one setter per field, `build()`.
//! Macro solution: `#[boil(builder)]` with `#[boil(default = ..)]`.
//! Checks: same results for complete, defaulted and incomplete builds.

use boilerless::Boil;

// ---- manual boilerplate ----
#[derive(Debug, PartialEq)]
struct ManualReq { url: String, timeout: u32 }
#[derive(Default)]
struct ManualReqBuilder { url: Option<String>, timeout: Option<u32> }
impl ManualReq {
    fn builder() -> ManualReqBuilder { ManualReqBuilder::default() }
}
impl ManualReqBuilder {
    fn url(mut self, v: impl Into<String>) -> Self { self.url = Some(v.into()); self }
    fn timeout(mut self, v: impl Into<u32>) -> Self { self.timeout = Some(v.into()); self }
    fn build(self) -> Result<ManualReq, String> {
        Ok(ManualReq {
            url: self.url.ok_or_else(|| "missing field `url`".to_string())?,
            timeout: self.timeout.unwrap_or(30),
        })
    }
}

// ---- macro solution ----
#[derive(Boil, Debug, PartialEq)]
#[boil(builder)]
struct Req {
    url: String,
    #[boil(default = 30)]
    timeout: u32,
}

fn main() {
    let m = ManualReq::builder().url("a").build().map(|r| (r.url, r.timeout));
    let g = Req::builder().url("a").build().map(|r| (r.url, r.timeout));
    assert_eq!(m, g);
    let m = ManualReq::builder().url("a").timeout(5u32).build().map(|r| r.timeout);
    let g = Req::builder().url("a").timeout(5u32).build().map(|r| r.timeout);
    assert_eq!(m, g);
    assert_eq!(ManualReq::builder().build().unwrap_err(), Req::builder().build().unwrap_err());
}
