# ra-macros

Toggle rust-analyzer proc-macro expansion for one project by editing its `.vscode/settings.json`.
Handy when heavy derive macros (such as `boilerless`) slow the IDE down, or when you want to see
macro errors again.

## Usage

```sh
python3 boilerless/tools/ra-macros/ra_macros.py status [PROJECT_DIR]   # show managed keys
python3 boilerless/tools/ra-macros/ra_macros.py off    [PROJECT_DIR]   # disable proc macros + hide their diagnostics
python3 boilerless/tools/ra-macros/ra_macros.py on     [PROJECT_DIR]   # enable proc macros, show diagnostics again
```

`PROJECT_DIR` defaults to the current directory. The script:

- is idempotent: a repeat run prints `nothing changed` and leaves the file as is;
- keeps every other key and merges `rust-analyzer.diagnostics.disabled` instead of replacing it;
- accepts trailing commas but refuses files with comments (JSONC) rather than dropping them;
- creates `.vscode/` when needed and backs the original up once to `settings.json.bak`.

After switching, reload the window or run **rust-analyzer: Restart server**.

## Tests

Trailing commas are accepted outside JSON strings. Values such as `"**/*.rs"`, `"https://host"`
and `"x,}"` remain unchanged when toggling macros. Actual line or block comments are rejected
without rewriting the file. Settings must be a JSON object, and
`rust-analyzer.diagnostics.disabled`, when present, must be an array of strings. Invalid input
produces a concise error and leaves both settings and backups untouched.

These tests run in CI and `just check`; to run them separately from the repository root:

```sh
python3 -m unittest discover boilerless/tools/ra-macros/tests
```
