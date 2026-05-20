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


def markdown_pages() -> list[Path]:
    return sorted(path for path in CONTENT_ROOT.rglob("*") if path.suffix in {".md", ".mdx"})


def route_to_file(route: str) -> Path:
    route = route.strip("/")
    if not route:
        return CONTENT_ROOT / "index.mdx"
    return CONTENT_ROOT / route / "index.mdx"


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
        route_file = route_to_file(path)
        alt_md = route_file.with_suffix(".md")
        return route_file.exists() or alt_md.exists()

    candidate = (source.parent / path).resolve()
    if candidate.exists():
        return True
    if candidate.suffix == "":
        return (candidate / "index.mdx").exists() or (candidate / "index.md").exists()
    return False


def main() -> int:
    failures: list[str] = []
    for page in markdown_pages():
        text = page.read_text(encoding="utf-8")
        for regex in (LINK, IMAGE):
            for match in regex.finditer(text):
                target = match.group(1)
                if not local_target_exists(page, target):
                    rel = page.relative_to(DOCS_ROOT)
                    failures.append(f"{rel}: missing local target `{target}`")

    if failures:
        print("Docs link check failed:")
        print("\n".join(failures))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
