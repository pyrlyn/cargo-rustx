# Ideas

Not approved yet.

- `#[widen]` in more positions: struct-literal fields (`S { f: e }` and shorthand), method-call arguments, typed `const`s and return positions. Left out of T28 to fit the task size.

- Bug: named arguments do not skip a capture list. `call_defaults/named.rs` `argument_end` reads `h(move[c] |a: u8, b: u8| a)` with `b: u8` as a named argument and fails with "expected `,`" (found during T31.2).
- Bug: a `String` parameter with a string-literal default (`user: String = "admin"`) expands to `unwrap_or("admin")`, which does not compile, and a given argument for it comes out as `Some("ann")`; `string_literals` (T29) could cover both positions (found during T33).
- Bug: a `#[dyn]` trait declared inside a fn body fails with "cannot find trait" (found during T33).
- Share the token helpers `rustx-cli fmt` repeats from `rustx-core` (`parameter_list`, `argument_end`, `angle_depth`, `matching_close`) through a small public API instead of copies (found during T31.1/T31.2).
- Reverse sugar: convert `rc ~T` / `iter ~T` at group starts and in tuple elements (e.g. `(Rc<RefCell<Box<dyn A + Send + Sync>>>, u8)` to `(rc ~A, u8)`); today only the inner `~A` converts there (found during T19.13).
- Reverse sugar: move the multi-place rules (T19.6 `into T`, T19.8 parameter defaults, T19.9 inline bounds, T19.12 field defaults) onto the driver's edit groups from T19.7, so other rules' edits inside their spans are no longer dropped; then add the test T19.11 planned, with `into T` and inline bounds converting in the same fn.
