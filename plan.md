# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T10.2 | todo | P2 | 4 | 0% | |

### T10.2. M3: default and named arguments at call sites

Problem 17. Calls inside the same `sugar!` input to a fn with defaults: omitted trailing arguments become `None`, given ones `Some(..)`; named arguments `f(a, port: 9000)` are reordered to the declaration. Callers elsewhere see the real `Option<T>` signature. Depends on T10.1.
