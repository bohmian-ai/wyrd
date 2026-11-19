---
id: TASK-002A
kind: implementation
status: proposed
spec: SPEC-verifier-code-mode
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018]
depends_on: [TASK-001]
parent_task:
remediates: []
---

# Integrate resource-scaling before the canonical benchmark

## Outcome and Value

Bring the committed optimization work from `resource-scaling` into the current
`verifier-code-mode` worktree before TASK-003 starts. Verification must retain
code-mode execution and its completed correctness fixes while using the newer
shared evidence, borrowed comparison and single-owner step-output paths. The
canonical benchmark then measures this integrated runtime rather than the older
allocation behavior.

This is the maintainer-requested integration prerequisite, not an expansion of
the benchmark workload or latency target. Revision 3 retains revision 2's
runtime obligations; integration preserves those and the source change's
approved behavior. It does not establish new performance results.

TASK-001 and its current correctness remediations are the runtime prerequisite.
TASK-002 has already passed in this worktree and its shipped projections and
guidance must be preserved; it is not a technical prerequisite of this
integration or the benchmark. The requested order here follows the current
worktree state without making TASK-003 depend on other-language projection work.

## What Changes and What Must Stay True

### Immutable integration inputs

Planning inspected these local Git inputs on 2026-11-09:

| Input | Identity |
| --- | --- |
| Destination worktree | `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`, branch `change/verifier-code-mode`, HEAD `bcf61769e9c7b9b6632ed93521a695ff72dc36f4` |
| Source worktree | `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`, branch `change/resource-scaling`, HEAD `b1cf95f2ebfb8e730a0e8de5b81d9dff71ea011b` |
| Shared ancestor | `5234961d51798581a026834cfffd5efaadc226b4` |
| Source authority | `changes/active/resource-scaling/spec.md`, approved revision 5 at source HEAD, plus the committed variant-file-reads/heap-profiling authorities carried by that history |

Integrate the committed source history through this source commit, including
its manifest/lockfile and existing benchmark/tooling changes. Do not copy files
from the source filesystem or cherry-pick only the last optimization commits:
they depend on the preceding resource, Variant and Gateway work. Do not silently
substitute a moving branch tip. Before execution, recheck both identities; if
the source advances, record the new range and refresh overlap/proof selection
before importing it. Do not absorb new material scope under an old pin.

The source has untracked `changes/active/gateway-latency-capture/`; exclude it.
The destination has an uncommitted approved spec revision 3, TASK-003, preserved
spec revisions and review reports. Preserve all of them, including the TASK-002
output-visibility closure and TASK-001 cleanup remediation. Record the initial
tracked/untracked state and use caller-approved Git handling that cannot lose
or accidentally include that work. Do not reset, clean, discard, or blanket
stage either worktree.

Planning and task approval do not authorize a commit, merge commit, push,
rebase or history rewrite. The caller owns the actual Git integration action
and preservation of local changes. Complete reversible reconciliation and proof
before any final Git authorization; report precisely if the final action remains
caller-owned. Never claim ancestry integration when only an uncommitted merge
candidate has been verified.

### Preserved contracts and costs

- Preserve code-mode accepted authority, child permissions/audits, host outcomes,
  call/time limits, original citations, recipient-tagged records and typed /
  immediate / queued / retained readback. Engine-owned settlement must finish
  before retry or result; Agent cleanup must not drain settlement owned by its
  caller. Include all current TASK-001 remediations, not only its first fix.
- Preserve the optimization branch's shared original evidence through execution
  and staging, pinned metadata reads, borrowed selection/comparison, streamed
  hashing, capture policy and single retained step output. Do not restore deep
  copies merely to get the old engine shape to compile.
- Keep one existing owner per workflow. No second engine, evidence reader,
  dispatch path, serializer or resource configuration. Fixed outbox geometry,
  counted delivery lifetime, tenant isolation, Variant values/layout/leases,
  byte reservations and retry identities remain governed by source authority.
- Carry the source's exact dependency pins and optional profiling boundaries;
  no additional dependency, feature, format, migration or platform exception is
  authorized by conflict repair. Importing existing approved source changes is
  distinct from inventing a new one during reconciliation.
- Keep TASK-003's canonical workload, timing exclusions, objectives and L2
  preservation obligations intact. Its private paths/commands must be revalidated
  against the integrated benchmark harness before implementation; prior
  performance records retain their original fixture/build meanings.

The import is broad (149 source-side changed files at planning), with two
textual conflicts. Clean Git merges are not proof of semantic compatibility.
Compilation and focused runtime checks may be costly after native dependency
pins change. Full qualification and new performance claims remain outside this
task, as do unrelated optimization or cleanup work.

## Approach

1. Recheck pinned identities, source authority/evidence and destination local
   changes. Establish the caller-owned Git integration boundary before modifying
   the index or touching local work.
2. Reconcile the complete committed source history with the destination;
   inspect textual conflicts and every overlapping owner, including clean
   merges, against both sides' obligations.
3. Preserve the source evidence/capture/output ownership design while retaining
   code-mode settlement and complete call records on every result branch.
   Update mechanical callers/tests through existing owners only.
4. Run the focused proofs below sequentially. Diagnose merge-caused failures
   at their owner and retain required assertions, limits and negative paths.
5. Revalidate TASK-003 references/selectors against the integrated harness,
   record the exact integrated candidate and verification, then obtain
   `$wyrd-task-review` before TASK-003 implementation begins.

## Proof Strategy

The intended outcome composes already implemented, approved behavior; it adds
no new product behavior. Use preservation/regression proof, not a manufactured
RED or a new integration harness. Inspect both sides before resolving each
conflict. If a reachable merge regression needs an executable correction,
first extend the smallest existing affected test, record its behavioral RED
(after the candidate compiles), implement the minimal owner correction and
rerun it and the preceding relevant proofs. Compilation failure or a weakened
assertion is not behavioral RED.

Two focused proof groups establish the composition:

- Engine results retain record settlement and recipient/custom-data evidence
  while capture, shared evidence and step dependencies retain the optimization
  branch's ownership and exact values. Exercise successful and inconclusive
  Agent checks, failed steps, timeout-before-retry/result, hash/redact capture,
  borrowed/direct allocation and dependency output through existing tests.
- Real server/worker execution preserves metadata pinning, admitted original
  evidence and exact permissions; the Rust public code-mode journey still
  executes both step/check programs, denies and repairs a nested query, and
  reads honest retained evidence. Keep the source's direct-route no-enqueue and
  allocation checks. No duplicated SDK or fault matrix belongs here.

## Acceptance Criteria

| Obligation / owner | Required observation and proof |
| --- | --- |
| User-requested integration; source approved resource/copy/Variant/Gateway behavior | Exact source commit/range and resulting candidate recorded; complete history/content accounted for, with every conflict resolution explained. Imported units/boundaries below pass; no untracked source work imported. Report pending caller-owned Git action honestly. |
| Verifier REQ-001–018 / INV-001–006 / AC-001–018 preservation | Existing accepted task/review evidence remains available; changed seams are re-proven by engine/Agent regressions, hosted-artifact coverage and focused Rust public journey. Earlier proofs carry forward only where changed dependencies cannot invalidate them. No rollback of settlement, Busy/error semantics, records or citations. |
| Source resource-scaling REQ-013 preservation and existing evidence/capture owners | Direct allocation regression still passes; full/hash/redact exact results, dependent output and original input staging remain correct without restored disposable copies. Existing direct-route and engine checks below pass. |
| Repository and dependency boundaries | Locked build, format/lints, dependency/tenancy and generated checks pass; no weakened test or forbidden feature/dependency crossing. Regenerate from owners only. |
| TASK-003 handoff; benchmark REQ-019–021 / INV-007 / AC-019–022 remain TASK-003-owned | TASK-003 depends on this task; its inspected owner paths/commands match the merged harness, without changing approved workload/timing/objectives or claiming existing records qualify it. Both tasks use the integrated candidate identity. |
| Preservation of local work | Initial/final tracked and untracked inventory explains every change; unrelated source and destination work remains intact. No unsolicited commit/push/history rewrite. |

## Implementation Reference

Planning used `git merge-tree --write-tree --name-only` without changing either
worktree/index. It reported conflicts in engine.rs and pg_hosted_artifacts.rs;
inspect again at the actual pinned inputs. These are starting points, not a
file allowlist.

| Existing owner and inspected consumers | Integration gap and selected reuse |
| --- | --- |
| `vala-verify/src/blocks/engine.rs::VerificationEngine::judge`, `Judged::{checked,cited,report}`; `blocks_agent.rs` check/step/settlement tests | Code-mode adds ToolCallsOwner settlement and result records; source adds fallible capture-aware judgments and boxed result ownership. Compose both in the existing engine. Choosing either side wholesale loses an approved invariant. |
| `blocks/scope.rs::{Settled,Scope::resolve}` and engine action/result assembly | Source shares successful output as Arc<Value> and borrows selected bindings; destination adds records/cleanup. Preserve single output ownership through dependency resolution and final report. Existing dependency/capture tests cover consumers. |
| `hosted/blocks.rs::HostedBlocks::engine`, `hosted/evidence.rs::TenantEvidence::{new,read}` and EvidenceReader admission; direct service/results staging | Source replaces cloned original inputs with one engine backing and weak pinned reader view, including reads spanning metadata IO. Destination passes the same reader into hosted tools/workspaces. Reuse that reader/admission lifecycle; do not reintroduce evidence clones or detach authority. |
| `pg_hosted_artifacts.rs::Registered::{seeded_experiment,evidence}`, metadata/admission/authority tests | Textual conflict combines destination's seeded Experiment helper with source's evidence-constructor/admission changes. Retain the helper's callers and adapt them to the single pinned-evidence owner; preserve its permission/value assertions. |
| `hosted/sandbox.rs`, hosted workspace/tool tests, config.rs and scribe_outbox.rs | Git auto-merges overlapping changes. Verify retained code-mode limits/host-call settlement alongside source startup resources and counted outbox lifetime; clean merge alone does not establish this. |
| Source Gateway adapters/provider encoders, Bifrost Scribe/Oracle and resource owners, Cargo.toml/lockfile | These are imported complete owners with existing source tests/evidence, not replacement machinery to redesign here. Run affected unit/boundary proof and retain unresolved source qualification limits honestly. |
| `wyrd-testing/src/bin/capacity/verification/run.rs::{drive,call}` and capacity/release-server/profile owners; TASK-003 references | Source changes benchmark infrastructure/profiling and verification harness dependencies. Revalidate the planned timing/workload extension on that owner; this task imports no canonical workload or timing observer. |

Source symbols/evidence are readable with `git show <source>:<path>` before
integration; after integration use the owning source tree. Source TASK-005's
Direct verification proof section supplies the inspected allocation/route
regressions. Read source packet evidence as evidence, not as permission to
claim its outstanding performance/review obligations completed.

## Verification and Evidence

Run through `mise`, serializing Cargo-backed commands. Confirm selectors in
the integrated tree before execution; a zero-selected run is not proof. Use
repository Postgres and the real worker, with only model providers scripted.
Equivalent selector/setup repairs must preserve proof and be recorded.

### Engine and source ownership

```sh
mise exec -- cargo nextest run --locked -p vala-verify --lib
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks) | test(=blocks_agent::timed_out_attempt_settles_before_retry_or_result) | test(=blocks_agent::agent_check_settles_before_result)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::blocks::tests::direct_execution_reuses_evidence_without_disposable_payload_copies) | test(=verification::results::tests::outcome_writes_output_then_summary)'
```

The library lane covers source capture/dependency/concurrency logic. The exact
Agent selectors pin record retention and settled cleanup at the conflict. The
server selectors pin the source copy-removal and staging behavior.

### Real runtime and direct route

```sh
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E "test(/^pg_hosted_artifacts::/) | test(=pg_verification_routes::direct_execution_judges_inline_without_enqueueing) | test(=pg_verification_routes::direct_execution_refuses_nonconforming_evidence)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations)"'
```

Hosted-artifact coverage includes the conflicted helper's consumers and
metadata/admission/permission behavior. The Rust SDK journey checks the public
composition; do not reproduce the Python/TypeScript/MCP full suites here.

### Imported owners and boundaries

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-spec -p wyrd-skald-providers -p wyrd-gateway -p vala-bifrost-redux --lib
mise run fmt:check
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:check
mise exec -- git diff --check
```

These are affected owner unit/boundary lanes, not full user-journey suites.
Add only a focused existing integration slice when actual reconciliation
changes a source Gateway/Bifrost runtime path beyond its imported version;
record why the carried source proof is insufficient. Changes to public docs or
generated docs additionally run their existing docs check; no docs rewrite is
planned. No broad gate or performance qualification run belongs to this task.
TASK-003 owns canonical measurement and final change review owns full suites.

Record initial/final identities and local-change inventories, conflict decisions,
actual tests selected/results, retained source qualification limitations,
TASK-003 reference revalidation and any caller-owned Git action still pending.
A successful import is not evidence that latency/allocation targets were met.

## Material Stop Conditions

Stop for the owning specification if preserving both sides requires changing
public/persisted contracts, authority/audit/tenant semantics, resource ownership,
cleanup/concurrency guarantees, migration behavior or approved benchmark
boundaries. Do not resolve these by choosing one branch silently. A new
source range with additional material scope requires refreshed planning.
Routine private type/caller/borrow/lifetime reconciliation and equivalent tests
remain implementation-owned. Preserve local work first if Git cannot compose
it safely; identify the exact caller-owned action rather than discarding it.

## Authority Links

- [Approved verifier spec revision 3](../spec.md), retaining runtime revision 2
- [Next task](TASK-003-canonical-sync-verification-benchmark.md)
- Source `changes/active/resource-scaling/spec.md` revision 5 at the pinned commit (present locally after integration)
- [Workflow](../../../spec-driven-development.md) and [execution](../../../implementation-execution.md)
- [Testing](../../../../TESTING.md), [maintainer style](../../../../agent_docs/maintainer-style.md), [agent surfaces](../../../../agent_docs/agent-surfaces.md)
- [Verifier authority](../../../../architecture/wyrd-design.md#agents-tools-and-workspaces), [security](../../../../architecture/wyrd-security-posture.md), [Bifrost authority](../../../../architecture/bifrost-design.md)

## Implementation Evidence (2026-11-09)

### Identities and range

| Input | Identity |
| --- | --- |
| Destination HEAD | `bcf61769e9c7b9b6632ed93521a695ff72dc36f4` (unchanged) |
| Source | `change/resource-scaling` advanced from pinned `b1cf95f2e` to `8d25b13a1ededa23b9eb91973a2b489fe9224557`; the one added commit (`Record TASK-005 implementation evidence`) changes only `changes/active/resource-scaling/tasks/TASK-005-design-borrowed-gateway-validation.md` (+81). No new code scope; integrated through `8d25b13a1`. |
| Merge base | `5234961d51798581a026834cfffd5efaadc226b4`; 71 source commits, 149 files |
| Candidate | uncommitted merge (`MERGE_HEAD` = `8d25b13a1`), index tree `0d4bfae2cca6a4e53de23786401368ba6033838c` |

Git action taken: `git merge --no-ff --no-commit 8d25b13a1`, then `git add` of
the three reconciled code files only. **Pending, caller-owned:** the merge
commit. No commit, push, rebase or reset was performed.

### Local-work preservation

Initial: `M changes/active/verifier-code-mode/spec.md` plus 25 untracked packet
files (backed up to a session scratch tarball before the merge). None overlap
the source range. Final: `spec.md` still modified and unstaged; the same 25
untracked files present and unstaged. Source untracked
`changes/active/gateway-latency-capture/` was not imported.

### Conflict and reconciliation decisions

| File | Decision |
| --- | --- |
| `vala-verify/src/blocks/engine.rs` check arm | Kept destination ordering (execute → `ToolCallsOwner::settle` → classify; Busy still returns after settlement) with source's fallible `Judged::cited(.., capture)`; settled records attach via `Result::map`. |
| `engine.rs` `Judged::report` / `checked` | Source shape (`report` infallible after capture moved into judgments; `checked` returns `Result`) plus destination `tool_calls` fields. |
| `pg_hosted_artifacts.rs` | Kept destination `seeded_experiment` helper and its caller; took source's slot-free `evidence` doc. |
| `pg_hosted_artifacts.rs` `CodeAgent::start` (clean-merged, failed to compile) | Adapted to the single pinned-evidence owner: `evidence(tenant, perms, limits)` then `admit(&BTreeMap::new(), &pinned)`, the pattern the source's own callers use. |
| `hosted/blocks.rs` source allocation test (clean-merged, failed to compile) | Dropped `ActionOutput.tool_calls`: destination moved records to the engine-owned `ToolCallsOwner`. |

Step path (`Attempted::settle`, `Arc::new(output.output)` + engine-owned
`self.tool_calls`) auto-merged consistently. No dependency, feature, migration
or contract change beyond the imported source history.

### Verification

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Integration recorded, conflicts explained, no untracked source work | sections above | `git diff --name-only --diff-filter=U` empty; inventory compared | PASS |
| Verifier REQ/INV/AC preservation | engine.rs check arm, report | `vala-verify --lib` 56/56; 3 exact `blocks_agent` settlement/record selectors 3/3; Rust journey `verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations` 1/1 | PASS |
| Source REQ-013 / evidence/capture owners | source owners unchanged | `verification::hosted::blocks::pg_tests::direct_execution_reuses_evidence_without_disposable_payload_copies` 1/1; `verification::results::tests::outcome_writes_output_then_summary` 1/1; `pg_hosted_artifacts::*` + both `pg_verification_routes::direct_execution_*` 17/17 | PASS |
| Repository and dependency boundaries | — | `fmt:check`, `lints`, `check:deps`, `check:tenant-isolation`, `codegen:check`, `git diff --check` all exit 0; skald-spec/providers/gateway/bifrost-redux `--lib` 1147/1147 | PASS |
| TASK-003 handoff | TASK-003 `depends_on` includes TASK-002A | every referenced path, `GatewayCapture::start`, `verification/run.rs::{drive,call}`, and `bench:build`/`bench:l1:verification`/`bench:results` exist in the merged tree | PASS |
| Local work preserved | — | final inventory above | PASS |

Selector corrections (equivalent proof): the direct-allocation test lives in
`verification::hosted::blocks::pg_tests`, not `::tests`; the task selector
matched zero tests, so it was rerun under repository Postgres with the
corrected path.

Diagnosis — imported-owner `--lib` lane:

- **Symptom:** 19 `vala-bifrost-redux` tests failed when run bare.
- **Evidence:** `postgres fixture starts: ... test DB env unset (WYRD_TEST_DATABASE_ADMIN_URL); refusing to create or drop fixture database` (`catalog/bifrost_catalog.rs:2915`).
- **Cause:** those tests need the repository Postgres fixture; the task's command ran without it. No code defect.
- **Fix site:** command only; rerun via `scripts/postgres/with-test-postgres.sh` (as `test:bifrost` does): 1147/1147 pass. No test changed.

Limits: source qualification/performance obligations (resource-scaling
TASK-005 review, latency/allocation targets) remain open in the source packet
and are not claimed here. Non-goals (benchmark workload, timing observers,
full suites, docs) untouched.

Git action completed under TASK-002B (caller authorized commits): the staged
merge was committed unchanged as `95d201a4f` (parents `bcf61769e`,
`8d25b13a1`; tree `0d4bfae2c`, identical to the recorded index tree). The
packet's spec revision 4, tasks and review reports followed as `08c842785`.
Independent task review of TASK-002A has not been run.
