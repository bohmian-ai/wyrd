---
id: BIFROST-OTEL-T04-R2
title: Close OTLP remediation standards gaps
kind: remediation
status: proposed
spec: SPEC-bifrost-canonical-otel-signals
spec_revision: 11
original_task: changes/active/bifrost-canonical-otel-signals/tasks/04-integrated-public-journeys.md
original_base: 442da074cb316be7f580694ba8274229561935a8
prior_candidate: 81eaa346e643ac6315e517041fc838c41057f7ac
remediation_base: e2ebc45a6359dad784614ab238540ea85543298c
candidate: 1c9fc1a0bd5b54733e241902b671f1f00345d892
findings: [FIND-BIFROST-OTEL-T04-7, FIND-BIFROST-OTEL-T04-8]
required_skill: wyrd-implement
---

# Close OTLP remediation standards gaps

## Objective

Finish BIFROST-OTEL-T04 without changing the accepted retry-identity behavior: make the changed OTLP trace helper signatures comply with the repository's import rule and make the new two-principal journey evidence exactly reproducible.

## Issue diagnosis

### FIND-BIFROST-OTEL-T04-7 — fully qualified types remain in changed signatures

The R1 implementation correctly moved the production Gate types to module-scope imports, but `crates/wyrd/wyrd-testing/tests/bifrost/otlp/trace_export.rs:32-55` materially changed `export_traces_over_grpc` and added `export_traces_over_grpc_as` while leaving `ResourceSpans` and `ExportTracePartialSuccess` fully qualified in both signatures. `architecture/agent-rules.md` requires ordinary imports at module scope and bare signature types. The helper works, but the remediation remains repository-noncompliant.

### FIND-BIFROST-OTEL-T04-8 — the new journey command is abbreviated

The R1 record names `negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution`, but `BIFROST-OTEL-T04-R1-close-retry-identity-review-gaps.md:152-154` records the Postgres-wrapped invocation with `-E '...'`. `AGENTS.md` §11 requires the exact focused `mise exec -- cargo nextest` command for every named Rust test in a task artifact or implementation report. The journey independently passes, but the durable record cannot reproduce that exact selection.

## Intended correction outcome

Both changed trace-export helper signatures declare their dependencies through module-top imports and use bare type names. The R1 implementation record contains the complete Postgres-wrapped command and passing result for the exact two-principal journey. Runtime behavior, assertions, test placement, and the accepted identity/fence semantics do not change.

## Decision-complete recommendation

Reuse the existing import block in `trace_export.rs`: bring the two signature types into that block and use their bare names in both changed helper signatures. Do not refactor the helper or change its behavior.

Replace the abbreviated new-journey evidence in the existing R1 implementation record with the exact command actually used, including the repository-managed Postgres wrapper, migration setup, package, target, journey profile, exact test expression, and ignored-test flag. Record its passing result. Keep the truthful 15/16 `verify:bifrost` limit and isolated Forge result unchanged.

## Constraints and preserved behavior

- Preserve revision 11 and every accepted original T04 and R1 behavior.
- Preserve the Gate identity inputs, UUID representation, Scribe fence, WAL, acknowledgement, audit, tenancy, and correlation semantics.
- Preserve the public two-principal/run journey and all assertions unchanged.
- Preserve native Arrow ingestion and public OTLP transport behavior.
- Add no public contract, migration, dependency, feature, helper, fixture, target, abstraction, or compatibility path.
- Do not edit unrelated RBAC, Forge, Oracle, SDK, schema, or architecture surfaces.

## Explicit non-goals

- No retry-identity redesign or additional identity coverage.
- No test refactor, fixture extraction, assertion change, or new test.
- No repair of the unrelated Forge publication-cancellation flake.
- No rerun of planning or specification revision.

## Acceptance criteria

| Finding | Required proof |
|---|---|
| `FIND-BIFROST-OTEL-T04-7` | The module import block owns `ResourceSpans` and `ExportTracePartialSuccess`, and both materially changed helper signatures use the bare names. `mise run lints` passes. |
| `FIND-BIFROST-OTEL-T04-8` | The R1 implementation record contains the exact Postgres-wrapped command selecting `negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution` and its passing result; running that exact command selects one test and passes. |

## Focused proof and broader verification

Run Cargo-backed commands sequentially through `mise`:

```bash
mise run fmt
mise run lints
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey -E 'test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)' --run-ignored=all"
mise run verify:bifrost
git diff --check
```

If the already isolated Forge cancellation recurs in `verify:bifrost`, record the aggregate result truthfully and retain exact isolated evidence under the repository's unrelated-failure rule; do not change Forge code for this remediation.
