# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T10.2 | in progress | P2 | 4 | 0% | Claude / opus-5.5 |
| T10.3 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |

### T10.2. M3: default arguments at call sites

Problem 17. Calls inside the same `sugar!` input to a fn with parameter defaults: omitted trailing arguments become `None`, given ones at defaulted positions `Some(..)`. Callers elsewhere see the real `Option<T>` signature. Split from the original T10.2 to fit the task size; depends on T10.1.

Plan: AST pass `passes/call_defaults.rs`, listed before `param_defaults::lower`. Collect defaulted signatures through `param_defaults::default_of`: path calls resolve to free and associated fns (no receiver) by last segment, `.m(..)` calls to methods with a receiver. Rewrite `Expr::Call`/`Expr::MethodCall`, and calls inside macro arguments that parse as comma-separated expressions (`vec![..]`, `format!(..)`), leaving other macro bodies alone. Errors on the user's tokens: too many arguments, a missing required argument, an ambiguous name. Unit tests, snapshot case, `pass/` test with the problem's `demo`, `fail/` cases.

### T10.3. M3: named arguments at call sites

Problem 17. `connect("db", port: 6432)` inside the same input names a defaulted parameter; arguments are reordered to the declaration and the rest filled as in T10.2. Named arguments are allowed only for defaulted parameters. Depends on T10.2.

Plan: token half in the same pass: in a call's argument group (not a fn parameter list or fn-pointer type), `name: expr` at argument start (a single `:`, not `::`) becomes a marker expression the AST half reads. The AST half places named arguments by parameter name. Errors: unknown name, a name for a parameter without a default, a duplicate, a positional argument after a named one, a named argument to a fn without defaults. Unit tests, snapshot case, `pass/` and `fail/` cases.
