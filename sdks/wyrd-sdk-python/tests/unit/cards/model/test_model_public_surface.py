from __future__ import annotations

import ast
from pathlib import Path

import wyrd
import wyrd.model as model_module

PACKAGE_ROOT = Path(__file__).resolve().parents[4] / "python" / "wyrd"


def _init_py_all_exports() -> set[str]:
    tree = ast.parse((PACKAGE_ROOT / "__init__.py").read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == "__all__":
                    return set(ast.literal_eval(node.value))
    raise AssertionError("wyrd.__init__ must define __all__")


def test_wyrd_model_all_exports_are_importable() -> None:
    for name in model_module.__all__:
        assert getattr(model_module, name) is not None


def test_top_level_model_exports_match_init_all() -> None:
    init_exports = _init_py_all_exports()

    assert "model" in wyrd.__all__
    assert "ModelCard" in wyrd.__all__
    assert "ModelSignature" in wyrd.__all__
    assert "SampleInput" in wyrd.__all__
    assert set(wyrd.__all__) == init_exports


def test_model_stub_public_classes_and_methods_have_docstrings() -> None:
    tree = ast.parse((PACKAGE_ROOT / "stubs" / "model.pyi").read_text(encoding="utf-8"))
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
