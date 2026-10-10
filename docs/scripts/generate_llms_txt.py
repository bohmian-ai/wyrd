#!/usr/bin/env python3
"""Generate /llms.txt and /llms-full.txt for the Wyrd docs site.

Both files live in docs/public/ so the static site serves them under the
deploy base (GitHub Pages project subpath /wyrd/).

llms.txt       - compact index for agents: the home page, then every sidebar
                 page, grouped and ordered exactly as the sidebar shows them.
                 Derived from the content on disk, so it never advertises a
                 route the build does not serve.
llms-full.txt  - the same index plus the JSON Schema of every Card kind.
"""

from __future__ import annotations

import json
from pathlib import Path


DOCS_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = DOCS_ROOT.parent
SCHEMA_DIR = REPO_ROOT / "crates" / "wyrd-spec" / "schemas"
CONTENT_DIR = DOCS_ROOT / "src" / "content" / "docs"
PUBLIC_DIR = DOCS_ROOT / "public"

# Production origin for the docs site: GitHub Pages serves the repository's
# project subpath. BASE must always match svelte.config.js `paths.base` so
# emitted URLs do not 404 on Pages.
ORIGIN = "https://bohmian-ai.github.io"
BASE = "/wyrd"
SITE = f"{ORIGIN}{BASE}"
SCHEMA_URL = "https://github.com/bohmian-ai/wyrd/tree/main/crates/wyrd-spec/schemas"

SUMMARY = (
    "Wyrd is open-source AI assurance and verification infrastructure. Verify AI "
    "systems before and after release, with verdicts you can trace, reproduce, "
    "and audit."
)

# Mirrors SECTIONS and TOPICS in docs/src/lib/content.ts: the sidebar order.
SECTIONS = ["Get started", "Build and verify", "Connect", "Operate", "Understand", "Reference"]
TOPICS = ["Declare", "Observe", "Verify"]

Page = tuple[str, str, str, str]  # (section, slug, title, description)


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
    """Mirror derived-nav.ts inPrimaryNav(): no drafts, Fathom, or planned pages."""
    return fm.get("draft") != "true" and fm.get("pillar") != "fathom" and fm.get("status") != "planned"


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
        key = (SECTIONS.index(section), TOPICS.index(topic) if topic in TOPICS else 0, _order(fm.get("order")), slug)
        ranked.append((key, (section, slug, fm.get("title", slug), fm.get("description", ""))))
    return [page for _key, page in sorted(ranked)]


def url_for(slug: str) -> str:
    return f"{SITE}/" if not slug else f"{SITE}/{slug}/"


def render_pages_index() -> list[str]:
    lines = ["## Home", "", f"- [Wyrd docs]({url_for('')}): the docs home, with a link to each published section.", ""]
    pages = sidebar_pages()
    for section in SECTIONS:
        entries = [page for page in pages if page[0] == section]
        if not entries:
            continue
        lines += [f"## {section}", ""]
        for _section, slug, title, description in entries:
            suffix = f": {description}" if description else ""
            lines.append(f"- [{title}]({url_for(slug)}){suffix}")
        lines.append("")
    return lines


def load_schema(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def render_llms_txt() -> str:
    lines = ["# Wyrd", "", f"> {SUMMARY}", ""]
    lines += render_pages_index()
    lines += [
        "## Machine-readable schemas",
        "",
        f"- [llms-full.txt]({SITE}/llms-full.txt): this index plus the JSON Schema of every Card kind.",
        f"- [Card and API JSON Schemas]({SCHEMA_URL}): the source schemas generated from `wyrd-spec`.",
        "",
    ]
    return "\n".join(lines)


def render_llms_full() -> str:
    lines = [
        "# Wyrd — full machine-readable reference",
        "",
        "Generated from the docs content tree and `crates/wyrd-spec/schemas/`.",
        "",
    ]
    lines += render_pages_index()
    lines += ["## Card schemas", ""]
    # The CardKind enum is the authority for which kinds exist; each kind's spec
    # schema supplies its own description.
    for kind in sorted(load_schema(SCHEMA_DIR / "card_kind.json")["enum"]):
        schema_file = f"{kind.lower()}_spec.json"
        schema = load_schema(SCHEMA_DIR / schema_file)
        lines += [
            f"### {kind}",
            "",
            schema.get("description", "").split("\n\n", 1)[0],
            "",
            f"Schema source: crates/wyrd-spec/schemas/{schema_file}",
            "",
            "```json",
            json.dumps(schema, indent=2, sort_keys=True),
            "```",
            "",
        ]
    return "\n".join(lines)


def main() -> None:
    PUBLIC_DIR.mkdir(parents=True, exist_ok=True)
    (PUBLIC_DIR / "llms.txt").write_text(render_llms_txt(), encoding="utf-8")
    (PUBLIC_DIR / "llms-full.txt").write_text(render_llms_full(), encoding="utf-8")


if __name__ == "__main__":
    main()
