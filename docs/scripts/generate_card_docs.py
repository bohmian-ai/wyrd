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
    "skill": "skill_spec.json",
    "subagent": "subagent_spec.json",
    "tool": "tool_spec.json",
    "trigger": "trigger_spec.json",
    "workflow": "workflow_spec.json",
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
    "skill": "Declare a reusable skill that an agent can select with predictable inputs.",
    "subagent": "Describe a narrower agent role that can be called by a parent agent.",
    "tool": "Declare an executable tool and the constraints around its use.",
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
            rows.append((name, scalar_type(detail.get("type")), "yes" if name in required else "no"))
    return rows


def title_for(slug: str) -> str:
    if slug == "mcp":
        return "MCP"
    if slug == "subagent":
        return "SubAgent"
    return slug.title()


CARD_KIND_GROUPS = {
    "data": "data",
    "model": "model",
    "experiment": "experiment",
    "drift": "experiment",
    "eval": "experiment",
    "prompt": "prompt",
    "agent": "agent",
    "subagent": "agent",
    "skill": "agent",
    "tool": "agent",
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


def intro_dl(slug: str, schema: dict) -> str:
    kind_attr = f' data-kind="{slug}"' if CARD_KIND_GROUPS.get(slug) != "neutral" else ""
    rows = properties(schema)
    required = [r for r in rows if r[2] == "yes"]
    optional_count = len(rows) - len(required)
    required_str = ", ".join(f"<code>{name}</code>" for name, *_ in required[:4]) or "none required"
    return (
        f'<dl class="wyrd-defs">'
        f'<dt{kind_attr}>{title_for(slug)}</dt>'
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
            lines.append(f"| `{md_escape(name)}` | `{md_escape(field_type)}` | {required} |")
    else:
        lines.append("This schema does not expose top-level fields yet.")

    lines.extend(
        [
            "",
            "## Shape",
            "",
            phase_gate(
                "5a",
                f"Copy-pasteable YAML for a {title}Card lands with the Card write path in Phase 5a. The JSON Schema at <code>{source}</code> is the current source of truth.",
            ),
            "",
            "## Lifecycle",
            "",
            phase_gate(
                "5a",
                f"Write, version, transition, and retire flows for {title}Cards land in Phase 5a.",
            ),
            "",
            "## Authoring notes",
            "",
            "- Keep card names stable; downstream runs, policies, and audits refer to them by identity.",
            "- Put operational rules in the spec instead of burying them in free-form notes.",
            "- Prefer explicit references to other cards when a dependency matters at runtime.",
            "",
            "## Related",
            "",
            f"- [Concepts overview](/concepts/) — where {title} fits in the seven primitives.",
            "- [Card reference index](/cards/) — every kind in one place.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    for slug, schema_file in CARD_SPECS.items():
        (OUT_DIR / f"{slug}.md").write_text(render(slug, schema_file), encoding="utf-8")


if __name__ == "__main__":
    main()
