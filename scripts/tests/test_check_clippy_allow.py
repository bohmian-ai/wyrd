#!/usr/bin/env python3
"""Self-tests for `scripts/check_clippy_allow.py`.

Run with `python scripts/tests/test_check_clippy_allow.py`. Exits non-zero
on the first failed assertion. Wired into `mise run check:clippy-allow-audit:self`.
"""

from __future__ import annotations

import sys
import tempfile
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPT_DIR))

from check_clippy_allow import ROOT, is_ignored_path, scan_file  # noqa: E402


def _scan(source: str) -> list[int]:
    with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as fh:
        fh.write(source)
        path = Path(fh.name)
    try:
        return [f.line for f in scan_file(path)]
    finally:
        path.unlink(missing_ok=True)


def test_bare_allow_is_flagged() -> None:
    src = "#[allow(clippy::cast_possible_wrap)]\nfn f() {}\n"
    assert _scan(src) == [1]


def test_multi_lint_allow_is_flagged() -> None:
    src = "#[allow(clippy::cast_precision_loss, clippy::cast_sign_loss)]\nfn f() {}\n"
    assert _scan(src) == [1]


def test_inner_crate_allow_is_flagged() -> None:
    src = "#![allow(clippy::too_many_lines)]\n"
    assert _scan(src) == [1]


def test_cfg_attr_allow_is_flagged() -> None:
    src = "#[cfg_attr(feature = \"foo\", allow(clippy::cast_sign_loss))]\nfn f() {}\n"
    assert _scan(src) == [1]


def test_allow_with_justification_on_prev_line_passes() -> None:
    src = (
        "// justification: LSN u64 fits in i64 for the stream lifetime\n"
        "#[allow(clippy::cast_possible_wrap)]\n"
        "fn f() {}\n"
    )
    assert _scan(src) == []


def test_allow_with_justification_two_lines_up_passes() -> None:
    src = (
        "// justification: parquet stats are always positive by writer invariant\n"
        "// second line explaining more detail\n"
        "#[allow(clippy::cast_sign_loss)]\n"
        "fn f() {}\n"
    )
    assert _scan(src) == []


def test_allow_with_blank_line_before_justification_is_flagged() -> None:
    src = (
        "// justification: real reason\n"
        "\n"
        "#[allow(clippy::cast_possible_wrap)]\n"
        "fn f() {}\n"
    )
    assert _scan(src) == [3]


def test_allow_with_unrelated_comment_is_flagged() -> None:
    src = (
        "// TODO fix this later\n"
        "#[allow(clippy::cast_possible_wrap)]\n"
        "fn f() {}\n"
    )
    assert _scan(src) == [2]


def test_justification_needs_text_after_colon() -> None:
    src = (
        "// justification:\n"
        "#[allow(clippy::cast_possible_wrap)]\n"
        "fn f() {}\n"
    )
    assert _scan(src) == [2]


def test_non_clippy_allow_is_not_flagged() -> None:
    src = "#[allow(dead_code)]\nfn f() {}\n"
    assert _scan(src) == []


def test_deny_and_warn_are_not_flagged() -> None:
    src = (
        "#[deny(clippy::cast_possible_wrap)]\n"
        "#[warn(clippy::cast_possible_wrap)]\n"
        "fn f() {}\n"
    )
    assert _scan(src) == []


def test_is_ignored_path_skips_tests_dir() -> None:
    assert is_ignored_path(ROOT / "crates" / "foo" / "tests" / "bar.rs")


def test_is_ignored_path_skips_examples_dir() -> None:
    assert is_ignored_path(ROOT / "crates" / "foo" / "examples" / "bar.rs")


def test_is_ignored_path_skips_benches_dir() -> None:
    assert is_ignored_path(ROOT / "crates" / "foo" / "benches" / "bench_x.rs")


def test_is_ignored_path_skips_pg_tests_file() -> None:
    p = ROOT / "crates" / "vala" / "vala-bifrost" / "src" / "writer" / "commit" / "pg_tests.rs"
    assert is_ignored_path(p)


def test_is_ignored_path_skips_pg_prefix_file() -> None:
    p = ROOT / "crates" / "foo" / "src" / "pg_setup.rs"
    assert is_ignored_path(p)


def test_is_ignored_path_skips_pg_tests_dir() -> None:
    p = ROOT / "crates" / "foo" / "src" / "pg_tests" / "helpers.rs"
    assert is_ignored_path(p)


def test_allow_inside_cfg_test_mod_is_skipped() -> None:
    src = (
        "fn prod() {}\n"
        "\n"
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    use super::*;\n"
        "    #[allow(clippy::let_unit_value)]\n"
        "    fn t() {}\n"
        "}\n"
    )
    assert _scan(src) == []


def test_allow_after_cfg_test_mod_is_flagged() -> None:
    src = (
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    fn a() {}\n"
        "}\n"
        "\n"
        "#[allow(clippy::let_unit_value)]\n"
        "fn prod() {}\n"
    )
    assert _scan(src) == [6]


def test_allow_inside_cfg_all_test_feature_mod_is_skipped() -> None:
    src = (
        "#[cfg(all(test, feature = \"foo\"))]\n"
        "mod tests {\n"
        "    #[allow(clippy::let_unit_value)]\n"
        "    fn t() {}\n"
        "}\n"
    )
    assert _scan(src) == []


def test_allow_inside_nested_braces_in_cfg_test_mod_is_skipped() -> None:
    src = (
        "#[cfg(test)]\n"
        "mod tests {\n"
        "    fn t() {\n"
        "        let x = || { 1 };\n"
        "        #[allow(clippy::let_unit_value)]\n"
        "        let _ = ();\n"
        "    }\n"
        "}\n"
    )
    assert _scan(src) == []


def test_allow_next_to_cfg_test_on_fn_is_skipped() -> None:
    src = (
        "#[cfg(test)]\n"
        "#[allow(clippy::too_many_arguments)]\n"
        "fn helper() {}\n"
    )
    assert _scan(src) == []


def test_allow_before_cfg_test_on_fn_is_skipped() -> None:
    src = (
        "#[allow(clippy::too_many_arguments)]\n"
        "#[cfg(test)]\n"
        "fn helper() {}\n"
    )
    assert _scan(src) == []


def test_allow_without_cfg_test_sibling_still_flagged() -> None:
    src = (
        "#[inline]\n"
        "#[allow(clippy::too_many_arguments)]\n"
        "fn helper() {}\n"
    )
    assert _scan(src) == [2]


def test_cfg_test_with_blank_line_before_mod_is_skipped() -> None:
    src = (
        "#[cfg(test)]\n"
        "\n"
        "mod tests {\n"
        "    #[allow(clippy::let_unit_value)]\n"
        "    fn t() {}\n"
        "}\n"
    )
    assert _scan(src) == []


def test_is_ignored_path_allows_production_file() -> None:
    assert not is_ignored_path(ROOT / "crates" / "foo" / "src" / "lib.rs")


def test_allow_inside_string_literal_still_matches() -> None:
    # Documented limitation: we do not strip strings. In practice a Rust
    # source file never contains the attribute pattern inside a string
    # outside test fixtures, and test fixture files are excluded.
    src = 'const S: &str = "#[allow(clippy::cast_possible_wrap)]";\n'
    assert _scan(src) == [1]


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
