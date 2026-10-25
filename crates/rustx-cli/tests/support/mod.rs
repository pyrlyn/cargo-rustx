//! Compile and execute expanded fixtures for CLI behavior regressions.

use std::{fs, path::Path, process::Command};

pub(super) fn compile_and_run(bytes: &[u8], dir: &Path) -> Vec<u8> {
    fs::create_dir_all(dir).unwrap();
    let source = dir.join("expanded.rs");
    let executable = dir.join(format!("expanded{}", std::env::consts::EXE_SUFFIX));
    fs::write(&source, bytes).unwrap();
    let compiled = Command::new("rustc")
        .args(["--edition=2024", "--crate-name", "cli_regression"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let run = Command::new(executable).output().unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    run.stdout
}
