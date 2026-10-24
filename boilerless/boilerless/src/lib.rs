//! Derive away common Rust boilerplate.
//!
//! `#[derive(Boil)]` plus `#[boil(pattern, ...)]` selects patterns; `#[bind(Trait)]` binds an
//! inherent impl block to a trait.
//!
//! | pattern    | item               | generates |
//! |------------|--------------------|-----------|
//! | `builder`  | struct, named      | `<Name>Builder`, `builder()`, `build() -> Result<_, String>` |
//! | `getters`  | struct, named      | `fn field(&self) -> &T` |
//! | `setters`  | struct, named      | `fn set_field(&mut self, impl Into<T>) -> &mut Self` |
//! | `new`      | struct, named      | `fn new(required..) -> Self` |
//! | `error`    | enum               | `Display` (`msg = ".."`), `Error`, `From` (`from`) |
//! | `enum_str` | unit enum          | `as_str`, `Display`, `FromStr` (`rename = ".."`) |
//! | `convert`  | `struct N(T);`     | `From<T> for N`, `From<N> for T` |
//! | `newtype`  | `struct N(T);`     | `Deref`, `DerefMut`, `AsRef<T>`, `into_inner` |
//! | `registry` | unit enum          | `ALL`, `COUNT`, `index`, `variant_name` |
//! | `config`   | struct, named      | `from_env`, `from_lookup` (`config(prefix = "..")`) |
//! | `bind`     | inherent `impl`    | `impl Trait for Type` delegating every method |
//!
//! Field options: `#[boil(default = EXPR)]`, `#[boil(skip)]`.
//!
//! ```
//! use boilerless::Boil;
//!
//! #[derive(Boil, Debug)]
//! #[boil(builder, getters)]
//! struct Server { host: String, #[boil(default = 8080)] port: u16 }
//!
//! let s = Server::builder().host("localhost").build().unwrap();
//! assert_eq!((s.host().as_str(), *s.port()), ("localhost", 8080));
//! ```

pub use boilerless_macros::{Boil, bind};
