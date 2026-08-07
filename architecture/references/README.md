# Wyrd Doctrine Reference Library

Shared reference library consumed by Wyrd planning, implementation, and review
skills. Skills own *process*; this library owns *knowledge*. Skills load only
the slices they need.

## Layout

```text
references/
  doctrine/     product framing + canonical boundary facts
  architecture/ implementation patterns ("build it this way")
  languages/    per-language implementation doctrine (Rust, PyO3, Python, TypeScript)
  domain/       specialized surfaces (OLAP / Iceberg / Bifrost)
```

## Files

| File | Purpose |
|---|---|
| `doctrine/positioning-and-vocabulary.md` | Doctrine vocabulary, Card envelope, `CardRef` shape, v1 Card kinds, deleted concepts |
| `doctrine/architecture-constraints.md` | Terse boundary checklist (wyrd/vala/skald, `wyrd-spec` free-of list, deployment topologies, observation identity) |
| `architecture/patterns.md` | Implementation doctrine: full crate inventory, contract placement, server/client/storage/provider/observability/audit patterns |
| `languages/rust-core.md` | Rust ownership, traits, async, allocation, API shape, concrete idiomatic examples |
| `languages/implementation-execution.md` | Mandatory single-task execution contract, escalation, focused verification, diff audit, completion evidence |
| `languages/pyo3-boundaries.md` | PyO3 classes, `fn __new__` rule, GIL, lifetimes, boundary conversion, module registration |
| `languages/errors.md` | Wyrd error codes, `WyrdError` derive, boundary conversion, Rust/Python/TS/HTTP/CLI mapping |
| `languages/python-api-and-stubs.md` | Python exports, generated stubs, package layout, test conventions |
| `languages/testing-workflows.md` | Three-tier test taxonomy, targeted `mise` tasks, boundary checks, aggregate CI gate |
| `languages/agent-harness.md` | Agent-facing contracts, MCP, structured validation, audit foundation |
| `languages/typescript-guide.md` | `@wyrd/sdk` conventions, high-performance TS patterns, declaration file do's/don'ts, napi bridge parity |
| `domain/iceberg-bifrost.md` | OLAP / Iceberg / Bifrost implementation doctrine + rebuild source-of-truth pointers |

## Consumers

- `.agents/skills/wyrd-plan/SKILL.md`
- `.agents/skills/wyrd-implement/SKILL.md`
- `.agents/skills/wyrd-review/SKILL.md`
- Global Claude and Codex `plan-readiness-reviewer` skills

New skills that need shared doctrine route here instead of carrying their
own copy. Add new files only when a doctrine gap forces it — extend an
existing file first.
