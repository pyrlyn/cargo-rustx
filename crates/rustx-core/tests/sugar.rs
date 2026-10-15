//! Snapshot tests of `rustx_core::sugar`: each `sugar_cases/<name>.rs` is converted into
//! `snapshots/sugar__<name>.snap`, and the conversion must expand back to the case's tokens.

use quote::ToTokens;

#[test]
fn cases() {
    insta::glob!("sugar_cases/*.rs", |path| {
        let source = std::fs::read_to_string(path).unwrap();
        let converted = rustx_core::sugar(&source).unwrap();
        let expanded = rustx_core::try_expand(converted.parse().unwrap()).unwrap();
        assert_eq!(
            expanded.into_token_stream().to_string(),
            syn::parse_file(&source)
                .unwrap()
                .into_token_stream()
                .to_string(),
            "{} does not expand back to its input",
            path.display()
        );
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let mut settings = insta::Settings::clone_current();
        settings.remove_snapshot_suffix();
        settings.bind(|| insta::assert_snapshot!(name, converted));
    });
}
