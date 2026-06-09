"""Assemble hand-authored Wyrd stub sources into public package stubs."""

from __future__ import annotations

import re
from pathlib import Path

STUB_DIR = Path("python/wyrd/stubs")
PACKAGE_DIR = Path("python/wyrd")
ROOT_OUTPUT_FILE = PACKAGE_DIR / "_wyrd.pyi"

PUBLIC_MODULE_STUBS = {
    "agent.pyi": PACKAGE_DIR / "agent" / "__init__.pyi",
    "cards.pyi": PACKAGE_DIR / "cards" / "__init__.pyi",
    "data.pyi": PACKAGE_DIR / "data" / "__init__.pyi",
    "model.pyi": PACKAGE_DIR / "model" / "__init__.pyi",
    "prompt.pyi": PACKAGE_DIR / "prompt" / "__init__.pyi",
    "session.pyi": PACKAGE_DIR / "session" / "__init__.pyi",
}

ROOT_STUB_FILES = ["header.pyi", "error.pyi"]
PACKAGE_STUB_FILE = "package.pyi"


def validate_source_stub(filename: str, raw_text: str) -> None:
    """Validate hand-authored stub conventions before assembly."""
    if re.search(r"^\s*def\s+__new__\s*\(", raw_text, flags=re.MULTILINE):
        raise SystemExit(
            f"{STUB_DIR / filename}: public stubs must document constructors as "
            "__init__, not __new__"
        )


def strip_imports_section(content: str) -> str:
    """Remove a module-local import block from a source stub."""
    pattern = r"####\s*begin\s+imports\s*####.*?####\s*end\s+of\s+imports\s*####\s*\n?"
    return re.sub(pattern, "", content, flags=re.DOTALL)


def remove_all_block(content: str) -> tuple[str, list[str]]:
    """Remove and return a source stub `__all__` block."""
    all_pattern = r"__all__\s*=\s*\[(.*?)\]"
    match = re.search(all_pattern, content, flags=re.DOTALL)
    if not match:
        return content, []
    items = re.findall(r'"([^"]+)"|\'([^\']+)\'', match.group(1))
    return re.sub(all_pattern, "", content, flags=re.DOTALL), [a or b for a, b in items]


def validate_no_duplicate_classes(path: Path, content: str) -> None:
    """Fail when one generated stub defines the same class name twice."""
    seen: dict[str, int] = {}
    duplicates: list[str] = []
    for line_no, line in enumerate(content.splitlines(), start=1):
        match = re.match(r"\s*class\s+([A-Za-z_][A-Za-z0-9_]*)\b", line)
        if not match:
            continue
        name = match.group(1)
        if name in seen:
            duplicates.append(f"{name} at lines {seen[name]} and {line_no}")
        else:
            seen[name] = line_no
    if duplicates:
        joined = "; ".join(duplicates)
        raise SystemExit(f"{path}: duplicate class declarations: {joined}")


def write_stub(path: Path, lines: list[str]) -> None:
    """Write a generated stub after duplicate-class validation."""
    content = "\n".join(lines).strip() + "\n"
    validate_no_duplicate_classes(path, content)
    path.write_text(content, encoding="utf-8")


def source_text(filename: str) -> str:
    """Read and validate one hand-authored source stub."""
    path = STUB_DIR / filename
    raw_text = path.read_text(encoding="utf-8")
    validate_source_stub(filename, raw_text)
    return raw_text


def rewrite_public_imports(filename: str, content: str) -> str:
    """Rewrite source-stub imports for generated package-local stubs."""
    replacements = {
        "agent.pyi": {
            "from collections.abc import Callable, Mapping, Sequence": (
                "from collections.abc import Callable, Mapping, Sequence"
            ),
            "from .error import WyrdError\nfrom .header import JsonDict, PathLike": (
                "from .._wyrd import JsonDict, PathLike, WyrdError"
            ),
            "from .prompt import Prompt": "from ..prompt import Prompt",
        },
        "data.pyi": {
            "from .cards import CardRef": "from ..cards import CardRef",
            "from .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError"
            ),
        },
        "model.pyi": {
            "from .cards import CardRef": "from ..cards import CardRef",
            "from .data import FieldSpec\nfrom .error import WyrdError\nfrom .header import CardRefLike, JsonDict, PathLike, StringMap": (
                "from .._wyrd import CardRefLike, JsonDict, PathLike, StringMap, WyrdError\n"
                "from ..data import FieldSpec"
            ),
        },
        "prompt.pyi": {
            "from .cards import CardRef": "from ..cards import CardRef",
            "from .error import WyrdError\nfrom .header import JsonDict, PathLike": (
                "from .._wyrd import JsonDict, PathLike, WyrdError"
            ),
        },
        "session.pyi": {
            "from .header import JsonDict": "from .._wyrd import JsonDict",
        },
    }
    for before, after in replacements.get(filename, {}).items():
        content = content.replace(before, after)
    return content


def assemble_root_stub() -> None:
    """Write the root native extension stub."""
    final_content = [
        "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
        "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
    ]
    master_all: list[str] = []

    for filename in ROOT_STUB_FILES:
        raw_text = source_text(filename)
        if filename != "header.pyi":
            raw_text = strip_imports_section(raw_text)
        text_to_append, all_items = remove_all_block(raw_text)
        master_all.extend(all_items)

        final_content.append(f"### {filename} ###")
        final_content.append(text_to_append.strip())
        final_content.append("")

    final_content.append("def _init() -> None:")
    final_content.append('    """Initialize the native Wyrd extension."""')
    final_content.append("    ...")
    final_content.append("")
    master_all.append("_init")

    final_content.append("### GLOBAL EXPORTS ###")
    final_content.append("__all__ = [")
    for item in sorted(set(master_all)):
        final_content.append(f'    "{item}",')
    final_content.append("]")

    write_stub(ROOT_OUTPUT_FILE, final_content)
    print(f"Compiled {len(master_all)} root exports into {ROOT_OUTPUT_FILE}")


def assemble_public_module_stubs() -> None:
    """Write package-local stubs for public Wyrd modules."""
    for filename, output_path in PUBLIC_MODULE_STUBS.items():
        raw_text = rewrite_public_imports(filename, source_text(filename))
        lines = [
            "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
            "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
            raw_text.strip(),
        ]
        write_stub(output_path, lines)
        print(f"Compiled {filename} into {output_path}")


def assemble_package_stub() -> None:
    """Write the root public package stub."""
    raw_text = source_text(PACKAGE_STUB_FILE)
    lines = [
        "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
        "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
        raw_text.strip(),
    ]
    output_path = PACKAGE_DIR / "__init__.pyi"
    write_stub(output_path, lines)
    print(f"Compiled {PACKAGE_STUB_FILE} into {output_path}")


def assemble() -> None:
    """Write all generated public stubs."""
    assemble_root_stub()
    assemble_package_stub()
    assemble_public_module_stubs()


if __name__ == "__main__":
    assemble()
