# Wyrd Review Criteria

This document calibrates code review findings for Wyrd. It is review guidance,
not a substitute for `AGENTS.md`, `architecture/agent-rules.md`, or
`architecture/wyrd-design.md`.

## Production-readiness bar

A finding becomes current implementation work only when it has a concrete,
reachable production path and a material impact on one or more of these areas:

- tenant isolation, authentication, authorization, secrets, auditability, or a
  named compliance control;
- data correctness, loss, corruption, migration safety, idempotency, replay,
  retry, cancellation, restart, or distributed coordination;
- availability, bounded resource use, latency, throughput, backpressure,
  timeouts, query cost, or operational recovery at an expected workload;
- a public HTTP, MCP, CLI, Rust, Python, or TypeScript contract that callers
  can misread, violate, or cannot use safely;
- maintainability problems that create a likely ownership, drift, testability,
  dependency, or recovery failure.

The finding must state the triggering input, state, workload, deployment
condition, or caller path. A rule or plan mismatch is not sufficient by itself
unless the rule is an explicit non-negotiable gate or the mismatch creates a
concrete production or maintenance failure.

## Disposition

Validation assigns one disposition to every deduplicated finding:

- `BLOCK_BEFORE_MERGE`: unsafe to merge without the fix or investigation.
- `FIX_BEFORE_PRODUCTION`: may be staged in the current slice, but must be
  resolved before the affected production deployment or public release.
- `FOLLOW_UP`: real and worth tracking, but not required in this slice.
- `VERIFICATION_REQUIRED`: static evidence is insufficient; record the exact
  check and keep it out of implementation work until confirmed.
- `KNOWN_DEFERRED`: intentionally deferred by the plan or an explicit owner
  decision; preserve the reference without creating duplicate work.
- `FALSE_POSITIVE`: the reviewer assessment is not supported after validation.

Only `BLOCK_BEFORE_MERGE` and `FIX_BEFORE_PRODUCTION` create fix designs and
implementation-plan items. A real issue may remain in `validation.md` as a
follow-up without blocking the current change.

## Review lenses

Keep the evidence bar high for every lens, but prioritize findings in this
order:

1. correctness, tenant safety, security, compliance, durability, and
   distributed reliability;
2. performance, boundedness, availability, and operability;
3. public contract and developer/agent ergonomics;
4. maintainability, ownership, dependency direction, and semantic
   documentation;
5. style and cleanup only when they affect one of the categories above.

Missing unit tests, comments, abstractions, or refactors are not standalone
required changes unless they prove or prevent a realistic regression, preserve
a public contract, expose a non-obvious invariant, or satisfy a named journey
or repository gate.

## Routing

Use the quick review for a normal implementation slice. Use the full review for
public contracts, persistence or migrations, tenant/auth/audit behavior,
distributed or async coordination, storage/OLAP, performance-sensitive paths,
phase completion, or merge readiness. High-risk changes may run every lens;
low- and standard-risk changes should select only lenses triggered by the
changed surface.
