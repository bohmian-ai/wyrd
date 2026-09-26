"""Self-check for verify_review_citations.check; run with python3."""

from verify_review_citations import check

SOURCE = "fn pack(budget: u32) {\n    let used = 0;\n    if used > budget {\n    }\n}\n"


def entry(**overrides):
    """Builds a citation that holds against SOURCE unless overridden."""
    base = {"id": "F1", "path": "a.rs", "line": 2, "snippet": "let used = 0;\n  if used > budget {", "symbols": ["pack"]}
    return {**base, **overrides}


def test_matching_citation_passes():
    assert check(entry(), SOURCE) is None


def test_missing_file_rejects():
    assert "does not exist" in check(entry(), None)


def test_wrong_line_rejects():
    assert "snippet not found" in check(entry(line=1), SOURCE)


def test_invented_snippet_rejects():
    assert "snippet not found" in check(entry(snippet="let used = 1;"), SOURCE)


def test_invented_symbol_rejects():
    assert "symbol" in check(entry(symbols=["pack_all"]), SOURCE)


if __name__ == "__main__":
    for name, test in list(globals().items()):
        if name.startswith("test_"):
            test()
    print("ok")
