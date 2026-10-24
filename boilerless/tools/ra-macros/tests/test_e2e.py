"""End-to-end tests: run the script as a subprocess against temp Cargo projects."""
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "ra_macros.py"


def make_project(base: Path, name: str) -> Path:
    p = base / name
    (p / "src").mkdir(parents=True)
    (p / "Cargo.toml").write_text(f'[package]\nname = "{name.lower()}"\nversion = "0.1.0"\nedition = "2021"\n')
    (p / "src" / "lib.rs").write_text("")
    return p


def snapshot(root: Path) -> dict:
    return {str(f.relative_to(root)): f.read_bytes() for f in root.rglob("*") if f.is_file()}


class E2E(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        base = Path(self.tmp.name)
        self.a = make_project(base, "A")
        self.b = make_project(base, "B")

    def tearDown(self):
        self.tmp.cleanup()

    def cli(self, *args, cwd):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args], cwd=cwd, capture_output=True, text=True,
            env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
        )

    def settings(self, root):
        return json.loads((root / ".vscode" / "settings.json").read_text())

    def test_targets_other_project_only(self):
        a_before = snapshot(self.a)
        r = self.cli("off", str(self.b), cwd=self.a)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(self.settings(self.b)["rust-analyzer.procMacro.enable"])
        self.assertEqual(snapshot(self.a), a_before, "A must stay untouched")
        r = self.cli("status", str(self.b), cwd=self.a)
        self.assertIn("rust-analyzer.procMacro.enable = false", r.stdout)

    def test_default_cwd(self):
        b_before = snapshot(self.b)
        r = self.cli("on", cwd=self.a)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(self.settings(self.a)["rust-analyzer.procMacro.enable"])
        self.assertEqual(snapshot(self.b), b_before, "B must stay untouched")

    def test_repeat_run_no_change(self):
        self.cli("off", str(self.b), cwd=self.a)
        first = snapshot(self.b)
        r = self.cli("off", str(self.b), cwd=self.a)
        self.assertIn("nothing changed", r.stdout)
        self.assertEqual(snapshot(self.b), first)

    def test_bad_mode_and_jsonc_exit_nonzero(self):
        r = self.cli("toggle", cwd=self.a)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("Usage", r.stderr)
        (self.b / ".vscode").mkdir()
        (self.b / ".vscode" / "settings.json").write_text('{\n// c\n}')
        r = self.cli("on", str(self.b), cwd=self.a)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("JSONC", r.stderr)


if __name__ == "__main__":
    unittest.main()
