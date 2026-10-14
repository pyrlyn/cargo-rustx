//! Snapshot tests: each `cases/<name>.rustx` is expanded and pretty-printed into
//! `snapshots/snapshots__<name>.snap`,
//! so a new sugar item only adds files and never edits a shared one.

use proc_macro2::TokenStream;

#[test]
fn cases() {
    insta::glob!("cases/*.rustx", |path| {
        let source = std::fs::read_to_string(path).unwrap();
        let tokens: TokenStream = source.parse().unwrap();
        let expanded = match rustx_core::try_expand(tokens) {
            Ok(file) => prettyplease::unparse(&file),
            Err(err) => format!("error: {err}"),
        };
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        insta::assert_snapshot!(name, expanded);
    });
}
