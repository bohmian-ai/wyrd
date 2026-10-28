#!/usr/bin/env python3
"""Generate /llms.txt and /llms-full.txt for the Wyrd docs site.

Both files live in docs/public/ so the static site serves them under the
deploy base (GitHub Pages project subpath /wyrd/).

llms.txt       - compact index for agents: where to start, the home page, then
                 every sidebar page, grouped and ordered exactly as the sidebar
                 shows them. Derived from the content on disk, so it never
                 advertises a route the build does not serve.
llms-full.txt  - every sidebar page's readable body in the same order, with
                 embedded source resolved by scripts/page_text.mjs through the
                 site build's own <CodeFromFile> rules.

Links are root-relative under the deploy base, so the same files work from the
published site and from a locally served build.
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path


DOCS_ROOT = Path(__file__).resolve().parents[1]
CONTENT_DIR = DOCS_ROOT / "src" / "content" / "docs"
PUBLIC_DIR = DOCS_ROOT / "public"

# Production origin for the docs site: GitHub Pages serves it on the
# docs.bohmian.ai custom domain under BASE. BASE must always match
# svelte.config.js `paths.base`; every emitted link is BASE-relative so a local
# build under the same base serves it.
ORIGIN = "https://docs.bohmian.ai"
BASE = "/wyrd"
SITE = f"{ORIGIN}{BASE}"

# The onboarding route, in reading order: the canonical start for agents. Only
# the steps the sidebar publishes are listed, so a withdrawn step drops out.
START = [
    ("get-started/setup", "install Wyrd and start a local server you keep running"),
    ("get-started/quickstart", "declare a Verifier in YAML and verify a passing and a failing support answer from Python, Rust, or TypeScript"),
]

SUMMARY = (
    "Wyrd is open-source AI assurance and verification infrastructure. Verify AI "
    "systems before and after release, with verdicts you can trace, reproduce, "
    "and audit."
)

# Mirrors SECTIONS and TOPICS in docs/src/lib/content.ts: the sidebar order.
SECTIONS = [
    "Get started",
    "Build and verify",
    "Connect",
    "Operate",
    "Understand",
    "Reference",
]
TOPICS = ["Declare", "Observe", "Verify"]

Page = tuple[str, str, str, str, Path]  # (section, slug, title, description, file)


def slug_for(path: Path) -> str:
    """Mirror docs/src/lib/content.ts toSlug() so URLs match served routes."""
    rel = path.relative_to(CONTENT_DIR).as_posix()
    rel = rel.rsplit(".", 1)[0]
    if rel == "index":
        return ""
    if rel.endswith("/index"):
        return rel[: -len("/index")]
    return rel


def read_frontmatter(path: Path) -> dict[str, str]:
    """Return the top-level `key: value` pairs of a leading `---` block.

    A minimal line parser keeps the generator free of a YAML dependency;
    content.ts validates the same frontmatter strictly at build time.
    """
    text = path.read_text(encoding="utf-8")
    fields: dict[str, str] = {}
    if text.startswith("---"):
        end = text.find("\n---", 3)
        for line in (text[3:end] if end != -1 else "").splitlines():
            key, sep, value = line.partition(":")
            if sep and key and not key[0].isspace():
                fields[key.strip()] = _unquote(value.strip())
    return fields


def _unquote(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {'"', "'"}:
        return value[1:-1]
    return value


def _order(value: str | None) -> float:
    try:
        return float(value) if value is not None else float("inf")
    except ValueError:
        return float("inf")


def in_sidebar(fm: dict[str, str]) -> bool:
    """Mirror derived-nav.ts inPrimaryNav(): no drafts or Fathom pages."""
    return fm.get("draft") != "true" and fm.get("pillar") != "fathom"


def sidebar_pages() -> list[Page]:
    """Every sidebar page, ordered by section, topic, `order`, then slug."""
    ranked = []
    for path in CONTENT_DIR.rglob("*") if CONTENT_DIR.exists() else []:
        if path.suffix not in {".svx", ".md"}:
            continue
        fm = read_frontmatter(path)
        if not in_sidebar(fm) or fm.get("section") not in SECTIONS:
            continue
        section, topic, slug = fm["section"], fm.get("topic"), slug_for(path)
        key = (
            SECTIONS.index(section),
            TOPICS.index(topic) if topic in TOPICS else 0,
            _order(fm.get("order")),
            slug,
        )
        ranked.append(
            (key, (section, slug, fm.get("title", slug), fm.get("description", ""), path))
        )
    return [page for _key, page in sorted(ranked)]


def url_for(slug: str) -> str:
    return f"{BASE}/" if not slug else f"{BASE}/{slug}/"


def render_start() -> list[str]:
    """The canonical start route, for an agent that reads only the top.

    Empty when the sidebar publishes none of its steps.
    """
    titles = {slug: title for _section, slug, title, _description, _path in sidebar_pages()}
    steps = [f"- [{titles[slug]}]({url_for(slug)}): {purpose}." for slug, purpose in START if slug in titles]
    if not steps:
        return []
    return [
        "## Start here",
        "",
        "Set up a local server, then run one verification, in this order. "
        "A connected Wyrd MCP server teaches the same route through "
        '`wyrd.guide {"topic":"start"}`.',
        "",
        *steps,
        "",
    ]


def render_pages_index() -> list[str]:
    lines = [
        "## Home",
        "",
        f"- [Wyrd docs]({url_for('')}): the docs home, with a link to each published section.",
        "",
    ]
    pages = sidebar_pages()
    for section in SECTIONS:
        entries = [page for page in pages if page[0] == section]
        if not entries:
            continue
        lines += [f"## {section}", ""]
        for _section, slug, title, description, _path in entries:
            suffix = f": {description}" if description else ""
            lines.append(f"- [{title}]({url_for(slug)}){suffix}")
        lines.append("")
    return lines


def render_llms_txt() -> str:
    lines = ["# Wyrd", "", f"> {SUMMARY}", ""]
    lines += render_start()
    lines += render_pages_index()
    lines += [
        "## Machine-readable reference",
        "",
        f"- [llms-full.txt]({url_for('')}llms-full.txt): every page below in full, with its embedded YAML and code.",
        "",
    ]
    return "\n".join(lines)


def page_bodies(pages: list[Page]) -> list[str]:
    """Each page's plain-Markdown body, rendered by the site's own embed rules."""
    rendered = subprocess.run(
        ["node", "scripts/page_text.mjs", *(str(page[4]) for page in pages)],
        cwd=DOCS_ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(rendered.stdout)


def render_llms_full() -> str:
    pages = sidebar_pages()
    lines = [
        "# Wyrd — full machine-readable reference",
        "",
        f"> {SUMMARY}",
        "",
        f"Every published docs page in sidebar order, generated from the docs content "
        f"tree. Links are relative to the docs base {BASE}/ (published at {SITE}/). "
        f"The page index is [llms.txt]({url_for('')}llms.txt).",
        "",
    ]
    lines += render_start()
    for (_section, slug, title, description, _path), body in zip(pages, page_bodies(pages)):
        lines += [f"# {title}", "", f"Page: {url_for(slug)}", ""]
        if description:
            lines += [f"> {description}", ""]
        lines += [body, ""]
    return "\n".join(lines)


def main() -> None:
    PUBLIC_DIR.mkdir(parents=True, exist_ok=True)
    (PUBLIC_DIR / "llms.txt").write_text(render_llms_txt(), encoding="utf-8")
    (PUBLIC_DIR / "llms-full.txt").write_text(render_llms_full(), encoding="utf-8")


if __name__ == "__main__":
    main()
