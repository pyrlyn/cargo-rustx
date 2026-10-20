# `rustx-cli expand` and `rustx-cli sugar`

`rustx-cli` is the command-line tool of the workspace. From a checkout, run it as `cargo run -p rustx-cli -- <command>`. Its third command, [`fmt`](fmt.md), has its own page.

## `expand`: show the plain Rust

```sh
rustx-cli expand <file>
```

A `.rustx` file is desugared as a whole. In a Rust file every `sugar! { .. }`, `rustx::sugar!` and `::rustx::sugar!` invocation is replaced by its expansion, at any depth. For this `lib.rs`:

```rust
use rustx::sugar;

sugar! {
    pub fn total(xs: &[u32]) -> u32 {
        var sum = 0;
        for x in xs {
            sum += x;
        }
        sum
    }
}
```

`rustx-cli expand lib.rs` prints:

```rust
use rustx::sugar;
pub fn total(xs: &[u32]) -> u32 {
    let mut sum = 0;
    for x in xs {
        sum += x;
    }
    sum
}
```

The output is formatted with `prettyplease`, so comments other than doc comments are dropped. Errors are printed as `file:line:column: message` with exit code 1:

```text
bad.rustx:2:5: `throw` is only allowed in a fn returning `T throws`
```

Not expanded: `sugar!` inside other macros' arguments, and the attribute macros. A `sugar!` in expression position is an error.

## `sugar`: move plain Rust onto sugar

```sh
rustx-cli sugar <file.rs>
```

It prints the file rewritten into sugar, as the text of a `.rustx` file: save it as one, or paste it into `sugar! { .. }`. For this file:

```rust
pub fn parse_all(text: &str) -> Vec<u32> {
    // A comment survives.
    let mut all = text.split(',').map(|s| s.trim().parse::<u32>().unwrap()).collect::<Vec<u32>>();
    all.sort();
    all
}
```

`rustx-cli sugar plain.rs` prints (`.rustx` text, not Rust; `crates/rustx-cli/tests/sugar.rs` checks that such output expands back to its input):

```text
pub fn parse_all(text: &str) -> Vec<u32> {
    // A comment survives.
    var all = text.split(',').map(|s| s.trim().parse<u32>().unwrap()).collect<Vec<u32>>();
    all.sort();
    all
}
```

Only code that a sugar item expands back to exactly is rewritten, and only those bytes change: comments and layout stay. So far the rules are [`var`](var.md), [method generics](turbofish.md), [`rc e`, `rc T` and `weak T`](rc.md) with the explicit borrows the `rc` binding makes redundant, [`~Trait`](boxed-traits.md) for `Box<dyn Trait + Send + Sync>`, [`iter T`](iter.md) for `impl Iterator<Item = T> + '_`, [string literals](string-literals.md) for `String::from("lit")` where a `String` is expected, [`into T`](into-params.md) for an `impl Into<T>` parameter with its conversion, [`throws`](throws.md) for a fn returning `Result<T, E>` whose body the `throws` pass would build, and [parameter defaults](param-defaults.md) for an `Option<T>` parameter unwrapped with `unwrap_or` at the top of the body, with its calls, [inline bounds](inline-bounds.md) for a generic used by exactly one parameter, [field defaults](field-defaults.md) for a struct followed by the `impl Default` that field defaults generate, its `..Default::default()` rests becoming `..`, and [`#[delegate]`](delegate.md) for a struct followed by the impls that forward a trait to one of its fields. [`from` variants and `#[display]`](from-display.md) take an enum followed by the `From`, `Display` and `Error` impls that sugar generates. Sugar expands to full std paths (`::std::rc::Rc::new`), and the check counts a short name such as `Rc` or `Option` as that path unless the file gives the name another meaning (its own item, a `use` from elsewhere, a generic parameter, a binding or a glob import). Code that expansion would already change as written, such as `var x` inside a macro call, is reported as an error instead.
