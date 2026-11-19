# Security and tenancy domain review

## Subject and boundary

Result: **FAIL**. Reviewed the immutable base `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb` to candidate `cd905f4cd929785bda25696326d58b4a4ae88dee` range. HEAD remained candidate when checked. This is the complete replacement TASK-001 security acceptance review, including Revision 1, under approved specification revision 18 and durability supplement revision 2; no earlier acceptance evidence is carried forward.

Boundary: execution/checkpoint authority, cancellation and principal revocation, exact-Verifier inspection authorization, narrow internal Bifrost reads, tenant SQL and storage isolation, peer capture trust, publication descriptor custody and audit. No provider/OIDC audit, unrelated dependencies, or source changes. CodeGraph was used before source navigation. Source-local proposals below require independent validation.

## Authority and source coverage

| Obligation / authority | Production path inspected | Proof inspected and assessment |
| --- | --- | --- |
| Root AGENTS; SQL AGENTS; security posture tenant isolation and Postgres binding | New `vala.scribe_state_heads` migration and query owner; RunState claim/read; Scribe batch-fence transaction | FORCE RLS, both tenant predicates, restricted operator policy and TenantConn signatures preserved. PASS. |
| Security posture dedicated execution reads; REQ-049/056; exact Verifier read/run scopes | `VerificationControl::require_verifier`, `authorized`, `accepted`, `get_execution`, `direct_execution`, `select`, `cancel` | Preflight denies callers with no applicable operation; exact UID authorization follows tenant identity resolution. Content and cancellation repeat authorization. PASS for grants; retained identity fails SEC-02. |
| Security posture narrow tokenless SYSTEM access; arbitrary public SQL retains caller grants | `VerificationControl::records`, `VerificationRuntime::reader`, `SystemReadAuthority::resolve/serving`, `InputResolver::{state,direct,result}` | Fixed SQL and typed execution/revision identifiers; grants scoped to registered table UIDs; principal/delegation serving is attribution only. No caller SQL is accepted by these methods. PASS. |
| REQ-014/057; DUR-009/010; current authority, stale writer rejection and cancellation fence | `HeldLease::confirm`; `RunState::durable`; `StateScribe::write`; `scribe_state_heads::{claim,advance}`; queue cancellation/principal cancellation; Scribe state acceptance/replay | Takeover writer/next-revision fence and replay suppression exist. Cancellation/lease authority is checked before, rather than at, acceptance: FAIL SEC-01. |
| Security posture accepted publication mounts/downloads; REQ-049/058; DUR-008 | `Publications::{path,complete,accept,receipt}`, `RunState::accept_publication`, `TenantEvidence::{publications,published}`, workspace publication mounting, inspection `file/download/content` | Shared File completion + inventory acknowledgement precede acceptance; exact private descriptors are retained, tenant path/digest/length checked, full producer selector compared. `publications_recover_exact_descriptors_from_run_state` and `publication_limits_survive_reclaim` source inspected. PASS for custody; authority gap is SEC-01. |
| Restricted peer capture and public-write exclusion | `grpc::serve_peer` router, `ScribeCapturePeerGrpc::{batch,ingest_capture}`, `ScribeBatch::into_frame`, Scribe ingress, Gate `authorize_write` and reserved-table list | Capture service mounted on mutual-TLS peer router with bounded decoding; allowlist and reserved-system-tenant restriction; public Gate excludes all four verification tables. State field required exactly for state-fenced destinations. PASS within existing trusted-cluster-member threat model. |
| Nonblocking audit; accepted authority never widens | `audit::authorize` call sites and `authorize_run/authorize_evidence`; `SystemReadAuthority::serving`; runner `Caller::accepted`; unchanged cancellation audit boundary | Decisions stage on existing Scribe outbox; checkpoint mechanics add no artificial permission check or audit gate. Current caller authority decides inspection; historical accepted authority only supplies execution/provenance. PASS. |

Read the applicable security posture sections (SYSTEM and accepted authority, dedicated inspection, peer trust, tenant isolation), specification REQ-013/014/049/056/057, TASK-001 scenarios 5/6 and security-preservation constraints, supplement DUR-008/009/010, spec-driven-development and maintainer guide. Source scope includes unchanged consumers where the task explicitly requires their continuity.

## Security Audit

### Critical

None proposed.

### High

- **SEC-01 — INCORRECT / REGRESSION: queued checkpoint acceptance can succeed after acknowledged cancellation or principal revocation.** Locations: `crates/wyrd/wyrd-server/src/verification/state.rs:660` and `:674`; `crates/vala/vala-sql/src/queries/scribe_state_heads.rs:138`; sibling authority writers `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:612` and `:621`.

  Governing obligation: REQ-014 requires checkpoint/final writes under current execution authority and queued lease; DUR-009 forbids unauthorized/stale accepted progress; security posture requires current authority during checkpoint acceptance and cancels unfinished queued work in the principal-revocation commit.

  Reachable scenario: a running queued execution confirms its lease through `HeldLease::confirm`, freezes an invocation/publication/settlement revision, and waits in the shared outbox/Scribe path. An administrator now cancels the execution, or revokes its submitting principal, and that transaction commits and returns. Both cancellation statements change only the queue row; neither fences the state head. Release the delayed frame before the original execution deadline. `advance` still sees the same head writer and next revision, and accepts it; `not_after` is the original execution deadline, not the current queue lease/status. `RunState::durable` releases its waiter after this acceptance without another authority check. The shared outbox can continue processing an already-staged frame even if periodic renewal subsequently cancels the runner. Thus cancelled/revoked work can acquire new accepted progress/publications after the revocation fence acknowledged success. A delayed invocation-start acknowledgement can also enable downstream dispatch before the periodic lost-lease signal arrives. Existing `begin` SQL, deleted in this range, locked and checked the live execution row inside the write transaction.

  Impact: principal revocation and acknowledged cancellation cease to be a reliable boundary for accepted execution effects; retained state can disclose newly accepted output produced by work whose authority has ended. This is an administrative boundary violation, not an attacker-selected arbitrary-SQL path.

  Smallest correction boundary: preserve the existing queue lease owner and generic keyed-state acceptance owner, but order every queued state acceptance against current queue cancellation, revocation and lease expiry at its authoritative commit boundary. Reuse the existing state-head/lease fence and committed owner handoff; a second detached confirmation, periodic renewal, post-ACK check, or shorter timeout does not close the race or remove already-accepted unauthorized state. Keep direct execution semantics and nonblocking audit unchanged. No new queue payload store or permission engine is needed.

  Focused closure proof: hold a real state frame after confirmation but before its acceptance; commit public cancel and principal revocation separately; release the frame and assert refusal, no accepted revision/publication, and no dependent invocation. Include expired lease without takeover, and verify restart/WAL replay cannot resurrect refused state. The current `cancelled_work_keeps_checkpoints_and_fences_stale_writes` cancels **before** starting the next `begin` and therefore cannot prove this ordering. The existing revocation journey similarly checks queue outcome after a held engine rather than a frozen pending state frame.

### Medium

- **SEC-02 — MISSING: retained execution identity and result inspection still depend on non-deleted Card rows.** Location: `crates/wyrd/wyrd-server/src/components/verification/service.rs:264` and `:326`, through `Self::card` at `:1145`; producer/readback gap in `verification/state.rs::StateRow`, `wyrd-spec::verification::state::ExecutionState`, and `verification/inputs.rs::DirectResult`.

  Governing obligation: security posture dedicated-read contract explicitly states retained identity/full metadata need neither an execution row nor fetching a deleted Card; TASK-001 scenario 6 explicitly requires source Card deletion proof; REQ-056 preserves public inspection and DUR-008 preserves retained output inspection. This unchanged consumer is explicitly within task acceptance, rather than unrelated historical debt.

  Failing supported path: complete a direct or queued verification and make all Bifrost records visible. Delete its Verifier or subject Card, then inspect using a newly issued caller retaining the exact recorded `verifier:read` grant. `get_card_by_uid` filters `status != 'deleted'` (`wyrd-sql/src/queries/cards/get.rs:17`). Queued `get_execution` consequently returns status with `result: None`; `direct_execution` returns None and the endpoint returns execution-not-found. Existing stored result bytes and granted read authority did not disappear. `StateRow` retains UIDs but its checkpoint uses an empty verifier version, and the accepted state has no full exact Card refs to replace the registry lookup.

  Impact: normal capability retirement makes retained assurance evidence unreadable or falsely absent despite valid evidence authorization. No privilege escalation is claimed.

  Smallest correction boundary: retain the exact response identity metadata with accepted execution/result records and project it through the existing `InputResolver` owner for both direct and queued readback. Reuse existing Bifrost correlation/identity fields where complete; retain only missing immutable reference metadata. Authorize against recorded exact Verifier UID as today. Do not broaden public Card reads, require cards:read, or fetch deleted Card specifications as a substitute for retained identity. Preserve existing execution/status and public response shapes.

  Focused closure proof: for a completed direct and queued execution, delete the subject and Verifier separately and assert an authorized exact-Verifier reader gets the same execution ID, full result and exact refs; an unrelated Verifier-scoped caller remains denied and a foreign tenant remains not-found. Current `terminal_selection_and_inspection_use_run_state` inspects invocation/publication pages but does not delete either Card or check this result path.

### Low / Defense In Depth

No optional hardening proposals.

### Positive Controls

- New SQL state storage forces tenant RLS; authority comes from TenantConn, not caller-selected tenant SQL.
- Fixed internal Bifrost reads use narrow table-UID grants without widening arbitrary public queries.
- Exact completed File descriptors survive recovery; downloads validate the tenant evidence namespace, declared digest and size and reauthorize the current reader.
- Public ingest cannot fabricate execution state; trusted peer capture remains restricted and transport bounded.
- Superseded writer and next-revision checks plus unfenced state-WAL suppression protect takeover/replay independently of the cancellation defect.

## Verification limits

Static review only; no Cargo commands, source edits, new tests or exploit execution. Task evidence was treated as claimed verification, and relevant test source was inspected; this report does not assert those tests ran during review. Broad OIDC/provider, external dependency advisory, and cross-platform execution audits were excluded. Only this assigned report was written.

The two proposals above are material task gaps. No additional speculative security finding is retained. Runtime race reproduction and deleted-Card regression proof remain necessary to close them.

## Independence disclosure

After independently tracing `RunState::durable`, `StateScribe::write`, `HeldLease::confirm`, `scribe_state_heads::advance`, and both cancellation SQL statements, I sent the concurrency reviewer the suspected authority race for coordination. That reviewer replied that it independently found the same cause. This exchanged discovery conclusions before the orchestrator reconciliation stage and departed from the required reviewer-isolation process. SEC-01's source tracing and diagnosis preceded the message; the report's evidence is from my own inspected source. SEC-02 was sent only to the orchestrator. Neither this agreement nor another review report was used as proof; I read no other reviewer report. A fresh validator must independently establish both proposals.
