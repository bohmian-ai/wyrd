# TASK-008 round-eight maintainer review

## Subject and scope

- Candidate: `c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Remediation range: `ea0ed46fa..c4bc77a5c877c508191dc606b3cd3bb78047dc29`.
- Approved authority: `changes/active/verified-change-contract/spec.md`,
  revision 57.
- Original task:
  `changes/active/verified-change-contract/tasks/task-008-closeout.md`.
- Prior finding under review: `FIND-TASK-008-CLOSEOUT-17` from
  `changes/active/verified-change-contract/review/TASK-008-r7/`.

This review is limited to whether the remediation is maintainable at the
changed surfaces and whether that range introduces a maintenance regression.
Earlier accepted code is not reopened. `FIND-TASK-008-CLOSEOUT-13` remains
deferred to integration. The candidate remained at the named commit throughout
this review. This checkout has no `.codegraph/` directory, so navigation used
Git, `rg`, and direct source inspection.

## Changed-surface coverage

| Changed surface | Owner, callers, and tests inspected | Governing principle | Result |
|---|---|---|---|
| `Queue::poll` in `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:307` | Read the complete `evidence.rs` module, `Backlog::with_replicas`, `Queue::backlog`, every direct test call, and the sole runtime caller `Deployment::drain` in `capacity/step.rs:395` | Workflows remain methods on the dependency-owning concrete owner; the main path and sequencing invariant must be discoverable; materially modified Rust needs substantive rustdoc including errors and cancellation | **PASS.** `Queue` already owns the durable pool and the poll workflow. The method keeps the existing first-reading fast path and expresses the exceptional empty-path retry locally as `S1 -> Q1 -> S2 -> Q2`. Its rustdoc explains why the ordering closes the pending-to-durable handoff, names the caller-owned repetition/deadline, and retains `# Errors` and `# Cancellation`. No helper, trait, configuration surface, or new owner was introduced. |
| `Deployment::drain` consumer in `crates/wyrd/wyrd-testing/src/bin/capacity/step.rs:395` | Read `Deployment`, `Deployment::run`, the full drain loop, and its `Drain::judge` transition | Callers should discover the workflow through the owning type, and ownership of repetition, deadlines, and result interpretation should remain clear | **PASS.** The caller is unchanged and continues to own repeated polling, the exact drain deadline, and the final scrape returned for evidence. The extra durable read is encapsulated by `Queue::poll`, where the process/durable observation invariant belongs. |
| Held-commit Oracle proof in `crates/wyrd/wyrd-testing/src/bin/capacity/evidence.rs:557` and the added crossed-handoff phase at line 725 | Read the complete `pg_tests` module and helpers, the production `OracleQueryAudit` owner/writer in `wyrd-server/src/oracle/query_audit.rs`, and the audit publication architecture | Tests should prove a caller-relevant outcome, keep environment-backed proof in `pg_tests`, and explain non-obvious orchestration and failure meaning | **PASS.** The existing public Oracle proof is extended in place rather than adding a fixture or test binary. The test documentation describes both schedules. Assertions name creation after the first scrape, the pre-commit durable state, pending release before the second scrape, visibility in the second durable read, persistence above the watermark, and clearance after publication. This makes the regression schedule followable without knowing the prior review. |
| Remediation implementation evidence in `TASK-008-CLOSEOUT-R6-close-final-audit-handoff.md` | Compared the added acceptance table and verification record with the source diff and prior finding | Evidence should identify current owners and observable proof without creating permanent production-code history or claiming deferred qualification | **PASS.** The record accurately names `Queue::poll`, the public Oracle proof, preserved adjacent behavior, and the deferred full capacity qualification. No task or finding identifier entered production code. |
| Generated declarations, public contracts, and documentation parity | Inspected the two-file remediation diff and affected ownership boundary | Public signatures and generated declarations must remain aligned when changed | **PASS / N/A.** The range changes a benchmark-internal method body, its rustdoc, an environment-owned test, and task evidence. It changes no public API, generated declaration, schema, report shape, or language SDK surface. |

## FIND-17 maintainer closure

The remediation places the correction at the existing observation owner and
keeps the caller contract unchanged. The method now pairs the final replica
scrape with a later durable read before accepting zero. A maintainer can find
the behavior from `Deployment::drain`, follow it into the cohesive `Queue`
method, and understand the four-observation completeness argument from the
method rustdoc. The focused Postgres proof makes the previously missed commit
crossing explicit against the real audit producer and publisher.

`FIND-TASK-008-CLOSEOUT-17` is therefore **closed from the maintainer-review
lens**.

## Material findings

None.

## Calibration notes

The crossed-handoff phase performs an additional zero-backlog assertion inside
the second scrape callback before releasing the held commit. The callback's
position already establishes that the first durable read completed, so this is
somewhat redundant, but it makes the pre-release state explicit and carries no
concrete maintenance or correctness cost. It is not a finding.

## Verification limits

The remediation record reports the focused Postgres handoff proof, pure
arithmetic and drain-edge tests, the complete ignored-inclusive capacity test
target, fixed-port regression pair, release-server tests, server journey,
formatting, lints, and diff checks as passing. This maintainer pass inspected
source and recorded evidence; it did not rerun the environment-backed proof or
the deferred full default capacity benchmark.

## Overall result

**PASS**

The remediation is cohesive, documented, test-readable, and introduces no
material maintainability regression within the user-directed range.
