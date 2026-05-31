"""Assemble hand-authored Wyrd stub sources into the native extension stub."""

from __future__ import annotations

import re
from pathlib import Path

STUB_DIR = Path("python/wyrd/stubs")
OUTPUT_FILE = Path("python/wyrd/_native.pyi")
PUBLIC_MODEL_FILE = Path("python/wyrd/model.pyi")
PUBLIC_PROMPT_FILE = Path("python/wyrd/prompt.pyi")
PUBLIC_INIT_FILE = Path("python/wyrd/__init__.pyi")

STUB_FILES = [
    "header.pyi",
    "error.pyi",
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


def extract_all(raw_text: str) -> list[str]:
    """Return the literal names from a stub-level __all__ block."""
    all_pattern = r"__all__\s*=\s*\[(.*?)\]"
    match = re.search(all_pattern, raw_text, flags=re.DOTALL)
    if not match:
        return []
    items = re.findall(r'"([^"]+)"|\'([^\']+)\'', match.group(1))
    return [a or b for a, b in items]


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
    write_public_model_stub()
    write_public_prompt_stub()
    write_public_init_stub()
    print(f"Compiled {len(master_all)} exports into {OUTPUT_FILE}")


def write_public_model_stub() -> None:
    """Write the public wyrd.model re-export stub."""
    exports = extract_all((STUB_DIR / "model.pyi").read_text(encoding="utf-8"))
    lines = [
        "from typing import TYPE_CHECKING",
        "",
        "if TYPE_CHECKING:",
        "    from ._native import (",
    ]
    lines.extend(f"        {name}," for name in exports)
    lines.extend(
        [
            "    )",
            "else:",
            "    from ._native.cards.model import (",
        ]
    )
    lines.extend(f"        {name}," for name in exports)
    lines.extend(["    )", "", "__all__ = ["])
    lines.extend(f'    "{name}",' for name in exports)
    lines.append("]")
    PUBLIC_MODEL_FILE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_public_prompt_stub() -> None:
    """Write the public wyrd.prompt re-export stub."""
    exports = extract_all((STUB_DIR / "prompt.pyi").read_text(encoding="utf-8"))
    lines = [
        "from typing import TYPE_CHECKING",
        "",
        "if TYPE_CHECKING:",
        "    from ._native import (",
    ]
    lines.extend(f"        {name}," for name in exports)
    lines.extend(
        [
            "    )",
            "else:",
            "    from ._native.cards.prompt import (",
        ]
    )
    lines.extend(f"        {name}," for name in exports)
    lines.extend(["    )", "", "__all__ = ["])
    lines.extend(f'    "{name}",' for name in exports)
    lines.append("]")
    PUBLIC_PROMPT_FILE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_public_init_stub() -> None:
    """Write the package root re-export stub."""
    lines = [
        "from . import data as data",
        "from . import model as model",
        "from . import prompt as prompt",
        "from .data import DataCard as DataCard",
        "from .data import Split as Split",
        "from .data import WyrdError as WyrdError",
        "from .model import ModelCard as ModelCard",
        "from .model import ModelSignature as ModelSignature",
        "from .model import SampleInput as SampleInput",
        "from .prompt import AnthropicSettings as AnthropicSettings",
        "from .prompt import GeminiSettings as GeminiSettings",
        "from .prompt import MediaRef as MediaRef",
        "from .prompt import OpenAIResponsesSettings as OpenAIResponsesSettings",
        "from .prompt import OpenAISettings as OpenAISettings",
        "from .prompt import Prompt as Prompt",
        "from .prompt import PromptCard as PromptCard",
        "from .prompt import PromptCardMetadata as PromptCardMetadata",
        "from .prompt import PromptRef as PromptRef",
        "from .prompt import ProviderRequest as ProviderRequest",
        "from .prompt import ResponseFormat as ResponseFormat",
        "",
        "__all__ = [",
        '    "AnthropicSettings",',
        '    "DataCard",',
        '    "GeminiSettings",',
        '    "MediaRef",',
        '    "ModelCard",',
        '    "ModelSignature",',
        '    "OpenAIResponsesSettings",',
        '    "OpenAISettings",',
        '    "Prompt",',
        '    "PromptCard",',
        '    "PromptCardMetadata",',
        '    "PromptRef",',
        '    "ProviderRequest",',
        '    "ResponseFormat",',
        '    "SampleInput",',
        '    "Split",',
        '    "WyrdError",',
        '    "data",',
        '    "model",',
        '    "prompt",',
        "]",
    ]
    PUBLIC_INIT_FILE.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    assemble()
