from __future__ import annotations

import ast
from pathlib import Path

import wyrd
import wyrd.model as model_module

PACKAGE_ROOT = Path(__file__).resolve().parents[3] / "python" / "wyrd"


def _init_pyi_exports() -> set[str]:
    tree = ast.parse((PACKAGE_ROOT / "__init__.pyi").read_text(encoding="utf-8"))
    names: set[str] = set()
    for node in tree.body:
        if isinstance(node, ast.ImportFrom):
            for alias in node.names:
                names.add(alias.asname or alias.name)
    return names


def _args_doc_lines(docstring: str) -> list[str]:
    lines = docstring.splitlines()
    if "Args:" not in lines:
        return []
    start = lines.index("Args:") + 1
    return [
        line.strip()
        for line in lines[start:]
        if line.startswith("    ")
        and not line.startswith("        ")
        and line.strip()
        and ":" in line
    ]


def test_wyrd_model_all_exports_are_importable() -> None:
    for name in model_module.__all__:
        assert getattr(model_module, name) is not None


def test_top_level_model_exports_match_generated_init_stub() -> None:
    pyi_exports = _init_pyi_exports()

    assert "model" in wyrd.__all__
    assert "ModelCard" in wyrd.__all__
    assert "ModelSignature" in wyrd.__all__
    assert "SampleInput" in wyrd.__all__
    assert set(wyrd.__all__) == pyi_exports


def test_model_stub_public_classes_and_methods_have_docstrings() -> None:
    tree = ast.parse((PACKAGE_ROOT / "model.pyi").read_text(encoding="utf-8"))
    missing: list[str] = []

    for node in tree.body:
        if not isinstance(node, ast.ClassDef) or node.name.startswith("_"):
            continue
        if ast.get_docstring(node) is None:
            missing.append(node.name)
        for child in node.body:
            if not isinstance(child, ast.FunctionDef):
                continue
            if child.name != "__init__" and child.name.startswith("_"):
                continue
            if ast.get_docstring(child) is None:
                missing.append(f"{node.name}.{child.name}")

    assert missing == []


def test_model_stub_args_docs_use_name_type_description_format() -> None:
    tree = ast.parse((PACKAGE_ROOT / "model.pyi").read_text(encoding="utf-8"))
    bad_lines: list[str] = []

    for node in ast.walk(tree):
        if not isinstance(node, ast.FunctionDef):
            continue
        docstring = ast.get_docstring(node)
        if docstring is None:
            continue
        for line in _args_doc_lines(docstring):
            before_colon = line.split(":", 1)[0]
            if "(" not in before_colon or ")" not in before_colon:
                bad_lines.append(f"{node.name}: {line}")

    assert bad_lines == []
