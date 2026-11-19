# TASK-001 cumulative and R3 review subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow
Original base: 0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb
Prior candidate: cd905f4cd929785bda25696326d58b4a4ae88dee
Candidate: d89356985566ab3492699682942438c25c8322fd
Remediation diff: git diff cd905f4cd d89356985
Cumulative context: git diff 0e8a31b14...d89356985

Original task: changes/active/verifier-review-pipelines/tasks/TASK-001-bifrost-execution-state.md
Remediation: ../task-001-postgres-cd905f4cd/TASK-001-R3-complete-execution-state-remediation.md
Authorities: current spec.md, execution-state-and-durability.md and relevant architecture sections, with explicit user decisions overriding conflicting historical requirements. New TASK007 wait/approval work is excluded. Architecture and packet are intentionally uncommitted; hashes are recorded alongside this subject. No source or authority edits authorized.

This is a full acceptance conclusion for TASK001 with R3 closure, not automatic whole-repository rediscovery. Prior task-001-bifrost-cd905f4cd reports and task-001-postgres-cd905f4cd reports are available for unchanged acceptance evidence. Reinspect affected contracts from the new diff and gaps in previous coverage. Prior findings16,17,19,21,22,23 require closure; 18 withdrawn,20 stronger handoff superseded by accepted rare old-worker overlap;24/25 subsumed by rejected SQL-head removal. IDs1–25 reserved. Human-directed USER-PG-001 and USER-DURABILITY-001 remain separate.

Approved limits: no per-execution Postgres state/head/ownership authority; retain ordinary queue, auth/Card/File coordination and generic Bifrost batch/catalog metadata. Direct successful final return does not wait for final Bifrost acceptance. Retain consequential checkpoint/publication boundaries. Rare late-old-worker overlap accepted; no new distributed fencing demanded. External calls can repeat after an ambiguous settlement; no universal exactly-once or invented provider idempotency support. No new retention policy, journal or suspension feature required.

## Common navigation map
- Engine checkpoint producer vala-verify -> verification/state.rs RunState/Held/Pending/StateScribe -> scribe_outbox.rs ScribeSink -> existing Bifrost Scribe shards/replay and vala-sql batch commits. Hosted blocks/evidence/publications consume checkpoints.
- verification/inputs.rs accepted recovery -> generic batch fences/live and published source coverage -> canonical execution_events snapshots. Unavailable owner must not become authoritative absence or stale recovery.
- components/verification/service.rs direct/submission/error/cancel -> verification/runner.rs attempt/lease/result reconciliation -> service/inspection.rs retained execution/result/content reads and authorization.
- wyrd-spec verification/state.rs identity/lifecycle -> canonical Bifrost reads after queue/Card deletion. Query verifier_executions remains coordination owner, not canonical error authority.
- Tests: pg_durable_verifier_work, pg_verification_runtime, pg_verifier_executions; wyrd-testing Scribe lifecycle; vala-sql pg_scribe_batch_commits; state/outbox units and prior SDK journey.
This is a starting map; expand against actual source and callers. CodeGraph before code search. Read AGENTS/nested rules, spec-driven-development, maintainer-style and applicable narrow authorities. Source immutable, reports only; no Cargo runs by reviewers (request narrow checks through root). Do not read other new discovery reports or implementation completion prose. Task recorded evidence is available as claims, not proof.

## Specialist selection before discovery
| Role | Material affected obligation and source basis | Report |
|---|---|---|
| closure discovery (ponytail-rev) | All R3 obligations/prior finding closure plus cumulative original acceptance gaps; service/state/runner and new proofs | task-review-closure.md |
| invariant-rev | Shared memory revisions, terminal producers, coalesced checkpoint admission and recovery source coverage changed materially | task-review-invariants.md |
| repo-rev | 42-file cross-tier Rust/SQL/proto/generated TS/docs delta; mandatory rustdoc, migrations, capability and generation boundaries | standards-review.md |
| maintainer-rev | Removal of SQL head and new recovery/inspection ownership materially changes complete workflows; compare existing owners and duplication | maintainer-review.md |
| system-rev | Local WAL/owner outage, restart, terminal failure/cancel and final-attempt recovery materially changed | system-review.md |
| domain-data | Batch-fence recovery coverage, SQL-head retirement, migration and dedup/replay persistence changes | domain-review-data.md |
| domain-security | Retained reads now authorize immutable verifier identity without live Cards/queue; cancellation and tenant boundaries affected | domain-review-security.md |

No fresh comprehensive first-round behavior team: original acceptance context exists and closure discovery owns gaps. No separate concurrency specialist: no new concurrency policy; invariant reviewer explicitly covers changed short-lock/coalescing/preparation/cancellation paths with prior domain-concurrency evidence available. No provider/OIDC/UI/native-lifetime specialist: unchanged adapters/credential profiles/UI/native lifetimes; closure reviewer checks required provider evidence without adding capability. Fresh independent final ponytail validator required after reports; verify applicability/coverage and every retained claim; new validator-only blocker requires separate source validation.

Each report needs explicit obligation/implementation/verification matrix, PASS/FAIL/BLOCKED, exact source-local findings with obligation, reachable producer-to-consumer path, material consequence, minimum correction boundary and focused proof. Reject optional improvements/unapproved machinery. Carry forward evidence explicitly; no claims of tests run here unless recorded by root. Read full functions relevant to correction and sibling writers. Preserve old IDs for same cause, propose local IDs for new defects. No intended verdict.

## Discovery completion and validation handoff

All seven selected independent discovery reports are complete. Root grouped proposals in claim-comparison.md and handed every report, prior context, authority and verification to a fresh final ponytail-rev. No intended verdict was supplied. No discovery follow-up was needed at handoff: claims converged on assigned owners; the validator may request focused source validation for a new blocker or unresolved correction decision. Source candidate and authority hashes still match establishment.

## Focused follow-up selection

During independent validation, a material correction-boundary uncertainty remained for FIND17: immediate cancellation/revocation commits versus first durable Bifrost terminal append, process death before postcommit staging, and pre-checkpoint immutable identity after source deletion. A fresh followup-rev is selected to trace actual existing retry/delivery/queue/runner owners and decide whether a minimal extension is within approved R3 or an unresolved material custody decision. Exact report followup-review.md; no intended verdict or other discovery conclusions supplied. This is not renewed stale-worker fencing discovery. Final validator must inspect its evidence before issuing the ledger.
