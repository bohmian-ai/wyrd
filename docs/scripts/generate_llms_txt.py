#!/usr/bin/env python3
"""Generate /llms.txt and /llms-full.txt for the Wyrd docs site.

Both files live in docs/public/ so Astro serves them at the site root.

llms.txt           - compact index for agents (links + one-line purposes).
llms-full.txt      - full schema dump per card kind, plus concept summaries.
"""

from __future__ import annotations

import json
from pathlib import Path


DOCS_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = DOCS_ROOT.parent
SCHEMA_DIR = REPO_ROOT / "crates" / "wyrd-spec" / "schemas"
PUBLIC_DIR = DOCS_ROOT / "public"

SITE = "https://wyrd.ai"

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
    "source": "Declare where Wyrd reads external observations, archived runs, or object-store evidence.",
    "trigger": "Describe an event source that can start a workflow or service action.",
    "workflow": "Describe a coordinated sequence of operators, tools, agents, or services.",
}

DESIGN_ONLY_CARDS = {
    "source": {
        "url": f"{SITE}/cards/source/",
        "schema": "SourceSpec is design-authoritative in architecture/wyrd-design.md and pending in wyrd-spec.",
    }
}

CONCEPTS = [
    ("card", "Cards", "The durable, versioned envelope for one thing Wyrd governs."),
    ("spec", "Specs", "The typed payload inside a Card. One schema per kind."),
    ("run", "Runs", "The record of an execution tied to a specific Card version."),
    ("observation", "Observations", "Structured behavioral signals attached to a Run."),
    ("policy-card", "Policies", "Decision rules evaluated when Cards are written or Runs are created."),
    ("audit", "Audit", "Immutable entries describing who or what made a decision."),
    ("lineage", "Lineage", "The typed graph that connects Cards to Cards, and Cards to Runs to Observations."),
    ("service-card", "Services", "The Card kind that packages a deployable AI surface."),
]


def title_for(slug: str) -> str:
    if slug == "mcp":
        return "MCP"
    return slug.title()


def load_schema(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def render_llms_txt() -> str:
    lines = [
        "# Wyrd",
        "",
        "> Wyrd is the declarative AI layer for developers, engineers, and agents. A standardized ecosystem for models, data, prompts, agents, workflows, evals, policies, audits, services, sources, and observations. Runtime tools resolve through tool registries; execution stays in the user runtime.",
        "",
        "## Concepts",
        "",
    ]
    for slug, title, purpose in CONCEPTS:
        lines.append(f"- [{title}]({SITE}/concepts/{slug}/): {purpose}")
    lines += ["", "## Card kinds", ""]
    for slug in sorted([*CARD_SPECS, *DESIGN_ONLY_CARDS]):
        lines.append(f"- [{title_for(slug)}Card]({SITE}/cards/{slug}/): {PURPOSES[slug]}")
    lines += [
        "",
        "## Machine-readable schemas",
        "",
        f"- [llms-full.txt]({SITE}/llms-full.txt): current schema inventory plus doctrine notes.",
        "- JSON Schemas: see `crates/wyrd-spec/schemas/` in the wyrd repository. `architecture/wyrd-design.md` remains the design authority while schemas are reconciled.",
        "",
    ]
    return "\n".join(lines)


def render_llms_full() -> str:
    lines = [
        "# Wyrd — full machine-readable reference",
        "",
        "Generated from `crates/wyrd-spec/schemas/`. `architecture/wyrd-design.md` is the design authority; this schema dump may include implementation drift while `wyrd-spec` is reconciled.",
        "",
    ]
    for slug, schema_file in sorted(CARD_SPECS.items()):
        title = title_for(slug)
        schema = load_schema(SCHEMA_DIR / schema_file)
        lines += [
            f"## {title}Card",
            "",
            PURPOSES[slug],
            "",
            f"URL: {SITE}/cards/{slug}/",
            f"Schema source: crates/wyrd-spec/schemas/{schema_file}",
            "",
            "```json",
            json.dumps(schema, indent=2, sort_keys=True),
            "```",
            "",
        ]
    for slug, meta in sorted(DESIGN_ONLY_CARDS.items()):
        title = title_for(slug)
        lines += [
            f"## {title}Card",
            "",
            PURPOSES[slug],
            "",
            f"URL: {meta['url']}",
            meta["schema"],
            "",
        ]
    return "\n".join(lines)


def main() -> None:
    PUBLIC_DIR.mkdir(parents=True, exist_ok=True)
    (PUBLIC_DIR / "llms.txt").write_text(render_llms_txt(), encoding="utf-8")
    (PUBLIC_DIR / "llms-full.txt").write_text(render_llms_full(), encoding="utf-8")


if __name__ == "__main__":
    main()
