# Wyrd Doctrine Reference Library

Single source of truth for Wyrd planning, design, implementation, and review
doctrine. Every Wyrd skill — the pipeline stages (`wyrd-spec`, `wyrd-plan`,
`wyrd-tasks`, `wyrd-implement`), the reviewer (`wyrd-architecture-reviewer`), and
the implementation reference (`wyrd-rust-python`) — **routes into this library**
instead of carrying its own copy. Skills own *process*; this library owns
*knowledge*.

Skills load only the slices a stage needs. The mapping from stage → files lives
in each skill's "References" section; this README is the index.

## Layout (by topic)

```
references/
  doctrine/     product framing + canonical boundary facts (stage-agnostic)
  architecture/ implementation patterns ("build it this way")
  rust-python/  language-level implementation doctrine
  review/       review rubrics + blocking patterns ("check this")
  domain/       specialized surfaces (OLAP/Iceberg/Arrow, OTel)
  map/          where things live — source/codebase/surface evidence
```

## Files

| File | Purpose | Primary consumers |
|---|---|---|
| `doctrine/positioning-and-vocabulary.md` | Doctrine vocabulary, Card envelope, forbidden drift | spec, plan, review |
| `doctrine/architecture-constraints.md` | Terse boundary checklist (wyrd/vala/skald, `wyrd-spec` free-of list, surface alignment) | spec, plan, review (fast-check) |
| `architecture/patterns.md` | Implementation doctrine: crate ownership, contract placement, server/client/storage/provider/observability patterns | plan, tasks, implement |
| `rust-python/rust-core.md` | Rust ownership, traits, async, allocation, API shape | tasks, implement |
| `rust-python/pyo3-boundaries.md` | PyO3 classes, GIL, lifetimes, boundary conversion | tasks, implement |
| `rust-python/errors.md` | Wyrd error codes, Rust errors, Python exceptions, problem JSON | tasks, implement |
| `rust-python/python-api-and-stubs.md` | Python exports, generated stubs, package layout | tasks, implement |
| `rust-python/testing-workflows.md` | Test selection, lint, codegen, completion checks (`mise` gates) | tasks, implement, test |
| `rust-python/agent-harness.md` | Agent-facing contracts, MCP, structured validation | spec, tasks, implement |
| `review/review-rubric.md` | Severity and doctrine review dimensions | review (always) |
| `review/implementation-rules.md` | Rust/PyO3/Python/API/MCP/codegen review rules | plan, tasks, review |
| `review/rust-service-architecture.md` | Rust crate/trait/async/backpressure/error review rubric + blocking patterns | plan, review |
| `review/production-architecture-rubric.md` | Security, reliability, performance, HA, distributed-systems thresholds | plan, review |
| `review/agent-first-review.md` | Whether a small agent can reason about Wyrd contracts/docs/errors | spec, review |
| `review/full-sweep-review.md` | Folder-scale review workflow, intake ledger, consensus | review |
| `review/dynamic-review-loop.md` | Terminal review→revise→re-review loop for Gate 1/Gate 2 until `Decision: approve` | spec, plan, review |
| `domain/iceberg-bifrost.md` | OLAP/Iceberg/Bifrost implementation doctrine | tasks, implement (when surface applies) |
| `domain/olap-datafusion-iceberg-arrow.md` | OLAP/DataFusion/Iceberg/Arrow review checks | plan, review (when surface applies) |
| `domain/observability-otel.md` | OTel trace/span/metric/log shape, cardinality, redaction | plan, tasks, review (when surface applies) |
| `map/source-map.md` | Current authority sources + historical/predecessor evidence locations | plan, review |
| `map/codebase-map.md` | Predecessor patterns and migration evidence | plan, review |
| `map/surface-mapping.md` | Source-repo → Wyrd surface migration mapping | plan, review |

## Impl vs. review is intentional, not duplication

Some topics appear twice by design, framed for different stages:

- **Architecture:** `architecture/patterns.md` (implementation — *build it this
  way*) vs. `review/rust-service-architecture.md` (*review whether…* + blocking
  patterns) vs. `doctrine/architecture-constraints.md` (terse fast-check facts).
- **OLAP/Iceberg:** `domain/iceberg-bifrost.md` (implementation) vs.
  `domain/olap-datafusion-iceberg-arrow.md` (review checks).

These are kept separate so each file is self-contained for its stage. The shared
*facts* (crate ownership, the `wyrd-spec` free-of list, product boundaries) are
stated canonically in `doctrine/architecture-constraints.md` and
`architecture/patterns.md`; the review files reference those facts rather than
inventing a third vocabulary. Do **not** add a fourth copy — extend the canonical
file and route to it.
