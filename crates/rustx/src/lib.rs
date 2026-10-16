//! cargo-rustx: a thin syntax layer over Rust that desugars into ordinary Rust.
//!
//! The facade users depend on: it re-exports the macros and holds the small runtime types
//! some sugar expands to, such as [`Shared`], so user crates need a single dependency.

mod rc;
mod shared;

/// The crate `#[dyn]` traits expand to, reached as `::rustx::dynosaur` so user crates need no
/// dependency of their own on it. Behind the `dyn` feature, because it brings its own `syn`.
#[cfg(feature = "dyn")]
pub use dynosaur;
pub use rc::RcExt;
pub use rustx_macros::{data, delegate, display, newtype, sugar, sugared};
pub use shared::Shared;

/// The error type behind `-> T throws`, which desugars to `::rustx::anyhow::Result<T>`; the
/// re-export spares users a dependency of their own and a version that could disagree.
pub use anyhow;

/// The pages of `docs/guide`, compiled and run as doc tests so the guide's examples cannot drift
/// from the macros they show. A page that is not listed here is not checked.
#[cfg(doctest)]
mod guide {
    macro_rules! pages {
        ($($(#[$cfg:meta])* $name:ident = $file:literal,)*) => {$(
            $(#[$cfg])*
            #[doc = include_str!(concat!("../../../docs/guide/", $file))]
            pub struct $name;
        )*};
    }

    pages! {
        Index = "README.md",
        SugarMacro = "sugar-macro.md",
        AttributeMacros = "attribute-macros.md",
        Sugared = "sugared.md",
        BuildRs = "build-rs.md",
        Cli = "cli.md",
        Fmt = "fmt.md",
        DynFeature = "dyn-feature.md",
        FromLifetimes = "from-lifetimes.md",
        Var = "var.md",
        InlineBounds = "inline-bounds.md",
        BoxedTraits = "boxed-traits.md",
        FromDisplay = "from-display.md",
        Turbofish = "turbofish.md",
        Delegate = "delegate.md",
        Data = "data.md",
        Iter = "iter.md",
        Throws = "throws.md",
        CaptureLists = "capture-lists.md",
        Shared = "shared.md",
        Widen = "widen.md",
        // `#[dyn]` expands to the `dynosaur` re-export, which exists only with the feature; the
        // dev-dependency of this crate on itself turns it on for `cargo test`.
        #[cfg(feature = "dyn")]
        DynTraits = "dyn-traits.md",
        IntoParams = "into-params.md",
        StringLiterals = "string-literals.md",
        Rc = "rc.md",
        ParamDefaults = "param-defaults.md",
        FieldDefaults = "field-defaults.md",
        DerivePresets = "derive-presets.md",
        Borrows = "borrows.md",
        Newtype = "newtype.md",
        BoundPresets = "bound-presets.md",
    }
}
