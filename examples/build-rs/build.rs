//! Desugars `src/stats.rustx` into `OUT_DIR`, where `src/lib.rs` includes it.

fn main() {
    if let Err(err) = rustx_build::expand_file("src/stats.rustx") {
        err.emit();
    }
}
