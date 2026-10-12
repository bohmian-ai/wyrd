---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-bifrost-variant-main-integration
spec_revision: 4
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-020, REQ-021, REQ-022, REQ-023, REQ-025, REQ-026, REQ-027]
primary_requirements: [REQ-026, REQ-027]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011, INV-012]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019, AC-020, AC-021, AC-022, AC-023, AC-025, AC-026, AC-027, AC-028]
scenarios: [S23]
depends_on: [TASK-002]
---

# Final qualification

## Outcome and Value

S's benchmark workloads run unchanged by name from the one `capacity` binary, and the review handoff is prepared. That leaves the change ready for the spec-owned immutable change review. The AC-028 ledger (P-028, formerly TASK-006-C02) moved to [TASK-002](TASK-002-close-the-integration.md) in the 2026-10-12 planning amendment.

Primary closure: REQ-026 / AC-026 and REQ-027 / AC-027. AC-028 is closed in TASK-002 and only consolidated here. Every other listed requirement is consolidated evidence only, owned by the task named in [decomposition](../decomposition.md).

Struck, with no obligation here: REQ-005, REQ-006, REQ-007's settlement model, and REQ-024. All of INV-001 to INV-012 apply.

Use `$wyrd-implement`. An implementer's self-report does not count as independent task completion.

## Owners, Scope, Consumers, and Prohibited Changes

**Owners:**
- the `wyrd-testing` `capacity` binary (`src/bin/capacity/{query,client_queue}` and its shared modules);
- the `bench:bifrost:*` tasks in `mise.toml`;
- `dispositions.tsv`, `evidence.md` and the review handoff.

**Consumers:**
- `changes/active/bifrost-variant/BENCHMARK-HANDOFF.md`, which must run as written;
- the final change review.

**Prohibited:**
- Changing S's workload inputs, steps, assertions, metrics, report layout or output paths.
- Restoring per-workload binaries (`bifrost_query_capacity`, `bifrost_client_queue`).
- Changing main's `bench:capacity`.
- Synthetic host load.
- Marking any unexecuted gate, geometry or parity command as passed.
- Absorbing implementation that TASK-001 or TASK-002 left missing. Return it to the owning task.

### Source-backed reuse

| Capability | Owner/symbol | Selected extension | Proof |
| --- | --- | --- | --- |
| REQ-026 — S workloads in one build | `capacity` dispatch (`main.rs`: `query-capacity`, `client-queue`); `mise.toml` `bench:bifrost:query-capacity`, `bench:bifrost:write-capacity`, `bench:bifrost:client-queue` | Already consolidated; verify only | S23 |
| AC-028 — no main trace except X1–X9 | `git diff` against S over Bifrost scope; TASK-002 P-028 | Consolidated from TASK-002; not rebuilt here | TASK-002-CLOSE-03 |
| REQ-027 — evidence closure | `evidence.md`, `dispositions.tsv` | Reconcile against revision 4; struck items are recorded as struck, not as passed | TASK-006-C03 |

## Requirement Authority

[spec.md](../spec.md) revision 4 is the requirement text: the Revision 3 rule, Benchmarks, AC-028, and Acceptance and evidence closure. This task does not copy that text.

## Approach

1. Verify that S's three workloads are callable by name from the `capacity` binary with S's inputs and report shape. Run its unit tests.
2. Confirm TASK-002's P-028 ledger is complete for the final candidate.
3. Reconcile `dispositions.tsv` and `evidence.md` against revision 4. Rows for the struck requirements are marked struck.
4. Prepare the review handoff with exact-S baselines and the six parity commands, still unexecuted.

## Proof Strategy (verification only)

This proof is verification-only. No behaviour changes in this task, so it uses no RED. If a check fails, the failure goes back to its owning task as a finding. It is not fixed here.

### S23 — S workloads callable by name from the one capacity build

**Proved behaviour.** Each S command below dispatches to S's workload in the `capacity` binary:
- `mise run bench:bifrost:query-capacity`, including `-- --fixture telemetry` and `-- --heavy`;
- `mise run bench:bifrost:write-capacity`, including `-- --fixture logs` and `-- --fixture telemetry`;
- `mise run bench:bifrost:client-queue`.

The workloads keep S's inputs, steps, assertions, metrics, report layout and output paths. Main's `mise run bench:capacity` stays the default mixed benchmark, beside them.

**Proof.**

```bash
# Build, plus the binary's own unit tests (29 on 2026-10-11).
mise exec -- cargo check --locked -p wyrd-testing --bin capacity
WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1 \
  mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity
# The workload code differs from S's per-workload binaries only by module path.
git diff -M cfe21dc9007d5d22d9ae4411be07fb5504b0cc05 -- crates/wyrd/wyrd-testing/src/bin/bifrost_query_capacity crates/wyrd/wyrd-testing/src/bin/bifrost_client_queue crates/wyrd/wyrd-testing/src/bin/capacity/query crates/wyrd/wyrd-testing/src/bin/capacity/client_queue
# Every command in the handoff names an existing mise task.
grep -o 'mise run bench:bifrost:[a-z-]*' changes/active/bifrost-variant/BENCHMARK-HANDOFF.md | sort -u
```

The six parity runs listed in the spec's Acceptance and evidence closure are executed only in the final change review, against exact-S baselines.

### P-028 — moved

The AC-028 ledger is TASK-002's P-028. This task only confirms it covers the final candidate.

## Acceptance Criteria

- **TASK-006-C01.** S23: `capacity` builds and its unit tests pass. The S workload diff against S changes only module paths. Every `bench:bifrost:*` command in the handoff resolves to a `mise.toml` task.
- **TASK-006-C02.** Moved to TASK-002-CLOSE-03 (2026-10-12). This task confirms the ledger covers the final candidate.
- **TASK-006-C03.** `dispositions.tsv` and `evidence.md` agree with revision 4. Rows for REQ-005, REQ-006, REQ-007's settlement model and REQ-024 are marked struck, and no struck item is reported as passed.
- **TASK-006-C04.** The review handoff names:
  - the immutable integrated candidate;
  - the exact-S baselines and the matched host, resources and settings;
  - the outstanding final runs: `mise run gate`, the two production-geometry lanes and the six parity commands.

  Only the independent final review can establish that the change is complete.
- **TASK-006-REVIEW.** A fresh independent task review accepts the checkpoint. An unmapped hunk or a missing proof is not a PASS.

## Write Set

- `changes/active/bifrost-variant-main-integration/{evidence.md,dispositions.tsv}` and the review handoff.
- Bench code only if S23 finds a divergence from S. Such a fix restores S's text exactly and is recorded.

## Verification and Evidence

Every command uses `WYRD_LOG=info,vala_bifrost_redux=debug RUST_LOG=info,vala_bifrost_redux=debug RUST_BACKTRACE=1`. Run the S23 commands.

If bench code changes, also run:
- `mise run fmt:check`
- `mise run lints`
- `mise run check:deps`
- `mise run check:workspace-hack`
- `mise run check:ci-selection`
- `git diff --check`

The final gate, geometry and parity runs belong to the change review.

## Material Stop Conditions

- A Bifrost hunk maps to no exception, and its owning task is already accepted. Report it as a regression against that task; do not relabel it.
- A handoff command no longer matches S's report shape. Report the divergence; do not invent a tolerance.
- AC-028's literal schema clause ("apart from X5 and X9") cannot cover the schema changes that X1 and X2 require. Record the mapping and leave the wording to the spec owner.

## Authority Links

- [Spec revision 4](../spec.md)
- [Decomposition](../decomposition.md)
- [Proof recipes](../proof-recipes.md)
- [Canonical evidence](../evidence.md)
- [Repository standards](../../../../AGENTS.md)
- [Bifrost authority](../../../../architecture/bifrost-design.md)
- [Benchmark handoff](../../bifrost-variant/BENCHMARK-HANDOFF.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
