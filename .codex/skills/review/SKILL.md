---
name: review
description: Wyrd-specific review binding for architecture, ownership, contracts, repository rules, and verification.
---

# Wyrd Review Binding

Apply this repository-specific lens alongside the global review pipeline. Do not
duplicate generic bug, code-quality, or maintainability findings unless the
Wyrd rule or architecture creates a distinct consequence.

Before judging the diff, read:

- `AGENTS.md`;
- `architecture/agent-rules.md`;
- `architecture/wyrd-design.md`;
- `architecture/wyrd-doctrine.mdx`; and
- the nearest owning crate or package manifest and `mise.toml` tasks.

When `.codegraph/` exists, use `codegraph explore` before repository search or
manual source reads to locate changed symbols, callers, ownership boundaries,
and local precedents.

## Wyrd-specific checks

- Verify the change uses Wyrd-native nouns, paths, contracts, and vocabulary;
  do not introduce legacy names, compatibility aliases, or stale migration
  surfaces.
- Confirm the owning crate or package matches the ownership boundaries in
  `AGENTS.md`, including dependency cost and client/server separation.
- Check Cards, `CardRef`, API versions, typed request/response bodies, stable
  errors, generated artifacts, MCP/CLI surfaces, and SDK boundaries against
  the active design authority when they are touched.
- Check tenant isolation, audit boundaries, server-owned durable behavior, and
  language-agnostic wire contracts when relevant.
- Check PyO3 feature gates, thin module aggregation, generated stubs, and
  public Python exports when cross-language surfaces are touched.
- Check Wyrd's test taxonomy and canonical `mise` task for the changed
  surface. A user- or agent-facing capability needs a user-journey test; a
  unit test alone is not sufficient.
- Enforce `AGENTS.md` §5 "Required Struct-Centered Rust Style" as a hard
  acceptance criterion. Treat new or materially changed module-level
  orchestration, repeated dependency/context threading, anemic structs whose
  natural behavior is detached, zero-sized utility structs, broad god objects,
  and traits around one implementation as confirmed findings. Do not force a
  genuinely stateless deterministic helper onto an artificial owner, and do
  not move registry, storage, policy, audit, or lifecycle IO onto declarative
  Card envelopes.
- Enforce the `AGENTS.md` §16 rustdoc requirement as a hard acceptance
  criterion. Inspect every new or materially modified Rust module, type, field,
  variant, trait item, constant, alias, function, method, helper, and test.
  Missing, placeholder, or mechanically restated rustdoc is a confirmed
  `BLOCK_BEFORE_MERGE` finding. Fallible functions require `# Errors`; document
  panics and async cancellation, partial progress, or retry behavior when
  applicable.
- Enforce synchronous Rust as the default. Every changed `async fn` must
  directly await IO or intentionally compose operations that do. Flag pure
  validation, parsing, planning, or transformation made async for caller
  uniformity or hypothetical future IO.
- Treat violations of `architecture/agent-rules.md` or explicit `AGENTS.md`
  requirements as confirmed evidence. Apply the shared materiality gate rather
  than silently ignoring a non-blocking violation.

For every finding, cite the exact rule or design section, changed path and
line, affected owner or contract, concrete workflow or maintenance impact, and
the canonical verification command. Clean output is valid, but state which
Wyrd-specific surfaces were inspected. Do not modify source files.
