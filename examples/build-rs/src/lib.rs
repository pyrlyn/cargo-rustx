//! A library whose code is written in `src/stats.rustx`; `build.rs` desugars it into plain Rust.

include!(concat!(env!("OUT_DIR"), "/src/stats.rs"));
