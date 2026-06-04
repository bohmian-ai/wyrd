from __future__ import annotations

import ast
from pathlib import Path

import wyrd
import wyrd.data as data_module

PACKAGE_ROOT = Path(__file__).resolve().parents[3] / "python" / "wyrd"


def _init_py_all_exports() -> set[str]:
    tree = ast.parse((PACKAGE_ROOT / "__init__.py").read_text(encoding="utf-8"))
    for node in tree.body:
        if isinstance(node, ast.Assign):
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == "__all__":
                    return set(ast.literal_eval(node.value))
    raise AssertionError("wyrd.__init__ must define __all__")


def test_wyrd_data_all_exports_are_importable() -> None:
    for name in data_module.__all__:
        assert getattr(data_module, name) is not None


def test_top_level_exports_match_init_all() -> None:
    init_exports = _init_py_all_exports()

    assert set(wyrd.__all__) == init_exports
    for name in wyrd.__all__:
        assert getattr(wyrd, name) is not None


def test_data_stub_public_classes_and_methods_have_docstrings() -> None:
    tree = ast.parse((PACKAGE_ROOT / "stubs" / "data.pyi").read_text(encoding="utf-8"))
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


def test_opsml_style_python_package_layout() -> None:
    assert (PACKAGE_ROOT / "__init__.py").is_file()
    assert (PACKAGE_ROOT / "__init__.pyi").is_file()
    assert (PACKAGE_ROOT / "_wyrd.pyi").is_file()
    assert {path.name for path in PACKAGE_ROOT.glob("*.pyi")} == {
        "__init__.pyi",
        "_wyrd.pyi",
    }
    for name in ("agent", "data", "model", "prompt"):
        assert (PACKAGE_ROOT / name / "__init__.py").is_file()
        assert (PACKAGE_ROOT / name / "__init__.pyi").is_file()
        assert not (PACKAGE_ROOT / f"{name}.py").exists()
        assert not (PACKAGE_ROOT / f"{name}.pyi").exists()
