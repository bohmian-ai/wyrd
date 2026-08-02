---
name: wyrd-advise
description: Ground a Wyrd conversation in doctrine, design authority, and per-surface expertise. Use when deliberating, weighing approaches, brainstorming design, asking "should we / what's the right way / is this a good idea", or making an architectural, product, API, SDK, CLI, MCP, UI, storage, or DX decision in dialogue. This skill injects context only. It imposes no loop, verdict, artifact, or handoff, and it does not plan (that is $wyrd-plan), implement (that is $wyrd-implement), or review (that is $wyrd-review).
---

# Wyrd Advise

Ground this conversation in Wyrd doctrine, design authority, and per-surface
expertise, then answer, advise, and decide as a Wyrd architect would.

This skill **injects context; it does not dictate output.** It imposes no fixed
loop, no verdict token, no required artifact, and no mandated handoff. Its only
job is to make you read the right authorities and the right reference slices
before you form an opinion, so that decisions made in conversation rest on the
same ground truth as `$wyrd-plan`, `$wyrd-implement`, and `$wyrd-review`.

`$name` denotes a Wyrd skill; load it with the `Skill` tool.

## Establish authority

Before advising on any Wyrd matter, read completely:

1. `AGENTS.md` and `architecture/agent-rules.md`.
2. `architecture/wyrd-design.md` — the active design authority.
3. `architecture/wyrd-doctrine.mdx` before reasoning about Wyrd contracts,
   public or internal APIs, SDKs, CLI, MCP, UI, docs, generated schemas, or
   implementation behavior.

Current design is authority, not immutable history. A requested change may
supersede an existing decision, but only by naming the superseded authority and
what would replace it. Absent that, treat a conflict with current design and
doctrine as unresolved and say so rather than reasoning past it.

When `.codegraph/` exists, use CodeGraph before grep, find, or manual
source-reading for any code-grounded question.

## Load references progressively

Read `architecture/references/README.md`, then load only the slice the
conversation needs. Read the selected reference completely when its topic comes
up; reason *from* it and cite the file it rests on rather than pasting it back.

| Reference | Load when the discussion touches |
|---|---|
| `architecture/references/doctrine/positioning-and-vocabulary.md` | Card vocabulary, `CardRef`, v1 kinds, or removed concepts |
| `architecture/references/doctrine/architecture-constraints.md` | Tier boundaries, deployment, or observation identity |
| `architecture/references/architecture/patterns.md` | Crate placement and server/client/storage/provider/audit patterns |
| `architecture/references/languages/rust-core.md` | Rust ownership, traits, async, allocation, or API shape |
| `architecture/references/languages/pyo3-boundaries.md` | PyO3 classes, GIL, lifetimes, conversions, or registration |
| `architecture/references/languages/errors.md` | Stable errors and boundary mappings |
| `architecture/references/languages/python-api-and-stubs.md` | Python exports, stubs, package layout, or tests |
| `architecture/references/languages/testing-workflows.md` | Test tiers, journey coverage, or boundary gates |
| `architecture/references/languages/agent-harness.md` | MCP and other agent-facing contracts |
| `architecture/references/languages/typescript-guide.md` | `@wyrd/sdk` and napi conventions |
| `architecture/references/domain/iceberg-bifrost.md` | Bifrost, Iceberg, DataFusion, or object storage |

This routing table is shared with `$wyrd-implement` and `$wyrd-review`; keep the
three in lockstep when the reference library changes.

## How to advise

This section constrains reasoning, not format.

- Adopt the collaborator context and lenses in `AGENTS.md` §19 — Ergonomics,
  Value, Simplicity, Blindspots. Complement the user's thinking; volunteer
  opinions and blindspots concisely. Do not restate §19 here.
- Tie each recommendation to a concrete authority: name the doctrine principle,
  design decision, or reference slice it rests on, and cite `file:line` when the
  ground truth is a specific rule.
- When an idea conflicts with a locked decision — for example a `Tool`, `Skill`,
  or `SubAgent` Card kind, a client-tier `sqlx`/cloud/DataFusion/Delta
  dependency, PyO3 in `wyrd-spec`, or business logic moved into `python/py-wyrd`
  — surface the conflict and the governing rule instead of going along with it.
  Distinguish a locked decision from a genuinely open one.
- Flag when a decision is *material* (public, wire, or persisted contract;
  migration or destructive behavior; dependency or Cargo feature;
  auth/tenancy/policy/audit semantics; ownership direction; or acceptance
  outcome) so the user knows it deserves `$wyrd-plan` rigor. Do not force the
  handoff.

## Boundary

This skill only injects context. When the conversation shifts to producing an
executable plan, implementing a change, or reviewing a diff, that is
`$wyrd-plan`, `$wyrd-implement`, or `$wyrd-review` territory — mention the
relevant one when the user crosses into it, but let them choose to switch.
