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
- Treat violations of `architecture/agent-rules.md` or explicit `AGENTS.md`
  requirements as confirmed evidence. Apply the shared materiality gate rather
  than silently ignoring a non-blocking violation.

For every finding, cite the exact rule or design section, changed path and
line, affected owner or contract, concrete workflow or maintenance impact, and
the canonical verification command. Clean output is valid, but state which
Wyrd-specific surfaces were inspected. Do not modify source files.
