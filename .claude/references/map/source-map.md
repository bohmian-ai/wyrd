# Source Map

Use this file to choose the smallest authoritative source for a review.

## Current Architecture Authority

Primary repository:

`/Users/stevenforrester/Documents/GitHub/wyrd`

| Path | Use when reviewing |
|---|---|
| `AGENTS.md` | Repository rules, source-of-truth ordering, Wyrd-native vocabulary, ownership boundaries, verification, and completion standard. |
| `architecture/wyrd-design.md` | Active design authority and first design filter for nouns, card kinds, foundations, services, and public surfaces. Wins over generated artifacts, older planning files, and implementation drift. |
| `docs/src/content/docs/concepts/doctrine.svxx` | Public doctrine summary for contracts, APIs, SDK surfaces, CLI, MCP, UI, docs, generated schemas, and implementation behavior. |
| `.dev/plan/` | Current implementation phase plans and handoffs in this repository. |
| `.dev/review/` | Current review artifacts and consensus ledgers in this repository. |
| `PLAN.md` | Pointer to older planning history when historical context is needed. |
| `crates/`, `python/py-wyrd`, `docs/`, `architecture/` | Implementation and public-surface evidence when reviewing concrete behavior. |

## Historical Planning Evidence

Historical planning repository:

`/Users/stevenforrester/Documents/GitHub/wyrd-plan`

Use `wyrd-plan` for predecessor research, old dialogue, sign-off history, or
comparison context only. It does not override the Wyrd repo's `AGENTS.md` or
`architecture/wyrd-design.md`.

| Path | Use when reviewing |
|---|---|
| `architecture/v1/` | Historical architecture context and older planning evidence. |
| `STATUS.md`, `CHANGELOG.md`, `sessions/` | Historical decision dialogue and sign-off evidence. |
| `plans/`, `source-maps/` | Older phase plans, source maps, and predecessor citations. |

## Predecessor Evidence

Predecessor repos are evidence for reuse and parity, not Wyrd public
vocabulary:

| Repo | Evidence to inspect |
|---|---|
| `/Users/stevenforrester/Documents/GitHub/opsml` | Registry, cards, local save/load, storage, auth, CLI, UI, MCP, and PyO3 patterns. |
| `/Users/stevenforrester/Documents/GitHub/scouter` | Drift, eval, trace ingest, alerts, DataFusion/Delta, OLAP, and observability patterns. |
| `/Users/stevenforrester/Documents/GitHub/potatohead` | Providers, prompts, agents, workflows, memory, callbacks, and LLM runtime patterns. |

Legacy migration docs may be useful historical context, but they do not
override the Wyrd repo's `AGENTS.md`, `architecture/wyrd-design.md`, or active
implementation plans.
