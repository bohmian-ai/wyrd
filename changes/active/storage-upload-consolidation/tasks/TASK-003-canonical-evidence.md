---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-storage-upload-consolidation
spec_revision: 2
requirements: [REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, INV-001, INV-002, INV-004, INV-005, AC-004, AC-005, AC-006, AC-007]
depends_on: [TASK-001, TASK-002]
---

# Canonical evidence resolution and readback

## Outcome and Value

Verification consistently resolves File and ArtifactRef objects through the
shared inventory, reuses required metadata within an execution, and reconstructs
execution evidence and judgments from their canonical tables.

For example, a submitted execution can be resumed and its judgment read back
with the original File declaration. Inventory supplies any returned object
metadata. Reading that judgment does not download the File again or hash its
bytes.

TASK-002 already makes File upload-to-verification work. This task closes
repeated lookup, workspace/admission, queued/group/scheduled and readback
behavior without replacing the benchmark branch's input/result architecture.

## What Changes and What Must Stay True

Keep distinct authorities: storage inventory for object facts, execution inputs
for declarations/source selectors and accepted activation context, execution
outputs for detailed result provenance, results for payload-free verdict
summaries, and Postgres for Card and execution coordination. No input/output
payload returns to the Postgres queue and no second evidence map is persisted.

Keep `InputResolver` as the shared execution/group/readback assembler.
`TenantEvidence` remains the bounded byte reader. File reuse/resolution stays
tenant- and origin-scoped. ArtifactRef retains exact Card identity, relative
path selection and Card-read authorization. Resolve required object metadata
once and reuse it for admission, byte reads and workspace selection; retain
existing authorization decisions and audit boundaries.

Delayed or unreadable inventory is bounded retryable readiness, not a missing
object verdict or deletion permission. Known deletion and invalid bytes remain
terminal under existing rules. Queued retries and caller deadlines stay bounded.
Status/judgment queries perform no new payload IO. A File inventory resolution
adds no application-level Postgres work; existing Card authority, target-cache,
queue and Bifrost catalog/fence SQL are unchanged.

Inventory lookups add query cost. Measure affected upload-completion, File and
ArtifactRef verification costs using existing experiment owners, without new
latency guarantees or relaxed budgets. No queue redesign, global cache,
durable repair, new outbox dependency, retention work, or migration.

## Approach

1. Trace admission, hosted evidence/workspace, queued runner, group/scheduled resolution and readback through their existing owners after the first two tasks.
2. Reuse required inventory metadata for each execution while preserving Card authorization and existing byte validation.
3. Preserve canonical input/output/summary assembly and remove any remaining competing payload persistence or fallback.
4. Close bounded readiness, known-deletion, corruption, cross-tenant and exact-Card cases.
5. Verify focused Rust/HTTP/MCP and language readback journeys, measure affected paths, and reconcile the narrow verification authority slices.

## Ordered Implementation Scenarios

### Scenario 1 — One inventory resolution serves an execution

**Behavior.** Verify File and exact ArtifactRef inputs, including relative path selection and workspace use. Admission and byte reading use the same resolved metadata; repeat use inside the execution does not query inventory again. File resolution adds no application-level Postgres lookup/write. An unauthorized Card or foreign tenant fails under existing authority before object access.

**RED.** Add planned [V1](#v1), using supported server test instrumentation for inventory/SQL counts and the real SDK/server for behavior. Existing independently repeated Artifact metadata lookup should fail the reuse assertion. Keep the successful File lifecycle from TASK-002 as regression evidence, not a manufactured RED.

**GREEN.** Share the required resolved metadata through existing execution-owned state and bounded reader/workspace owners. Preserve Card authorization and verify size/digest/format at byte read. Rerun V1 and TASK-002's affected journey.

**REFACTOR.** Remove repeated lookup and alternate assembly paths, without a new long-lived cache or changed permission/audit boundaries.

### Scenario 2 — Readiness never becomes guessed evidence

**Behavior.** Delay or fail inventory visibility for direct and queued execution. Direct work returns the typed bounded readiness failure; queued work follows existing retry bounds and cannot wait forever. An object known deleted retains terminal behavior, corrupt bytes fail digest/length checks, and a format mismatch cannot produce a pass. No case guesses a digest-derived path or reads a SQL fallback.

**RED.** Add [V2](#v2) with deterministic existing fault controls. Assert the selected exact catalog error and queued terminal/retry outcomes, with no sleeps or indefinite polling. Failures must prove a behavioral gap, not missing service setup.

**GREEN.** Use the shared inventory outcome and map readiness through existing execution error/retry ownership. Retain terminal validation/deletion semantics. Rerun V1–V2.

**REFACTOR.** Unify equivalent readiness handling inside the existing owners without hiding terminal failures or adding a repair worker.

### Scenario 3 — Canonical readback does not fetch object bytes

**Behavior.** Read direct, submitted, grouped and scheduled execution/results. Declarations/selectors come from execution inputs, detail from execution outputs, summary/verdict from results. IDs and source selectors reconstruct the expected response without a copied evidence payload. Status and judgment reads still work without newly downloading or hashing object bytes; unavailable required metadata is readiness, never a fabricated response.

**RED.** Add [V3](#v3) plus focused mirrored SDK readback [V4](#v4)–[V6](#v6) and MCP [V7](#v7). Block payload reads after the execution finishes using an existing controlled test seam, and assert readback performs no payload IO. Where the benchmark already satisfies canonical ownership, record verification-only evidence rather than force code churn.

**GREEN.** Extend InputResolver and existing service/runner/group projections only where needed. Keep accepted context and payload-free Postgres coordination intact. Rerun V1–V7 and targeted existing group/schedule regressions.

**REFACTOR.** Delete competing copies and duplicate assembly; retain the existing canonical tables and result/execution identity joins.


## Acceptance Criteria

| Obligations | Required proof |
|---|---|
| REQ-010–011, REQ-014–015; INV-002, INV-004; AC-004, AC-007 | V1: correct tenant File/Artifact bytes and workspace, exact Card authorization, one required metadata resolution, no added File application SQL. |
| REQ-013; AC-006 | V2: delayed and unavailable inventory is bounded retryable; deletion, length/digest and format failures retain terminal behavior; no fallback or path inference. |
| REQ-009, REQ-012, REQ-016; INV-001, INV-005; AC-005 | V3–V7: direct/submitted/group/schedule evidence and results reconstruct from canonical owners; no extra persisted payload and no new byte IO on status/judgment reads. |
| Spec scope/costs and acceptance performance paragraph | Same-method measurements against the combined baseline expose completion and inventory lookup cost; no unsupported zero-cost claim, weakened budget or unrelated optimization. |

## Implementation Reference

- `wyrd-server/src/verification/inputs.rs::InputResolver::{record,evidence,
  slots,result,direct}` already joins canonical input, output and summary tables
  by execution/result identity; `evidence` resolves group selectors recursively.
  Reuse this complete assembly workflow. Status readback calls it from
  `components/verification/service.rs::VerificationControl::{get_execution,
  direct_execution}`; queued resolution reaches it from
  `verification/runner.rs`, and groups from `verification/groups.rs`.
- `verification/results.rs::ResultPayload` and the Bifrost
  `tables/verification/{execution_inputs,execution_outputs,results}.rs`
  distinguish stored declarations, detailed output, and summary. Preserve their
  ownership and queued activation context. No replacement result store.
- `verification/hosted/evidence.rs::TenantEvidence::{read,artifact,
  artifact_file,stored}` owns bytes and citations; `artifact_file` calls
  authorized Card reading and listing. `hosted/workspace.rs::Workspaces::open`
  stages those same inputs. TASK-001 replaces the Card inventory underneath;
  TASK-002 replaces File location inference. The remaining gap is execution-wide
  metadata reuse and complete readiness/readback behavior.
- Current `components/verification/service.rs::VerificationControl::evidence`
  trusts object length; TASK-002 replaces it. Do not reintroduce that behavior or
  create another preparation flow here. The baseline fitter inventory reader
  belongs to TASK-001 and is a regression dependency.
- Existing tests in server `pg_verification_routes.rs` cover submitted replay,
  exact grants, direct execution and tenant isolation; `pg_hosted_artifacts.rs`
  covers Artifact evidence/code; feature-gated `pg_verification_groups.rs`
  and `pg_verification_runtime.rs` supply controlled runtime/group faults.
  Reuse their real dependency setup rather than mock away inventory.
- SDK stories `verifier_building_blocks` and their existing submitted/custom
  readback tests, plus MCP `verification.rs::pg_tests`, are the public
  projection owners. Add only missing behavioral assertions and preserve
  cross-language story alignment. SDKs remain projections over `wyrd-client`.
- `wyrd-testing/src/bin/capacity/verification/{mod,run,workload}.rs` owns the
  existing verification experiment. The inspected workload uses inline JSON,
  so its present direct timing alone cannot prove File/ArtifactRef inventory
  or upload-completion cost. Reuse existing measurement infrastructure and add
  only the affected workload observations needed by the approved spec; do not
  create a new benchmark system or claim unmeasured upload performance.

## Verification and Evidence

All added test names below are **planned**; confirm actual exact selectors after
adding them and before RED. Packages, target registration, and runner/setup
come from inspected manifests, test roots, and `mise.toml`. Tests requiring
controlled faults use the existing `test-support` server feature. Keep Cargo
commands sequential and use tracing to diagnose failures.

For language journeys, prepare `mise run build:server`, `mise run py:setup`,
`mise run ts:build` and `mise run ts:build:testing` as applicable, matching
the existing language tasks' dependencies.

### V1

Planned real-server inventory/authorization/workspace and query-work proof.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_routes::evidence_metadata_is_resolved_once)'"
```

### V2

Planned direct/queued readiness, deletion and corruption proof.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_routes::inventory_readiness_preserves_execution_bounds)'"
```

### V3

Planned canonical direct/submitted/group/schedule readback proof.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 -E 'test(=pg_verification_routes::canonical_readback_does_not_read_payload_bytes)'"
```

### V4

Planned Rust SDK canonical readback story.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::stored_evidence_readback_uses_canonical_records)'"
```

### V5

Planned matching Python SDK canonical readback.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/integration/test_verifier_building_blocks.py::test_stored_evidence_readback_uses_canonical_records'
```

### V6

Planned matching TypeScript SDK canonical readback.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts --testNamePattern "^stored evidence readback uses canonical records$"'
```

### V7

Planned MCP canonical readback projection.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=verification::pg_tests::stored_evidence_readback_uses_canonical_records)'"
```

### Owner checks and measurement

Run targeted regressions from the inspected hosted Artifact, runtime and group
modules with their current feature/runner setup. Run `mise run codegen:check`
when projected contracts change, `mise run check:tenant-isolation`,
`mise run check:deps`, `mise run fmt`, `mise run lints`,
and `git diff --check`. Apply the relevant Python/TypeScript lint, typing and
generated-declaration checks if their projections change. Already-correct
canonical storage is verification-only; do not force production edits.

Use `mise run bench:l1:verification` for the existing direct-path regression
measurement. Compare the same method/workload on the combined baseline and
candidate, retaining correctness and loss observations. Record affected File,
ArtifactRef and upload-completion measurements through that existing owner,
explicitly distinguishing measured intervals from byte transfer and publication.
Read `benchmarks/README.md` before running; a dirty/profiling/smoke record is
not a promoted performance baseline. No synthetic host load, new latency target
or relaxed budget is authorized.

### Integrated change review handoff

The three tasks collectively own every REQ-001–018, INV-001–005, and AC-001–008.
Carry individual evidence forward. On the immutable integrated candidate, change
review runs the complete affected SDK/HTTP/MCP/CLI journeys once, the applicable
storage backend proof, contract and boundary checks, and the cross-task
fresh-install upload → inventory → verification → readback → deletion journey.
This paragraph is a change-review obligation, not an instruction to rerun full
language suites in this task. Missing credentialed proof stays explicit.

## Material Stop Conditions

Stop for a specification revision if the solution needs copied evidence in the
queue/output/summary, changed source-selector semantics, another durable store,
unbounded readiness, new object-hashing guarantees, broader Card/tenant authority,
or changed audit or other producer ACK behavior. Do not replace InputResolver or
the byte reader merely to simplify a private call path. New private test seams
or equivalent focused commands are implementation adaptations; record them.
A performance shortfall is evidence to diagnose, not permission to expand scope
or weaken correctness.

## Authority Links

- [Approved specification, revision 2](../spec.md).
- [Inventory dependency](TASK-001-object-inventory.md).
- [Shared File dependency](TASK-002-shared-file-uploads.md).
- [Repository rules](../../../../AGENTS.md).
- [Spec-driven development](../../../spec-driven-development.md).
- [Implementation execution](../../../implementation-execution.md).
- [Testing](../../../../TESTING.md).
- [Wyrd Verifier authority](../../../../architecture/wyrd-design.md#verifier).
- [Bifrost durability and visibility](../../../../architecture/bifrost-design.md#durability-and-visibility).
- [Security posture](../../../../architecture/wyrd-security-posture.md).
- [Benchmark measurement rules](../../../../benchmarks/README.md).


## Implementation Evidence

Commits `a90fbe5a4`, `8a5a0785e`, `47ba4087c` on `canonical-evidence`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-010–011, REQ-014–015; INV-002, INV-004; AC-004, AC-007 | `verification/hosted/evidence.rs::TenantEvidence` keeps per-execution `files` and `artifacts` memos filled at `admit` and reused by `read`, `artifact_file`, `select`, workspace staging, and citations; each Artifact use still calls `cards/routes.rs::authorize_card_ref` (one audited `card:read` per use); `cards/service.rs::get_card_objects_by_ref` returns the hydrated Card with its inventory so hydration and selection share one read; `storage/inventory.rs::ObjectInventory::executed_by` takes the SYSTEM principal from `Caller.executed_by`, so File resolution makes no application Postgres lookup | V1 `pg_hosted_artifacts::evidence_metadata_is_resolved_once`: RED on the pre-change source (16 inventory reads), GREEN 2 reads, both audited under the executing principal (a fresh ID no registry names); no-grant and foreign-tenant Artifact reads terminal, foreign File not ready; regressions `pg_hosted_files::*`, `pg_hosted_artifacts::*`, `pg_verification_routes::*` (TASK-002 `file_reuse_requires_completed_file_inventory` included) | PASS |
| REQ-013; AC-006 | `TenantEvidence::admit` resolves every File through the inventory; no record or unreadable inventory is `BlockError::Retryable` → `EngineError::EvidenceUnavailable` via `admission`; existing mappings make it 503 (`run_direct`) or a bounded `Retry` (`runner.rs::engine_failure`) | V2 `pg_verification_runtime::inventory_readiness_preserves_execution_bounds`: RED on the pre-change source (completed after 1 attempt with no record); GREEN queued `retrying`/`evidence_unavailable` ×2 → `errored` after 3 attempts; a second run completes once the upload completes; direct `fail_next_query_after_schema` → 503 "input doc: the evidence inventory cannot be read now", then passes. Terminal digest, length, format: `pg_hosted_files::{evidence_refuses_tampered_body, evidence_reads_only_matching_files}`; wrong Artifact UID or path and stored bytes that no longer match the inventory stay terminal: `pg_hosted_artifacts::artifact_reads_refuse_before_use`; a deleted File record is planned again: `file_reuse_requires_completed_file_inventory` | PASS |
| REQ-009, REQ-012, REQ-016; INV-001, INV-005; AC-005 | No production change: `InputResolver` already reads only Bifrost tables (verification-only evidence) | V3 `pg_verification_routes::canonical_readback_does_not_read_payload_bytes` (stored bytes deleted; a new judgment cannot pass; status still returns the verdict and the original File declaration; passes on the pre-change source); V4 `wyrd-ai verifier_building_blocks::stored_evidence_readback_uses_canonical_records`; V5 Python `test_stored_evidence_readback_uses_canonical_records`; V6 TypeScript "stored evidence readback uses canonical records"; V7 `wyrd-mcp verification::pg_tests::stored_evidence_readback_uses_canonical_records`; group and schedule: `pg_verification_groups::*`, `pg_verification_runtime::verification_records_have_one_payload_owner` | PASS |
| Spec costs and performance paragraph | See Measurement | `bench:l1:verification`, candidate `47ba4087c` and baseline `3a99eb05f` | NOT MEASURED |

Measurement: both runs exited INVALID with `host.busy` before admitting any
workload. Non-benchmark processes (`mediaanalysisd`, a virtual machine) used
14.3% of 11 CPUs, above the 10% limit. No reading exists, so no cost claim is
made. The existing `direct-verify` workload uses inline JSON and cannot show
File or ArtifactRef inventory cost even on a quiet host. Change review must
rerun the comparison on a quiet host.

Checks: `fmt`, `lints`, `check:tenant-isolation`, `check:deps`,
`py:typecheck`, `ts:typecheck`, `ts:format:check`, `ts:lints`, ruff
format/check on the changed Python test, `git diff --check`. Server
integration: 37 passed across `pg_hosted_*`, `pg_verification_routes::*`,
`pg_verification_groups::*`, V2 and `verification_records_have_one_payload_owner`.

Selector adaptations: V1 lives in `pg_hosted_artifacts`, which owns the
registered-Artifact setup. V2 lives in `pg_verification_runtime`, because
`pg_verification_routes` claims no queue.

Known deletion of a File stays retryable, as TASK-002 left it. No production
path deletes a File, and REQ-013 forbids treating a missing row as deletion.
Queued exhaustion ends `errored`, as Artifact readiness already did. The
trace-only rule that the last attempt judges inconclusive is unchanged.

Architecture: `architecture/wyrd-design.md` Verifier evidence paragraph now
states per-execution metadata reuse and File readiness semantics. No wire
contract changed, so codegen does not apply.

Non-goals stayed excluded: no queue redesign, global cache, durable repair,
new outbox dependency, retention work, migration, or persisted evidence copy.

Diagnosis (first GREEN run of V1 reported 16 reads):
- **Symptom:** V1 failed with the RED count after the source was restored.
- **Evidence:** the restored files carried their pre-RED mtimes, which are
  older than the RED build's.
- **Cause:** `tar x` keeps the archived mtimes, so cargo's fingerprint kept the
  RED binary.
- **Fix site:** the source was touched and the run repeated (37/37 passed). No
  code change.
