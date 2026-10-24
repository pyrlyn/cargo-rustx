# Ideas

Not approved yet.

- `#[widen]` in more positions: struct-literal fields (`S { f: e }` and shorthand), method-call arguments, typed `const`s and return positions. Left out of T28 to fit the task size.

- Bug: a `#[dyn]` trait declared inside a fn body fails with "cannot find trait" (found during T33).
- Reverse sugar: convert `rc ~T` / `iter ~T` at group starts and in tuple elements (e.g. `(Rc<RefCell<Box<dyn A + Send + Sync>>>, u8)` to `(rc ~A, u8)`); today only the inner `~A` converts there (found during T19.13).
- Reverse sugar: move the multi-place rules (T19.6 `into T`, T19.8 parameter defaults, T19.9 inline bounds, T19.12 field defaults) onto the driver's edit groups from T19.7, so other rules' edits inside their spans are no longer dropped; then add the test T19.11 planned, with `into T` and inline bounds converting in the same fn.
