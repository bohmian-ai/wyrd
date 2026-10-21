#!/usr/bin/env python3
"""Validate local Markdown links in docs pages.

Every link must name an existing page or served file, and a published page may
link only to published pages: a draft is absent from the production build.
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlparse

from generate_llms_txt import BASE, read_frontmatter


DOCS_ROOT = Path(__file__).resolve().parents[1]
CONTENT_ROOT = DOCS_ROOT / "src" / "content" / "docs"
PUBLIC_ROOT = DOCS_ROOT / "public"
LINK = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
IMAGE = re.compile(r"!\[[^\]]*\]\(([^)]+)\)")


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


def target_page(source: Path, raw_target: str) -> Path | None:
    """The content page a local link names, resolved as the site build routes it.

    Root-relative links may carry the deploy base, which the build leaves as is.
    """
    parsed = urlparse(raw_target.strip().split()[0])
    path = unquote(parsed.path)
    if parsed.scheme or not path:
        return None
    if path.startswith("/"):
        route = path.strip("/")
        base = BASE.strip("/")
        if route == base or route.startswith(f"{base}/"):
            route = route[len(base):].strip("/")
        candidates = route_candidates(route)
    else:
        candidate = (source.parent / path).resolve()
        candidates = (
            [candidate]
            if candidate.suffix
            else [candidate / f"index{suffix}" for suffix in PAGE_SUFFIXES]
            + [candidate.with_suffix(suffix) for suffix in PAGE_SUFFIXES]
        )
    return next((c for c in candidates if c.suffix in PAGE_SUFFIXES and c.exists()), None)


def local_target_exists(source: Path, raw_target: str) -> bool:
    parsed = urlparse(raw_target.strip().split()[0])
    path = unquote(parsed.path)
    if parsed.scheme or not path:
        return True
    if path.startswith("/"):
        if (PUBLIC_ROOT / path.lstrip("/")).exists():
            return True
    elif (source.parent / path).resolve().exists():
        return True
    return target_page(source, raw_target) is not None


def is_draft(page: Path) -> bool:
    """Drafts never reach the production build, so they are not link destinations."""
    return read_frontmatter(page).get("draft") == "true"


def main() -> int:
    failures: list[str] = []
    for page in markdown_pages():
        text = page.read_text(encoding="utf-8")
        published = not is_draft(page)
        for regex in (LINK, IMAGE):
            for match in regex.finditer(text):
                target = match.group(1)
                if not local_target_exists(page, target):
                    problem = "missing local target"
                elif published and (dest := target_page(page, target)) and is_draft(dest):
                    problem = "link to unpublished page"
                else:
                    continue
                line_num = text[:match.start()].count("\n") + 1
                rel = os.path.relpath(page, DOCS_ROOT)
                failures.append(f"{rel}:{line_num}: {problem} `{target}`")

    if failures:
        print("Docs link check failed:")
        print("\n".join(failures))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
