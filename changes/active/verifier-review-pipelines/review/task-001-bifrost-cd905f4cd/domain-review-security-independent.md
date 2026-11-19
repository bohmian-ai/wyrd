# Independent domain review: security

**FAIL**. Immutable subject: `0e8a31b14d336fadf62e1d8525a7fa030ab6c4eb...cd905f4cd929785bda25696326d58b4a4ae88dee`. HEAD was checked and equals the candidate. This is fresh discovery: no other report or reviewer conclusion was consumed. No source was changed.

## Boundary and authority

Reviewed execution admission/authority, dedicated inspection and publication/File reads, narrow internal Bifrost authority, tenant/RLS state heads, cancellation/current lease authority at checkpoint acceptance, peer checkpoint transport and nonblocking audit. Used CodeGraph before source searches, then read cumulative changed source and relevant unchanged callers.

Authorities read: root and SQL AGENTS; task-review skill; spec-driven-development; agent_docs index/Rust/maintainer guidance; TESTING; approved spec revision 18, TASK-001 including Revision 1, durability supplement revision 2; security-posture runtime identity/execution reads/peer trust/RLS/audit; Wyrd design Verifier execution and inspection; Bifrost durability/visibility. The architecture expressly requires cancellation/expiry enforcement at acceptance and reads independent of source Cards/leftover queue rows.

## Source coverage and obligations

| Obligation / boundary | Source evidence | Verification evidence / result |
| --- | --- | --- |
| Exact Verifier current caller grant; SYSTEM grants no caller authority | `service.rs::authorize_run/get_execution/records`; `inspection.rs::require_verifier/authorized`; `runner.rs::dispatch` uses recorded authority via `Caller::accepted` | Static caller trace and existing runtime proof inventory: PASS after identity resolves; resolution gap below |
| Tenant separation on state and File custody | `RunState::claim/accepted`; SQL `scribe_state_heads` TenantConn signatures, tenant-leading key, FORCE RLS USING/WITH CHECK; `published_object` verifies evidence tenant prefix, digest and length | SQL/custody tests inventoried; PASS on inspected source |
| Exact private publication association, no caller path/global digest lookup | `AcceptedPublication.objects`; `inspection::select/file`; `hosted/publications::accept/published_object`; `TenantEvidence::published` and workspace mounts | Existing publication/mount tests plus source: PASS |
| Current cancellation/lease authority at acceptance | `StateScribe::confirmed/write`, `RunState::durable`, `HeldLease::confirm`, queue renew/cancel/cancel_principal/fence_principals, state-head advance and Scribe fence/replay | Existing cancellation test cancels before next checkpoint; FAIL SEC-I-01 |
| Recorded identity supports direct and historical inspection/source deletion | `inspection::authorized/accepted`, `service::get_execution/direct_execution/card`, `InputResolver::direct/state/result`, Card get/delete SQL | TASK scenario 6 demands this; FAIL SEC-I-02 |
| Private peer checkpoint trust | `build_peer_grpc` mounts capture service on mutual TLS; capture parser retains same fence fields; Scribe ingress requires fence presence exactly on state-fenced builtin table; outbox frame/peer serialization | Peer adapter tests and static route trace: PASS within approved trusted cluster-process model |
| Audit stays nonblocking | `audit::authorize` stages on process outbox; `authorize_run` does likewise; receipt specialization in `ScribeSink::tally` concerns Object/ExecutionState rather than Audit | Existing outbox units and source: PASS |

Reviewed producer-to-consumer paths include direct fresh-state creation and queued state recovery, accepted-state fixed query, public exact producer selection and File download/local content, state advance to WAL/batch fence and replay, and cancellation writers. Full changed scope of this specialist is covered; unrelated provider/OIDC and quota are excluded.

## Material proposals

### SEC-I-01 — INCORRECT: staged checkpoints can advance after cancellation or lease expiry

**Authority:** DUR-009, REQ-057/INV-018, task preserved cancellation semantics and Revision 1 item 7. Wyrd design requires cancellation, expiry and deadline fences at acceptance.

**Locations:** `verification/state.rs::StateScribe::confirmed/write` and `RunState::durable`; `verification/runner.rs::state` lines 485–489; `vala-sql/src/queries/scribe_state_heads.rs::advance` lines 148–161; `wyrd-sql/src/queries/verifier_executions.rs::CANCEL_SQL/CANCEL_PRINCIPAL_SQL` lines 612–629.

**Reachable scenario:** a queued worker confirms its lease, freezes a settlement/publication and stages its checkpoint. Hold Scribe delivery, or let a transient refusal delay the outbox. A separate authorized cancellation commits queue status `cancelled` and clears lease expiry. Resume delivery before original execution deadline. Cancellation changes no Bifrost state-head writer. SQL `advance` checks only current head writer, exactly-next revision and `offered.not_after`; runner sets that bound to `run.deadline_at`, not current lease expiry. Therefore Scribe accepts the staged write. A lease expiring before delayed acceptance with no replacement has the same defect. The outbox retains and submits staged writes independently of waiter cancellation (`frames/deliver/tally`); dropping the runner does not retract them.

**Consequence/materiality:** recoverable progress and accepted publication visibility change after committed cancellation or expired queue authority. A waiter can also receive success before renewal notices loss. Takeover correctly replaces a writer but does not cover cancellation or expiry when there is no replacement. This is a required integrity/authority boundary, not a demand for exactly-once external calls.

**Smallest safe correction:** retain the existing queue lease owner and generic Bifrost acceptance owner, but establish an authoritative ordering between queue cancellation/current lease validity and checkpoint acceptance. Acceptance must not rely solely on the earlier process-side confirmation. Reuse existing queue fencing/cancellation and state-head/batch-fence mechanisms; do not add another scheduler or state mirror. Another pre-submit confirmation, faster polling, or cancelling the waiter cannot close a delayed/retrying immutable outbox write. Private implementation mechanics belong to these existing owners.

**Focused proof:** hold an actual queued checkpoint after initial lease confirmation but before durable acceptance; commit cancel, release delivery and assert no new accepted revision/publication or success receipt. Repeat after lease expiry without replacement. Exercise principal cancellation through the existing cancellation owner. Preserve accepted-before-cancel and takeover/replay behavior. Existing `pg_durable_verifier_work::cancelled_work_keeps_checkpoints_and_fences_stale_writes` cancels first, then calls `begin`, so it covers a different ordering.

### SEC-I-02 — MISSING: inspection still relies on queue rows and undeleted source Cards

**Authority:** TASK-001 scenario 6 explicitly includes source Card deletion; REQ-056; security execution-read contract; Wyrd design lines 1925–1928 require recorded immutable identities without source Card retrieval or leftover queue rows. Direct verify now participates in common checkpoint/publication lifecycle.

**Locations:** `components/verification/service/inspection.rs::authorized` lines 251–267 and `accepted` lines 295–312; `service.rs::get_execution` lines 274–300 and `direct_execution` lines 336–349; direct `RunState::fresh` producer and `StateRow` recorded identities.

**Reachable scenarios:** (1) perform successful direct verify with a step, then request its invocation page. It persists invocation state and intentionally creates no queue row. `accepted` invokes `authorized`, which requires `queue.execution_status` and returns not-found before reading accepted state. Publication listing/selection/download/content share this gate. (2) complete direct or queued work and delete an otherwise unreferenced source subject through supported Card deletion. Status uses `Self::card` to build its result; `cards/get.rs::SELECT_BY_UID` excludes deleted Cards and the delete owner sets that status. Queued status loses its full result; direct status becomes not-found although canonical records remain. The second failure occurs even with a retained queued row.

**Consequence/materiality:** legitimate exact-Verifier readers cannot inspect retained evidence/full results across explicitly required supported flows. Permission checks are sound once identity resolves, but current identity resolution does not implement the retained contract. These paths share the missing recorded-metadata resolution owner.

**Smallest safe correction:** reuse `InputResolver`/accepted-state fixed queries and `records` narrow SYSTEM capability to resolve immutable execution/Verifier/subject metadata, then authorize the requesting caller on that recorded exact Verifier before exposing content. Queue state may supply current coordination status without defining execution existence. Retain exact references needed by existing result responses in canonical execution data instead of fetching visible Cards. Apply this resolution to status and inspection authorization. Do not insert direct runs into the queue, grant cards:read, add tombstones or invent an archive API.

**Focused proof:** direct verify a nonempty step plan and read its accepted invocation page through the public Execution handle; exercise existing backend publication read. Delete subject Card and assert unchanged full result/exact references for direct and queued execution. A different-Verifier scoped caller and another tenant must remain denied/not-found. Use existing fixtures, without a new harness.

## Limits and result

No Cargo commands, source edits, or live reproduction were run by this reviewer. At read time root `verification.md` recorded 25/25 state/outbox units and 1/1 engine restoration unit passing, with integration work in progress. These do not exercise the failing delayed-acceptance or recorded-identity paths. Implementation evidence was treated as test inventory, not proof of those gaps. No unrelated token/OIDC/provider audit, quota review or whole-repository security sweep was performed. Peer review assumes the approved mutually authenticated trusted cluster-process model.

**FAIL**, with SEC-I-01 and SEC-I-02 proposed for independent validation. No optional hardening proposals.
