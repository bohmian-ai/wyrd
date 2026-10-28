#!/usr/bin/env python3
"""Reject public docs snippets that bypass the repository task runner."""

from __future__ import annotations

import re
import sys
from pathlib import Path


DOCS_ROOT = Path(__file__).resolve().parents[1]
SCAN_ROOTS = [DOCS_ROOT / "src" / "content" / "docs", DOCS_ROOT / "CONTRIBUTING.md"]
# Cargo, uv, and pnpm are forbidden only where they drive this repository: a
# reader's own project (`cargo run`, `uv run quickstart.py`, `pnpm add`) has
# no mise.
FORBIDDEN = re.compile(
    r"^\s*(?:[$>]?\s*)?("
    r"cargo\s+(?:test|nextest|clippy|fmt|check|build|bench)\b"
    r"|cargo\b.*\s(?:-p|--package|--workspace|--manifest-path)\b"
    r"|uv\s+run\s+(?:--(?:project|directory|package)\b|(?:python\s+)?scripts/)"
    r"|pnpm\s+(?:test|check|build|dev|preview|install|exec\s+vitest)\b"
    r"|pnpm\b.*\s(?:-C|--dir|--filter)\b)"
)
FENCE = re.compile(r"^\s*```(?P<info>.*)$")
ALLOWED_HINTS = {"internal", "contributor", "maintainer"}


def paths() -> list[Path]:
    result: list[Path] = []
    for root in SCAN_ROOTS:
        if root.is_file():
            result.append(root)
        elif root.exists():
            result.extend(sorted(path for path in root.rglob("*") if path.suffix in {".svx", ".md", ".mdx"}))
    return result


def is_allowed(info: str, previous_lines: list[str]) -> bool:
    lowered = info.lower()
    if any(hint in lowered for hint in ALLOWED_HINTS):
        return True
    context = "\n".join(previous_lines[-4:]).lower()
    return any(hint in context for hint in ALLOWED_HINTS)


def main() -> int:
    failures: list[str] = []
    for path in paths():
        in_fence = False
        fence_info = ""
        previous: list[str] = []
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            match = FENCE.match(line)
            if match:
                if not in_fence:
                    in_fence = True
                    fence_info = match.group("info") or ""
                else:
                    in_fence = False
                    fence_info = ""
                previous.append(line)
                continue
            if in_fence and FORBIDDEN.match(line) and not is_allowed(fence_info, previous):
                rel = path.relative_to(DOCS_ROOT)
                failures.append(f"{rel}:{lineno}: use a mise task instead of `{line.strip()}`")
            previous.append(line)

    if failures:
        print("Public docs command check failed:")
        print("\n".join(failures))
        return 1
    return 0


def self_check() -> None:
    """Pin which Cargo, uv, and pnpm commands count as bypassing the task runner."""
    for line in ["cargo test", "cargo nextest run --locked", "cargo run -p wyrd-server", "cargo build --workspace",
                 "uv run python scripts/check_links.py", "uv run scripts/x.py", "uv run --project sdks/wyrd-sdk-python pytest",
                 "pnpm test", "pnpm check", "pnpm install --frozen-lockfile", "pnpm -C docs build"]:
        assert FORBIDDEN.match(line), line
    for line in ["cargo new quickstart && cd quickstart", "cargo add wyrd-ai@0.4.0 serde_json", "cargo run", "cargo install wyrd-cli --version 0.4.0 --locked",
                 "uv run quickstart.py", "uv run wyrd init",
                 "pnpm init && pnpm pkg set type=module", "pnpm add wyrd-ai@0.4.0", "pnpm wyrd init"]:
        assert not FORBIDDEN.match(line), line


if __name__ == "__main__":
    self_check()
    sys.exit(main())
