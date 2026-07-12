---
name: review
description: Repo-local Wyrd architecture and contract review. Use when reviewing Wyrd code, docs, plans, APIs, SDKs, schemas, CLI, MCP, UI, storage, Vala, Skald, or diffs that must preserve Wyrd doctrine and ownership boundaries.
---

# Wyrd Architecture and Contract Review

Review Wyrd changes for doctrine, ownership, and cross-surface contract integrity.

## Required authority

Read completely:

1. `AGENTS.md`
2. `architecture/agent-rules.md`
3. `architecture/wyrd-design.md`
4. `architecture/wyrd-doctrine.mdx`
5. Changed files and the nearest implementation, tests, schemas, generated artifacts, and public surfaces

`architecture/wyrd-design.md` wins over generated artifacts, predecessor repositories, and implementation drift. The implementation plan is the primary source of change intent. Read its goals, invariants, dependency decisions, and non-goals before judging the diff. When the plan conflicts with active design authority, report an authority conflict and explain both sides; do not silently prefer the code or prescribe a reversal from one rule alone. Do not treat `wyrd-plan` as active authority.

When invoked by `review-and-plan`, use its shared packet and required output path. Do not gather a second diff or create a new review ID.

## Review

Report only concrete drift that could encode the wrong contract or user workflow.

Check:

- 16 native Card kinds plus `External`; no `Tool`, `Skill`, or `SubAgent` Card kinds;
- one shared Card envelope, foundations, `CardRef`, relationships, and status model;
- server-owned durable behavior and language-agnostic wire contracts;
- Rust, Python, and TypeScript surface parity where first-class behavior ships;
- `wyrd-spec` remains foundational, IO-free, async-free, and PyO3-free;
- approved Python owner crates own Python-visible behavior behind optional features;
- `python/py-wyrd` remains a thin aggregator;
- client-tier crates do not acquire server/data-plane dependencies;
- Skald remains independent of Vala; Vala may depend on Skald to implement the
  reusable agent evaluation engine used by offline consumers and `wyrd-server`;
- `wyrd-server` remains the only HTTP/gRPC serving surface;
- tenant isolation, permissions, stable errors, and audit context cross durable paths;
- MCP, CLI, HTTP, schemas, docs, and SDKs expose the same nouns and lifecycle;
- user/agent-facing capabilities ship real client → server → client journeys;
- no predecessor vocabulary, compatibility aliases, or stale design authority is introduced.

Before reporting an architecture or ownership finding, evaluate the proposed
alternative. State its dependency and compilation impact, how it preserves the
plan's user workflow, and why it is materially better. Classify disagreements as
implementation defects, stale authority, or design questions. A written-rule
violation alone is not a finding.

## Findings

For each CRITICAL, MAJOR, or MINOR finding include:

- **Location**
- **Issue**
- **Doctrine**
- **Why it matters**
- **Evidence**
- **Fix or verification gate**

Consolidate shared root causes. If no issue clears the bar, state that the change aligns with current Wyrd authority. Do not modify source files.
