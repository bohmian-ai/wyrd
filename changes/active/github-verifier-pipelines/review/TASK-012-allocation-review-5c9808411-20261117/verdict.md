# TASK-012-R1-allocation verdict

**FIX_REQUIRED** — seven independently validated findings remain: FIND-012-9 through FIND-012-15. The candidate cannot build the shipped server and does not yet complete the approved shared-pressure, CPU-dispatch and retained-custody integrations. Required final functional/L1 evidence is also incomplete.

## Immutable subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Base: `0ae00f4293f3d2dcf636bcbd88a419fe6245c6dd`.
Candidate: `5c980841140721cfaa1945fba1057675805ca77a`.
Reviewed actual diff: `git diff 0ae00f429...5c9808411`, 64 commits, 87 paths.
Read-only detached candidate: `/tmp/wyrd-task012-review-5c9808411`.

Both original and detached HEAD remained at the candidate, and detached source remained clean. Original uncommitted skill/workflow edits and Oracle field/initializer correction are excluded. The review writes reports/remediation only; it does not implement, commit, push, merge or deploy.

Behavioral authority: approved spec revision16, `spec-revisions/16-shared-workload-management.md` (including recorded maintainer decisions), current spec and finalized shared-workload design. Original task: `tasks/012-R1-allocation.md`, requiring REQ-019–024, AC-015–018 and preserved INV-001–007. Root/nested applicability, architecture authority, topic/testing/workflow references and navigation are recorded in scope.md and reports.

This is the **first review of the new allocation amendment**, not another unsuccessful historical precharge remediation round. Earlier TASK-012 findings/accepted behavior remain context only where unaffected. Revision16 expressly supersedes speculative envelope/copy requirements; no old precharge finding is reopened. It also expressly orders owner-document reconciliation, so fixing the stale resource paragraphs needs no new behavioral decision. No specification ambiguity or missing independent access requires BLOCKED/SPEC_REVISION_REQUIRED.

## Independent assignments and evidence

The orchestrator did not discover or validate findings. Fresh separate reviewers performed discovery and independent validation:

| Report | Concrete assignment |
|---|---|
| task-review-behavior.md | Primary approved requirement/scenario coverage; verification/worker/state/outbox; pinned Monty dependency; broad changed Rust/manifests, preserved limits and durable consumers |
| task-review-invariants.md | Runtime root custody, attribution, sampled pressure, targets/victims, normalized CPU service and lane lifetimes; no blocking core finding retained |
| system-review.md | Bifrost governor/Scribe/Oracle/Forge pressure and CPU integration, remote execution, IO and durable progress |
| domain-review-gateway.md | Gateway identity/permission/quota boundaries, buffers, capture, stream/participant lifetimes and CPU phases |
| findings-validation.md | Fresh independent source/caller/authority/materiality/correction validation of every proposal, all coverage and empty ledgers |

Separate repo/maintainer reviewers were unnecessary: primary owns broad rules/reuse, each concrete owner specialist covers its architecture. No changed SQL/migration/public SDK/UI projection requires a separate specialist; primary and validator checked those exclusions. All selected reports exist; validator confirmed selection and coverage. Validator-origin false-conclude-doc observation received separate primary source confirmation. No unresolved inter-reviewer conflict or unvalidated retained finding remains.

## Validated findings

The complete, single retained-finding records, exact locations, supported triggers, source evidence and corrections are in [findings-validation.md](findings-validation.md). This verdict references them without redefining approved behavior.

| ID | Classification | Validated boundary |
|---|---|---|
| FIND-012-9 | REGRESSION | Oracle resources field/initializer gated out of ordinary shipped-server build despite unconditional query-start access |
| FIND-012-10 | MISSING | Bifrost governor usage/cap not connected to workload targets; fresh Scribe work bypasses sampled pressure admission |
| FIND-012-11 | MISSING | Substantial gateway phases, remote follower/stage CPU entry and checkpoint preparation bypass shared lanes |
| FIND-012-12 | INCORRECT | Successful stream captures drop their charge before retained content/object persistence/outbox handoff |
| FIND-012-13 | INCORRECT | Buffered answer debit follows HTTP wrapper rather than last retained allocation; available JSON capacity omitted |
| FIND-012-14 | VIOLATION | Required owner resource documentation and changed-item rustdoc remain stale/missing |
| FIND-012-15 | EVIDENCE_GAP | Final candidate-bound functional and full clean compatible L1 qualification evidence absent |

## Reconciled acceptance matrix

PASS is limited to the source/assertion claim stated; it does not waive candidate-final execution or the failed production integrations.

| Existing requirement/scenario | Implementation and assertion/execution evidence | Result |
|---|---|---|
| REQ-019/023; AC-015; scenario1 core custody | Existing root ledger, new attribution split/merge/shared-owner/last-drop assertions; workspace/spill confirmed-deletion regressions recorded in task | PASS for core mechanism; handoff exceptions below |
| REQ-019/020; AC-015/016; scenario2 consumption | Real worker independent telemetry/reuse/stale/cancelled-exit assertions and exact pinned fork; raw/encoded media overlap; checkpoint actual prepared-row/outbox charge | PASS for consumption; checkpoint CPU fails below |
| REQ-020/021; AC-016; scenario3 | Runtime max-observation, .95/1 thresholds, inclusive .90 twice, cap/tenant sharing, reclaim/victim/exclusion assertions; production Bifrost bridge missing | FAIL / FIND-012-10 |
| REQ-019/022; AC-017; scenario4 core scheduler | Logical-time running-service/tenant-floor/rebase assertions; withdrawal, lane-free admission/IO and nested work assertions | PASS for core mechanism |
| REQ-022/024; AC-017; scenario5 dispatch | Local Monty/Oracle/parallel sort/repartition/real spill/Forge/Scribe assertions; three production CPU boundaries missing and default Oracle fails compilation | FAIL / FIND-012-9/11 |
| REQ-019–024; AC-018; scenario6 connected flow | Real combined-server attribution, pressure cancellation/Busy deferral and critical-pressure result-drain assertions; Bifrost bridge and gateway handoffs incomplete | FAIL / FIND-012-10/11/12/13 |
| REQ-024; AC-018; scenario7 | Partial/intermediate functional proof; failed release lint/build, missing immutable final result record and clean compatible L1 results | FAIL / FIND-012-9/15 |
| INV-001–007; preserved limits/durability/non-goals | Traced exact identity, lease/ACK/spend/host settlement, local limits/fork semantics, quotas/permissions, pools/spill/WAL/fence and mandatory drain; no changed wire/schema/SQL/UI/SDK projection | PASS for inspected preserved boundaries, subject to retained integration/custody failures |
| Required owner/changed-item documentation | Approved amendment explicitly requires reconciliation; obsolete architecture and conclude contracts plus bounded new-item omissions remain | FAIL / FIND-012-14 |

## Verification limits and process history

See verification.md for commands, provenance and cutoff. Fresh immutable-range `git diff --check` passed. No fresh Cargo was launched while the existing external sequential runner occupied the shared target. Existing lints output E0609 matches immutable source; successful feature-unified tests cannot prove the default shipped binary.

External logs show fmt, sandbox, redux/server integration, gateway native (17 Rust +8 Python) and server journey (62 tests) passing, but use the mutable original worktree containing the excluded Oracle correction. At cutoff Oracle journey was running; Scribe/Forge journeys and three exact regressions had no final result captured. None is silently counted as clean-candidate proof. Candidate task records and inspected assertions establish local claims, not final qualification.

The mandatory wait audit is covered by the independent reports: new process-local memory/CPU waits are sanctioned; Scribe staging/fsync and Oracle spill disk waits release lanes while preserving order; gateway provider/backpressure/storage and checkpoint outbox/ACK waits must stay outside CPU phases. No new SQL/Vault/object-store wait is introduced by these integrations. Correcting pressure admission must preserve accepted bounded drain.

Benchmarks were **not run, by user instruction**. REQ-024/AC-018 full clean L1 qualification remains open; no smoke/dirty substitute or performance benefit is claimed.

Behavioral acceptance is **FIX_REQUIRED**. Historical TDD compliance is separately **VIOLATED**: scenarios2–6 disclose production-first/mutation-only additions, alongside genuine early RED in some core cycles. Preserve those records. Do not manufacture RED, undo correct behavior or require production churn to repair history.

## Bounded handoff

[TASK-012-R1-allocation-R1-integration-and-qualification.md](TASK-012-R1-allocation-R1-integration-and-qualification.md) packages only these validated corrections and proof requirements. Review the successor against this candidate and check each retained closure plus affected seams. Preserve accepted manager-core behavior and non-goals; do not restart old precharge discovery. No old remediation-round limit applies to this new approved amendment. Implementation and subsequent review remain caller-owned.
