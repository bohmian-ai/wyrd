from __future__ import annotations

import ast
from pathlib import Path

import wyrd
import wyrd.data as data_module

PACKAGE_ROOT = Path(__file__).resolve().parents[3] / "python" / "wyrd"


def _init_pyi_exports() -> set[str]:
    tree = ast.parse((PACKAGE_ROOT / "__init__.pyi").read_text(encoding="utf-8"))
    names: set[str] = set()
    for node in tree.body:
        if isinstance(node, ast.ImportFrom):
            for alias in node.names:
                names.add(alias.asname or alias.name)
    return names


def test_wyrd_data_all_exports_are_importable() -> None:
    for name in data_module.__all__:
        assert getattr(data_module, name) is not None


def test_top_level_exports_match_generated_init_stub() -> None:
    pyi_exports = _init_pyi_exports()

    assert set(wyrd.__all__) == pyi_exports
    for name in wyrd.__all__:
        assert getattr(wyrd, name) is not None


def test_data_stub_public_classes_and_methods_have_docstrings() -> None:
    tree = ast.parse((PACKAGE_ROOT / "data.pyi").read_text(encoding="utf-8"))
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
