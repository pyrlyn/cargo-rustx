# Cases

Each case is a small piece of code that is awkward in ordinary Rust, then the same behavior written with rustx. Both versions compile and run as doc tests of the `rustx` crate. The [guide](../guide/README.md) shows what each sugar item expands to, and its limits.

| Case | What gets noisy | Sugar |
| --- | --- | --- |
| [Service settings](settings.md) | `Option` parameters, hand-written `Default`, `Result` returns | field defaults, `into`, named arguments, `throws`, `var` |
| [Worker totals](workers.md) | `Arc<Mutex<_>>`, clone-before-`move`, `u64::from` | `shared`, capture lists, `#[widen]` |
| [Scanning rows](scan.md) | named lifetimes, repeated bounds, turbofish | `&from`, `bound`, `.collect<Vec>()`, `var` |
| [One borrowed view](rows.md) | a lifetime on the struct, the `impl` and every method | `#[borrows]`, `&from(Self)` |
| [Orders](orders.md) | newtype impls, derive lists, `From` / `Display` / `Error`, `Option` arguments | `#[newtype]`, `derive` presets, `#[data]`, `#[display]`, `throws`, `into`, defaults |
| [Directory tree](tree.md) | `Rc<RefCell<_>>` and `.borrow_mut()` on every access | `rc`, `into` |
| [Pluggable sinks](plugins.md) | `Box<dyn Trait + Send + Sync>` | `~Trait`, `var` |
