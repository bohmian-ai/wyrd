"""Assemble hand-authored Wyrd stub sources into the native extension stub."""

from __future__ import annotations

import re
from pathlib import Path

STUB_DIR = Path("python/wyrd/stubs")
OUTPUT_FILE = Path("python/wyrd/_native.pyi")

STUB_FILES = [
    "header.pyi",
    "error.pyi",
    "data.pyi",
    "model.pyi",
]


def strip_imports_section(content: str) -> str:
    """Remove a module-local import block from a source stub."""
    pattern = r"####\s*begin\s+imports\s*####.*?####\s*end\s+of\s+imports\s*####\s*\n?"
    return re.sub(pattern, "", content, flags=re.DOTALL)


def assemble() -> None:
    """Write the assembled native extension stub."""
    final_content = [
        "# AUTO-GENERATED STUB FILE. DO NOT EDIT.",
        "# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value",
    ]
    master_all: list[str] = []
    all_pattern = re.compile(r"__all__\s*=\s*\[(.*?)\]", re.DOTALL)

    for filename in STUB_FILES:
        file_path = STUB_DIR / filename
        if not file_path.exists():
            continue

        raw_text = file_path.read_text(encoding="utf-8")
        if filename != "header.pyi":
            raw_text = strip_imports_section(raw_text)

        match = all_pattern.search(raw_text)
        if match:
            items = re.findall(r'"([^"]+)"|\'([^\']+)\'', match.group(1))
            master_all.extend(a or b for a, b in items)
            text_to_append = all_pattern.sub("", raw_text)
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
