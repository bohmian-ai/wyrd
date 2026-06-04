"""Assemble hand-authored Wyrd stub sources into the native extension stub."""

from __future__ import annotations

import re
from pathlib import Path

STUB_DIR = Path("python/wyrd/stubs")
OUTPUT_FILE = Path("python/wyrd/_wyrd.pyi")

STUB_FILES = [
    "header.pyi",
    "error.pyi",
    "agent.pyi",
    "data.pyi",
    "model.pyi",
    "prompt.pyi",
]


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


def assemble() -> None:
    """Write the assembled native extension stub."""
    final_content = [
        "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
        "# ruff: noqa: F811",
        "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
    ]
    master_all: list[str] = []
    all_pattern = r"__all__\s*=\s*\[(.*?)\]"

    for filename in STUB_FILES:
        file_path = STUB_DIR / filename
        if not file_path.exists():
            continue

        raw_text = file_path.read_text(encoding="utf-8")
        validate_source_stub(filename, raw_text)
        if filename != "header.pyi":
            raw_text = strip_imports_section(raw_text)

        match = re.search(all_pattern, raw_text, flags=re.DOTALL)
        if match:
            items = re.findall(r'"([^"]+)"|\'([^\']+)\'', match.group(1))
            master_all.extend(a or b for a, b in items)
            text_to_append = re.sub(all_pattern, "", raw_text, flags=re.DOTALL)
        else:
            text_to_append = raw_text

        final_content.append(f"### {filename} ###")
        final_content.append(text_to_append.strip())
        final_content.append("")

    final_content.append("### GLOBAL EXPORTS ###")
    final_content.append("__all__ = [")
    for item in sorted(set(master_all)):
        final_content.append(f'    "{item}",')
    final_content.append("]")

    OUTPUT_FILE.write_text("\n".join(final_content) + "\n", encoding="utf-8")
    print(f"Compiled {len(master_all)} exports into {OUTPUT_FILE}")


if __name__ == "__main__":
    assemble()
