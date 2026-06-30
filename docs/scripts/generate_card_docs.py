#!/usr/bin/env python3
"""Generate Wyrd card reference pages from checked-in JSON Schemas."""

from __future__ import annotations

import json
import re
from pathlib import Path


DOCS_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = DOCS_ROOT.parent
SCHEMA_DIR = REPO_ROOT / "crates" / "wyrd-spec" / "schemas"
OUT_DIR = DOCS_ROOT / "src" / "content" / "docs" / "cards"

# Only the three shipped card kinds get a generated reference page. The full
# catalog of all kinds (with shipped/spec-only status) is the hand-authored
# table in cards/index.mdx; spec-only kinds do not get standalone prose pages.
CARD_SPECS = {
    "data": "data_spec.json",
    "model": "model_spec.json",
    "prompt": "prompt_spec.json",
}

PURPOSES = {
    "agent": "Declare an agent that Wyrd can describe, govern, evaluate, install references for, and observe. Execution stays in the user or framework runtime. Wyrd does not host user agent loops or execute user agent code.",
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


def md_escape(value: str) -> str:
    value = re.sub(r"[\n\r\t\x00-\x1f]", " ", value)
    value = value.replace("|", r"\|")
    value = value.replace("`", r"\`")
    value = value.replace("<", "&lt;").replace(">", "&gt;")
    return value


def load_schema(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def scalar_type(value: object) -> str:
    if isinstance(value, list):
        return " | ".join(str(item) for item in value)
    if isinstance(value, str):
        return value
    return "object"


def properties(schema: dict) -> list[tuple[str, str, str]]:
    props = schema.get("properties", {})
    required = set(schema.get("required", []))
    rows: list[tuple[str, str, str]] = []
    if isinstance(props, dict):
        for name, detail in sorted(props.items()):
            if not isinstance(detail, dict):
                rows.append((name, "object", "yes" if name in required else "no"))
                continue
            rows.append(
                (
                    name,
                    scalar_type(detail.get("type")),
                    "yes" if name in required else "no",
                )
            )
    return rows


def title_for(slug: str) -> str:
    if slug == "mcp":
        return "MCP"
    return slug.title()


CARD_KIND_GROUPS = {
    "data": "data",
    "model": "model",
    "experiment": "experiment",
    "drift": "experiment",
    "eval": "experiment",
    "prompt": "prompt",
    "agent": "agent",
    "mcp": "agent",
    "operator": "agent",
    "workflow": "agent",
    "trigger": "agent",
    "service": "service",
    "artifact": "neutral",
    "audit": "neutral",
    "policy": "neutral",
}


def phase_gate(phase: str, body: str) -> str:
    return (
        f'<aside class="wyrd-phase-gate">'
        f'<span class="wyrd-phase-gate__badge">Phase {phase}</span>'
        f"{body}"
        f"</aside>"
    )


LIFECYCLE_NOTES = {
    "agent": [
        "An `AgentCard` is the declarative spec; the live agent runtime lives in",
        "`skald-agent`. A Wyrd consumer turns an `AgentCard` into a live `Agent` by",
        "unwrapping the inner `AgentDef`, binding a provider client from the",
        "`skald-runtime` `ProviderRegistry`, resolving tools from the Wyrd-side tool",
        "registry, and constructing the live agent via `Agent::from_def`.",
        "",
        "See [agent runtime](/concepts/agent-runtime/) for the layering and the",
        "`Observer` hook surface.",
    ],
    "workflow": [
        "A `WorkflowCard` is the declarative spec; the live workflow engine lives in",
        "`skald-workflow`. A Wyrd consumer turns a `WorkflowCard` into a live",
        "`Workflow` by unwrapping the inner `WorkflowDef` and calling",
        "`Workflow::from_def(def, providers, tools, observer)`. The engine binds every",
        "agent, validates the task graph, and produces a runnable workflow.",
        "",
        "`Workflow::run` returns a `WorkflowRun` with per-task outcomes, events, and the",
        "terminal task id. `Workflow::execute_task` runs one task at a time for callers",
        "that own their own loop.",
        "",
        "See [agent runtime](/concepts/agent-runtime/) for handoff and observability.",
    ],
}


RELATED_NOTES = {
    "agent": "- [Agent runtime](/concepts/agent-runtime/) — how AgentCards lower into live Skald agents.",
    "workflow": "- [Agent runtime](/concepts/agent-runtime/) — how WorkflowCards lower into live Skald workflows.",
}


def intro_dl(slug: str, schema: dict) -> str:
    kind_attr = (
        f' data-kind="{slug}"' if CARD_KIND_GROUPS.get(slug) != "neutral" else ""
    )
    rows = properties(schema)
    required = [r for r in rows if r[2] == "yes"]
    optional_count = len(rows) - len(required)
    required_str = (
        ", ".join(f"<code>{name}</code>" for name, *_ in required[:4])
        or "none required"
    )
    return (
        f'<dl class="wyrd-defs">'
        f"<dt{kind_attr}>{title_for(slug)}</dt>"
        f"<dd>{PURPOSES[slug]}</dd>"
        f"<dt>Required</dt>"
        f"<dd>{required_str}</dd>"
        f"<dt>Optional</dt>"
        f"<dd>{optional_count} additional spec fields — see table below.</dd>"
        f"</dl>"
    )


def render(slug: str, schema_file: str) -> str:
    schema_path = SCHEMA_DIR / schema_file
    title = title_for(slug)
    schema = load_schema(schema_path)
    rows = properties(schema)
    source = schema_path.relative_to(REPO_ROOT)

    lines = [
        "---",
        f"title: {title}",
        f"description: Generated reference for the Wyrd {title} card spec.",
        "---",
        "",
        f"# {title}",
        "",
        PURPOSES[slug],
        "",
        intro_dl(slug, schema),
        "",
        "This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.",
        "",
        "## Source",
        "",
        f"- Schema: `{source}`",
        "",
        "## Fields",
        "",
    ]

    if rows:
        lines.extend(["| Field | Type | Required |", "| --- | --- | --- |"])
        for name, field_type, required in rows:
            lines.append(
                f"| `{md_escape(name)}` | `{md_escape(field_type)}` | {required} |"
            )
    else:
        lines.append("This schema does not expose top-level fields yet.")

    lifecycle = LIFECYCLE_NOTES.get(
        slug,
        [
            f"`{title}Card` is a shipped holder: author it locally, then register it to a",
            "server. Registration is idempotent on identity and rejects a changed spec at",
            "the same version. See [Register cards](/server/register-cards/).",
        ],
    )

    lines.extend(
        [
            "",
            "## Shape",
            "",
            f"The `card.json` envelope wraps this spec under `kind: {title}`. For a runnable "
            f"authoring walkthrough, see the [Python SDK](/python/). The JSON Schema at "
            f"`{source}` is the field-level source of truth.",
            "",
            "## Lifecycle",
            "",
            *lifecycle,
            "",
            "## Authoring notes",
            "",
            "- Keep card names stable; downstream runs, policies, and audits refer to them by identity.",
            "- Put operational rules in the spec instead of burying them in free-form notes.",
            "- Prefer explicit references to other cards when a dependency matters at runtime.",
            "",
            "## Related",
            "",
            f"- [How it connects](/start-here/how-it-connects/) — where {title} fits in the card model.",
        ]
    )
    if related_note := RELATED_NOTES.get(slug):
        lines.append(related_note)
    lines.extend(
        [
            "- [Card reference index](/cards/) — every kind in one place.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for slug, schema_file in CARD_SPECS.items():
        # Never clobber a hand-authored page for a kind (none exist today).
        if (OUT_DIR / f"{slug}.svx").exists() or (OUT_DIR / f"{slug}.mdx").exists():
            continue
        (OUT_DIR / f"{slug}.md").write_text(render(slug, schema_file), encoding="utf-8")


if __name__ == "__main__":
    main()
