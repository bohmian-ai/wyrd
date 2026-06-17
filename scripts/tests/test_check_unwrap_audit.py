#!/usr/bin/env python3
"""Self-tests for `scripts/check_unwrap_audit.py`.

Run with `python scripts/tests/test_check_unwrap_audit.py`. Exits non-zero
on the first failed assertion. Wired into `mise run check:audit-script`.
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPT_DIR))

from check_unwrap_audit import (  # noqa: E402
    expect_arg_is_named_literal,
    read_string_literal,
    scan_file,
    strip_strings_and_comments,
)


def _has_finding(source: str, line_number: int) -> bool:
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as fh:
        fh.write(source)
        path = Path(fh.name)
    try:
        findings = scan_file(path)
    finally:
        path.unlink(missing_ok=True)
    return any(f.line == line_number for f in findings)


def _scan(source: str) -> list[int]:
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as fh:
        fh.write(source)
        path = Path(fh.name)
    try:
        return [f.line for f in scan_file(path)]
    finally:
        path.unlink(missing_ok=True)


def test_read_string_literal_regular() -> None:
    assert read_string_literal('"hello"', 0) == (7, "hello")


def test_read_string_literal_empty() -> None:
    assert read_string_literal('""', 0) == (2, "")


def test_read_string_literal_raw() -> None:
    assert read_string_literal('r"hello"', 0) == (8, "hello")


def test_read_string_literal_raw_hashed() -> None:
    assert read_string_literal('r#"he"llo"#', 0) == (11, 'he"llo')


def test_read_string_literal_byte() -> None:
    assert read_string_literal('b"abc"', 0) == (6, "abc")


def test_read_string_literal_not_a_literal() -> None:
    assert read_string_literal("not a literal", 0) is None
    assert read_string_literal("var_name", 0) is None


def test_expect_arg_named_literal_permitted() -> None:
    assert expect_arg_is_named_literal('.expect("guarded")', 8)
    assert expect_arg_is_named_literal('.expect( "guarded" )', 8)
    assert expect_arg_is_named_literal('.expect(r#"named"#)', 8)
    assert expect_arg_is_named_literal('.expect("multi word")', 8)


def test_expect_arg_empty_literal_rejected() -> None:
    assert not expect_arg_is_named_literal('.expect("")', 8)


def test_expect_arg_dynamic_rejected() -> None:
    assert not expect_arg_is_named_literal(".expect(msg)", 8)
    assert not expect_arg_is_named_literal(".expect(&msg)", 8)
    assert not expect_arg_is_named_literal(".expect(&format!(\"x\"))", 8)
    assert not expect_arg_is_named_literal('.expect("x".into())', 8)


def test_scan_unwrap_flagged() -> None:
    src = "fn main() { let x = something().unwrap(); }\n"
    assert _has_finding(src, 1)


def test_scan_empty_expect_flagged() -> None:
    src = 'fn main() { let x = something().expect(""); }\n'
    assert _has_finding(src, 1)


def test_scan_dynamic_expect_flagged() -> None:
    src = 'fn main() { let x = something().expect(&format!("oops")); }\n'
    assert _has_finding(src, 1)


def test_scan_named_expect_permitted() -> None:
    src = 'fn main() { let x = something().expect("guarded"); }\n'
    assert _scan(src) == []


def test_scan_raw_named_expect_permitted() -> None:
    src = 'fn main() { let x = something().expect(r#"named"#); }\n'
    assert _scan(src) == []


def test_scan_unwrap_in_cfg_test_ignored() -> None:
    src = (
        "fn prod() {}\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    #[test]\n"
        "    fn ok() {\n"
        "        let x = something().unwrap();\n"
        "    }\n"
        "}\n"
    )
    assert _scan(src) == []


def test_scan_unwrap_inside_string_literal_ignored() -> None:
    src = 'fn main() { let s = "calls .unwrap() inside text"; }\n'
    assert _scan(src) == []


def test_scan_unwrap_inside_raw_string_with_braces_ignored() -> None:
    src = (
        "fn prod() {\n"
        '    let s = r#""spec":{"interface":{"unwrap":1}}"#;\n'
        "}\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    #[test]\n"
        "    fn ok() {\n"
        "        let y = something().unwrap();\n"
        "    }\n"
        "}\n"
    )
    assert _scan(src) == []


def test_strip_strings_preserves_length() -> None:
    src = 'let s = "abc";\nfn f() { let y = r#"x{}y"#; }\n'
    sanitized = strip_strings_and_comments(src)
    assert len(sanitized) == len(src)
    assert "abc" not in sanitized
    assert "x{}y" not in sanitized


def main() -> int:
    failures = 0
    tests = [
        (name, fn)
        for name, fn in sorted(globals().items())
        if name.startswith("test_") and callable(fn)
    ]
    for name, fn in tests:
        try:
            fn()
        except AssertionError as exc:
            failures += 1
            print(f"FAIL {name}: {exc}")
        else:
            print(f"ok   {name}")
    if failures:
        print(f"{failures} failed")
        return 1
    print(f"{len(tests)} passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
