#!/usr/bin/env python3
"""Validate local Markdown links in docs pages."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse


DOCS_ROOT = Path(__file__).resolve().parents[1]
CONTENT_ROOT = DOCS_ROOT / "src" / "content" / "docs"
PUBLIC_ROOT = DOCS_ROOT / "public"
LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
IMAGE = re.compile(r"!\[[^\]]*\]\(([^)]+)\)")
EXTERNAL_SCHEMES = {"http", "https", "mailto", "tel"}


PAGE_SUFFIXES = {".svx", ".md", ".mdx"}


def markdown_pages() -> list[Path]:
    return sorted(path for path in CONTENT_ROOT.rglob("*") if path.suffix in PAGE_SUFFIXES)


def route_candidates(route: str) -> list[Path]:
    route = route.strip("/")
    if not route:
        return [CONTENT_ROOT / "index.svx", CONTENT_ROOT / "index.md", CONTENT_ROOT / "index.mdx"]
    return [
        CONTENT_ROOT / route / "index.svx",
        CONTENT_ROOT / route / "index.md",
        CONTENT_ROOT / route / "index.mdx",
        CONTENT_ROOT / (route + ".svx"),
        CONTENT_ROOT / (route + ".md"),
        CONTENT_ROOT / (route + ".mdx"),
    ]


def route_to_file(route: str) -> Path | None:
    candidates = route_candidates(route)
    for candidate in candidates:
        if candidate.exists():
            return candidate
    return candidates[0]  # default for existence check


def local_target_exists(source: Path, raw_target: str) -> bool:
    target = raw_target.strip().split()[0]
    parsed = urlparse(target)
    if parsed.scheme in EXTERNAL_SCHEMES:
        return True
    if parsed.scheme:
        return True

    path = unquote(parsed.path)
    if not path:
        return True

    if path.startswith("/"):
        public_file = PUBLIC_ROOT / path.lstrip("/")
        if public_file.exists():
            return True
        return any(c.exists() for c in route_candidates(path.strip("/")))

    candidate = (source.parent / path).resolve()
    if candidate.exists():
        return True
    if candidate.suffix == "":
        return (
            (candidate / "index.svx").exists()
            or (candidate / "index.md").exists()
            or (candidate / "index.mdx").exists()
            or candidate.with_suffix(".svx").exists()
            or candidate.with_suffix(".md").exists()
            or candidate.with_suffix(".mdx").exists()
        )
    return False


def main() -> int:
    failures: list[str] = []
    for page in markdown_pages():
        text = page.read_text(encoding="utf-8")
        for regex in (LINK, IMAGE):
            for match in regex.finditer(text):
                target = match.group(1)
                if not local_target_exists(page, target):
                    line_num = text[:match.start()].count("\n") + 1
                    rel = page.relative_to(DOCS_ROOT)
                    failures.append(f"{rel}:{line_num}: missing local target `{target}`")

    if failures:
        print("Docs link check failed:")
        print("\n".join(failures))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
