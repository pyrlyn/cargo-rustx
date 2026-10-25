"""Unit tests for ra_macros (stdlib unittest)."""
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import ra_macros as rm  # noqa: E402


class StringPreservation(unittest.TestCase):
    def test_json_strings_are_never_cleaned_as_syntax(self):
        values = ['x,}', 'x,]', '**/*.rs', '// comment', 'quote: " and slash: \\', 'київ']
        for value in values:
            with self.subTest(value=value):
                text = json.dumps({'value': value})
                self.assertEqual(rm.parse(text), {'value': value})
                self.assertEqual(rm.parse(text[:-1] + ',}'), {'value': value})

    def test_inline_comments_are_rejected(self):
        for text in ['{"a": 1 // comment\n}', '{"a": 1, /* comment */}']:
            with self.subTest(text=text), self.assertRaises(rm.JsoncError):
                rm.parse(text)

    def test_settings_must_be_an_object(self):
        for text in ['[]', 'null', '"text"', '1']:
            with self.subTest(text=text), self.assertRaisesRegex(ValueError, 'object'):
                rm.parse(text)


class ParseLoad(unittest.TestCase):
    def test_trailing_commas(self):
        self.assertEqual(rm.parse('{"a": [1, 2,], "b": 1,}'), {"a": [1, 2], "b": 1})

    def test_empty_text(self):
        self.assertEqual(rm.parse(""), {})

    def test_jsonc_refused(self):
        for text in ['{\n  // c\n  "a": 1}', '{"a": /* c */ 1}']:
            with self.assertRaises(rm.JsoncError):
                rm.parse(text)

    def test_url_with_slashes_is_not_a_comment(self):
        self.assertEqual(rm.parse('{"u": "http://x"}'), {"u": "http://x"})

    def test_missing_file(self):
        self.assertEqual(rm.load(Path("/nonexistent/x/settings.json")), {})


class Apply(unittest.TestCase):
    def test_on(self):
        new = rm.apply({}, "on")
        self.assertTrue(all(new[k] is True for k in rm.KEYS))
        self.assertNotIn(rm.DIAG_KEY, new)

    def test_off(self):
        new = rm.apply({}, "off")
        self.assertTrue(all(new[k] is False for k in rm.KEYS))
        self.assertEqual(new[rm.DIAG_KEY], rm.DIAG)

    def test_preserves_unrelated_keys(self):
        data = {"editor.tabSize": 2, "rust-analyzer.check.command": "clippy"}
        for mode in ("on", "off"):
            new = rm.apply(data, mode)
            self.assertEqual(new["editor.tabSize"], 2)
            self.assertEqual(new["rust-analyzer.check.command"], "clippy")

    def test_diagnostics_merge(self):
        new = rm.apply({rm.DIAG_KEY: ["inactive-code", "macro-error"]}, "off")
        self.assertEqual(new[rm.DIAG_KEY], ["inactive-code", "macro-error", "unresolved-proc-macro"])
        back = rm.apply(new, "on")
        self.assertEqual(back[rm.DIAG_KEY], ["inactive-code"])

    def test_idempotent(self):
        for mode in ("on", "off"):
            once = rm.apply({"x": 1}, mode)
            self.assertEqual(rm.apply(once, mode), once)

    def test_input_not_mutated(self):
        data = {rm.DIAG_KEY: ["a"]}
        rm.apply(data, "off")
        self.assertEqual(data, {rm.DIAG_KEY: ["a"]})

    def test_bad_mode(self):
        with self.assertRaises(ValueError):
            rm.apply({}, "maybe")

    def test_status_lines(self):
        lines = rm.status_lines({"rust-analyzer.procMacro.enable": False})
        self.assertEqual(lines[0], "rust-analyzer.procMacro.enable = false")
        self.assertIn('= "(default)"', lines[1])
        self.assertEqual(len(lines), 4)


class RunOnDisk(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "Cargo.toml").write_text("[package]\nname='x'\n")
        self.path = rm.settings_path(self.root)

    def tearDown(self):
        self.tmp.cleanup()

    def run_mode(self, mode):
        out, err = io.StringIO(), io.StringIO()
        return rm.run(mode, self.root, out, err), out.getvalue(), err.getvalue()

    def test_creates_dir_and_file(self):
        changed, out, err = self.run_mode("off")
        self.assertTrue(changed and self.path.exists())
        self.assertEqual(err, "")
        self.assertFalse(self.path.with_suffix(".json.bak").exists(), "no backup of a new file")

    def test_backup_created_once(self):
        self.path.parent.mkdir()
        self.path.write_text('{"orig": true,}')
        self.run_mode("off")
        bak = self.path.with_suffix(".json.bak")
        self.assertEqual(bak.read_text(), '{"orig": true,}')
        self.run_mode("on")
        self.assertEqual(bak.read_text(), '{"orig": true,}')
        self.assertTrue(json.loads(self.path.read_text())["orig"])

    def test_second_run_no_change(self):
        self.run_mode("on")
        before = self.path.read_text()
        changed, out, _ = self.run_mode("on")
        self.assertFalse(changed)
        self.assertIn("already on", out)
        self.assertEqual(self.path.read_text(), before)

    def test_status_does_not_write(self):
        changed, out, _ = self.run_mode("status")
        self.assertFalse(changed or self.path.exists())
        self.assertIn("(default)", out)

    def test_jsonc_file_untouched(self):
        self.path.parent.mkdir()
        self.path.write_text('{\n // keep\n "a": 1}')
        with self.assertRaises(rm.JsoncError):
            self.run_mode("on")
        self.assertIn("// keep", self.path.read_text())

    def test_warns_without_cargo_toml(self):
        (self.root / "Cargo.toml").unlink()
        _, _, err = self.run_mode("status")
        self.assertIn("no Cargo.toml", err)


if __name__ == "__main__":
    unittest.main()
