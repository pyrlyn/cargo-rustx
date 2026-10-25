#!/usr/bin/env python3
"""Toggle rust-analyzer proc-macro settings in a project's .vscode/settings.json.

Usage: ra-macros [on|off|status] [PROJECT_DIR]   (default: status, current dir)
Idempotent; keeps every other key; backs up the file once before first change.
"""
import json
import re
import shutil
import sys
from pathlib import Path

DIAG_KEY = "rust-analyzer.diagnostics.disabled"
KEYS = [
    "rust-analyzer.procMacro.enable",
    "rust-analyzer.procMacro.attributes.enable",
    "rust-analyzer.cargo.buildScripts.enable",
]
DIAG = ["unresolved-proc-macro", "macro-error"]
MODES = ("on", "off", "status")


class JsoncError(ValueError):
    """The settings file has comments and cannot be rewritten safely."""


def parse(text: str, name: str = "settings.json") -> dict:
    """Parse settings text; tolerates trailing commas, refuses comments."""
    def syntax(match):
        if match.lastgroup == "comment":
            raise JsoncError(f"{name} has comments (JSONC); refusing to rewrite it. Remove comments first.")
        return "" if match.lastgroup == "comma" else match.group()

    # Consume entire quoted strings first, including escaped quotes/backslashes. Syntax-like
    # text inside a value (glob patterns, URLs, or `,}`) must survive byte for byte.
    text = re.sub(r'"(?:[^"\\]|\\.)*"|(?P<comment>//|/\*)|(?P<comma>,)(?=\s*[}\]])', syntax, text)
    data = json.loads(text or "{}")
    if not isinstance(data, dict):
        raise ValueError(f"{name}: settings must be a JSON object")
    return data


def load(path: Path) -> dict:
    """Load settings from `path`; a missing file is empty settings."""
    if not path.exists():
        return {}
    return parse(path.read_text(encoding="utf-8"), str(path))


def apply(data: dict, mode: str) -> dict:
    """Return a new settings dict with proc macros switched `on` or `off`."""
    new = dict(data)
    diag = new.get(DIAG_KEY, [])
    if not isinstance(diag, list) or any(not isinstance(value, str) for value in diag):
        raise ValueError(f"{DIAG_KEY} must be an array of strings")
    diag = list(diag)
    if mode == "on":
        new.update({k: True for k in KEYS})
        diag = [d for d in diag if d not in DIAG]  # show macro errors again
    elif mode == "off":
        new.update({k: False for k in KEYS})
        diag += [d for d in DIAG if d not in diag]  # hide noise when macros are off
    else:
        raise ValueError(f"unknown mode {mode!r}")
    if diag:
        new[DIAG_KEY] = diag
    else:
        new.pop(DIAG_KEY, None)
    return new


def status_lines(data: dict) -> list:
    """Human-readable `key = value` lines for the managed keys."""
    return [f"{k} = {json.dumps(data.get(k, '(default)'))}" for k in KEYS + [DIAG_KEY]]


def settings_path(root: Path) -> Path:
    return root / ".vscode" / "settings.json"


def write(path: Path, data: dict) -> None:
    """Write settings, creating .vscode/ and a one-time .bak of the original."""
    path.parent.mkdir(parents=True, exist_ok=True)
    bak = path.with_suffix(".json.bak")
    if path.exists() and not bak.exists():
        shutil.copy2(path, bak)
    path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def run(mode: str, root: Path, out=sys.stdout, err=sys.stderr) -> bool:
    """Execute one command; returns True if the file changed."""
    if not (root / "Cargo.toml").exists():
        print(f"warning: no Cargo.toml in {root}", file=err)
    path = settings_path(root)
    data = load(path)
    if mode == "status":
        for line in status_lines(data):
            print(line, file=out)
        return False
    new = apply(data, mode)
    if new == data:
        print(f"{path}: already {mode}, nothing changed", file=out)
        return False
    write(path, new)
    print(f"{path}: proc macros {mode}. Reload window or run 'rust-analyzer: Restart server'.", file=out)
    return True


def main(argv=None) -> None:
    argv = sys.argv[1:] if argv is None else argv
    mode = argv[0] if argv else "status"
    root = Path(argv[1] if len(argv) > 1 else ".").expanduser().resolve()
    if mode not in MODES:
        sys.exit(__doc__)
    try:
        run(mode, root)
    except (ValueError, OSError) as e:
        sys.exit(str(e))


if __name__ == "__main__":
    main()
