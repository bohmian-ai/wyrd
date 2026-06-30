#!/usr/bin/env python3
"""Generate /llms.txt and /llms-full.txt for the Wyrd docs site.

Both files live in docs/public/ so the static site serves them under the
deploy base (GitHub Pages project subpath /wyrd/).

llms.txt       - compact index for agents (links + one-line purposes). The
                 page list is derived from the content actually on disk so it
                 can never advertise a route the build does not serve.
llms-full.txt  - full JSON Schema dump per card kind, plus the same page index.
"""

from __future__ import annotations

import json
from pathlib import Path


DOCS_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = DOCS_ROOT.parent
SCHEMA_DIR = REPO_ROOT / "crates" / "wyrd-spec" / "schemas"
CONTENT_DIR = DOCS_ROOT / "src" / "content" / "docs"
PUBLIC_DIR = DOCS_ROOT / "public"

# Production origin for the docs site. Pages deploys to the project subpath
# /wyrd/ under the org's github.io domain. BASE must always match
# svelte.config.js `paths.base` so emitted URLs do not 404 on Pages.
ORIGIN = "https://mitari-ai.github.io"
BASE = "/wyrd"
SITE = f"{ORIGIN}{BASE}"

# The home route is served by src/routes/+page.svelte, not by a content file,
# so it is described here explicitly.
HOME = (
    "",
    "Wyrd",
    "The typed control layer for AI systems: cards, a registry, and the Skald runtime.",
)

# Section directories in the order they appear in the site nav. Ordered to
# match the locked 7-section Diátaxis IA (Decision 8 from commit 09):
# Overview / Setup / Tutorials / How-to / Concepts / Reference / For Agents.
# Any content directory not listed here is appended afterwards in alphabetical
# order so a new section never silently drops out of llms.txt.
SECTION_ORDER = [
    "overview",
    "setup",
    "tutorials",
    "how-to",
    "concepts",
    "cards",
    "api",
    "for-agents",
    # Legacy sections that predate the Diátaxis IA; kept in SECTION_ORDER so
    # they appear in a stable position rather than being sorted alphabetically.
    "start-here",
    "skald",
    "guides",
    "agents",
    "evaluation",
    "server",
    "python",
    "roadmap",
    "fathom",
]

SECTION_TITLES = {
    "": "Home",
    "overview": "Overview",
    "setup": "Setup",
    "tutorials": "Tutorials",
    "how-to": "How-to guides",
    "concepts": "Concepts",
    "cards": "Cards",
    "api": "API reference",
    "for-agents": "For Agents",
    "start-here": "Start here",
    "skald": "Skald runtime",
    "guides": "Guides",
    "agents": "Agents",
    "evaluation": "Evaluation",
    "server": "Server",
    "python": "Python",
    "roadmap": "Roadmap",
    "fathom": "Fathom",
}

# Card kinds that ship a dedicated reference page under /cards/. Every other
# kind appears only as a row in the catalog table on /cards/, so its schema in
# llms-full.txt links to that catalog rather than a route that does not exist.
CARDS_WITH_PAGES = {"data", "model", "prompt"}

CARD_SPECS = {
    "agent": "agent_spec.json",
    "artifact": "artifact_spec.json",
    "audit": "audit_spec.json",
    "data": "data_spec.json",
    "drift": "drift_spec.json",
    "eval": "eval_spec.json",
    "experiment": "experiment_spec.json",
    "mcp": "mcp_spec.json",
    "model": "model_spec.json",
    "operator": "operator_spec.json",
    "policy": "policy_spec.json",
    "prompt": "prompt_spec.json",
    "service": "service_spec.json",
    "trigger": "trigger_spec.json",
    "workflow": "workflow_spec.json",
}

PURPOSES = {
    "agent": "Declare an agent that Wyrd can describe, govern, evaluate, install references for, and observe. Execution stays in the user or framework runtime.",
    "artifact": "Track an immutable artifact that belongs to a card, run, or service release.",
    "audit": "Represent a reviewable event or decision that needs durable provenance.",
    "data": "Describe a dataset, feature table, document set, or other data dependency.",
    "drift": "Describe the drift contract Wyrd uses to watch behavior over time.",
    "eval": "Define an evaluation suite, scoring rule, or acceptance check.",
    "experiment": "Group work-in-progress runs and candidate changes under a single intent.",
    "mcp": "Describe an MCP surface that tools and agents can discover consistently.",
    "model": "Describe a model artifact, its interface, and the context needed to use it safely.",
    "operator": "Describe an operator that performs bounded work inside a workflow or service.",
    "policy": "Capture rules that decide whether a card, run, or action is allowed.",
    "prompt": "Version prompt content and the contract around its inputs and outputs.",
    "service": "Describe a deployable service and the runtime rules Wyrd can lock.",
    "trigger": "Describe an event source that can start a workflow or service action.",
    "workflow": "Describe a coordinated sequence of operators, tools, agents, or services.",
}


def slug_for(path: Path) -> str:
    """Mirror docs/src/lib/content.ts toSlug() so URLs match served routes."""
    rel = path.relative_to(CONTENT_DIR).as_posix()
    rel = rel.rsplit(".", 1)[0]
    if rel == "index":
        return ""
    if rel.endswith("/index"):
        return rel[: -len("/index")]
    return rel


def read_frontmatter(path: Path) -> tuple[str, str]:
    """Return (title, description) from a leading `---` YAML front-matter block.

    A minimal line parser keeps the generator free of a YAML dependency in the
    docs build environment.
    """
    text = path.read_text(encoding="utf-8")
    title = ""
    description = ""
    if text.startswith("---"):
        end = text.find("\n---", 3)
        block = text[3:end] if end != -1 else ""
        for line in block.splitlines():
            stripped = line.strip()
            if stripped.startswith("title:"):
                title = _unquote(stripped[len("title:") :].strip())
            elif stripped.startswith("description:"):
                description = _unquote(stripped[len("description:") :].strip())
    if not title:
        title = slug_for(path).rsplit("/", 1)[-1] or path.stem
    return title, description


def _unquote(value: str) -> str:
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {'"', "'"}:
        return value[1:-1]
    return value


def collect_pages() -> list[tuple[str, str, str, str]]:
    """Scan the content tree and return (section, slug, title, description).

    The home route is injected first; every other entry is a real file on
    disk, so the list always matches what the build prerenders.
    """
    pages: list[tuple[str, str, str, str]] = [("", HOME[0], HOME[1], HOME[2])]
    for path in sorted(CONTENT_DIR.rglob("*")):
        if path.suffix not in {".svx", ".md"}:
            continue
        slug = slug_for(path)
        section = slug.split("/", 1)[0] if slug else ""
        title, description = read_frontmatter(path)
        pages.append((section, slug, title, description))
    return pages


def grouped_pages(
    pages: list[tuple[str, str, str, str]],
) -> list[tuple[str, list[tuple[str, str, str, str]]]]:
    sections: dict[str, list[tuple[str, str, str, str]]] = {}
    for entry in pages:
        sections.setdefault(entry[0], []).append(entry)
    ordered = [s for s in SECTION_ORDER if s in sections]
    extras = sorted(s for s in sections if s and s not in SECTION_ORDER)
    result: list[tuple[str, list[tuple[str, str, str, str]]]] = []
    # Home first.
    if "" in sections:
        result.append(("", sections[""]))
    for section in ordered + extras:
        result.append((section, sorted(sections[section], key=lambda e: e[1])))
    return result


def url_for(slug: str) -> str:
    return f"{SITE}/" if not slug else f"{SITE}/{slug}/"


def render_pages_index() -> list[str]:
    lines: list[str] = []
    for section, entries in grouped_pages(collect_pages()):
        heading = SECTION_TITLES.get(section, section.replace("-", " ").title())
        lines += [f"## {heading}", ""]
        for _section, slug, title, description in entries:
            suffix = f": {description}" if description else ""
            lines.append(f"- [{title}]({url_for(slug)}){suffix}")
        lines.append("")
    return lines


def card_url(slug: str) -> str:
    return f"{SITE}/cards/{slug}/" if slug in CARDS_WITH_PAGES else f"{SITE}/cards/"


def load_schema(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def title_for(slug: str) -> str:
    if slug == "mcp":
        return "MCP"
    return slug.title()


def render_llms_txt() -> str:
    lines = [
        "# Wyrd",
        "",
        "> Wyrd is the declarative AI layer for developers, engineers, and agents. A standardized ecosystem for models, data, prompts, agents, workflows, evals, policies, audits, services, sources, and observations. Runtime tools resolve through tool registries; execution stays in the user runtime.",
        "",
    ]
    lines += render_pages_index()
    lines += [
        "## Machine-readable schemas",
        "",
        f"- [llms-full.txt]({SITE}/llms-full.txt): every card JSON Schema plus this page index.",
        "- JSON Schemas: see `crates/wyrd-spec/schemas/` in the wyrd repository. `architecture/wyrd-design.md` remains the design authority while schemas are reconciled.",
        "",
    ]
    return "\n".join(lines)


def render_llms_full() -> str:
    lines = [
        "# Wyrd — full machine-readable reference",
        "",
        "Generated from the docs content tree and `crates/wyrd-spec/schemas/`. `architecture/wyrd-design.md` is the design authority; this schema dump may include implementation drift while `wyrd-spec` is reconciled.",
        "",
    ]
    lines += render_pages_index()
    lines += ["## Card schemas", ""]
    for slug, schema_file in sorted(CARD_SPECS.items()):
        title = title_for(slug)
        schema = load_schema(SCHEMA_DIR / schema_file)
        lines += [
            f"### {title}Card",
            "",
            PURPOSES[slug],
            "",
            f"URL: {card_url(slug)}",
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
