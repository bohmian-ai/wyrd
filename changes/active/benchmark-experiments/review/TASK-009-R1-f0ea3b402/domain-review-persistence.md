# Persistence, concurrency and tenant isolation domain review

Result: **FAIL** — one material authority-update finding; no additional runtime persistence/tenancy defect established.

## Immutable subject and boundary

Reviewed TASK-009 revision 2 against approved `spec.md` revision 24, full range `6f38db16c524417301ffc4a5dc0b478daf997ed1..f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`. HEAD remained that candidate at review completion. No prior acceptance evidence was carried forward. CodeGraph was consulted before source discovery.

This domain covers preparation → input publication/ACK → tenant admission transaction → durable queue/runner input loading, including schedule cursor, group activity generation, dedupe, rollback, frozen authority/identity and source references. Inspected cumulative source/diff and relevant implementation/test commits (`b6d856818`, `20995edb2`, `4621e16ad`, `929a1d911`, `6e1b0a674`, `33f202555`, `4778abf71`). Delivery/benchmark validity and the shared outbox timer diagnosis belong to the other assigned reviews.

## Authority and source coverage

| Boundary | Governing authority | Source traced |
|---|---|---|
| Durable scheduling acceptance | REQ-035/036; AC-015/016/017; TASK-009 scheduling and migration sections | `verifier_executions.rs` prepared/admission types, `prepare_scheduled`, `settle_due`, `accept_scheduled`, `prepare_group`, `accept_grouped`; `scheduler.rs` preparation, in-flight ownership and stage |
| Tenant/transaction ownership | Root AGENTS; SQL AGENTS connections/transactions/queries; security posture Postgres tenant binding | `TenantConn`; `SubmissionSink::write`; SQL insert/existing/cursor/group statements; existing composite-FK/RLS migrations |
| Frozen group/observation inputs | REQ-035/036/037; Wyrd group source semantics; Bifrost execution_inputs table owner | `GroupPoller::poll/record`, `PinnedGroup`, schedule selector construction, `ResultPayloadBuilder::inputs`, runner `dispatch/recorded/supplied` and InputResolver consumption |
| Successful ACK boundary | REQ-035 and task shared handoff | `ScribeWrite::ExecutionInputs`, private tuple construction of `Acknowledged`, Scribe successful/lost-frame filtering, shared admission writer |
| Recovery and duplicate work | REQ-036/037; accepted orphan/prequeue selection limitation | Existing unique occurrence/snapshot indexes, replay lookup, conditional source transitions; runner settlement/lease fence; scheduler reservations and RAII in-flight claims |
| Migration/rollout | SQL AGENTS immutable migration rule; task limits/migration | Additive `20261028000000_verification_group_generation.sql`; original pending-table RLS/FKs; `RECORD_GROUP_ACTIVITY_SQL`; task explicit stop/drain-old-process rollout |
| Authority reconciliation | Root authority rule; task owner-update obligation; REQ-038 | Changed Wyrd/Bifrost sections plus remaining normative group/queue publication descriptions identified below |

Read the repository authority map, spec-driven workflow, maintainer guide, relevant Rust guidance and the nested SQL rules. No other nested AGENTS exists in these SQL/server/Bifrost directories.

## Acceptance and adversarial evidence

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| No queued candidate or consumed source before its own successful input ACK | Scheduler rolls candidate preparation back before `stage`; `Acknowledged` constructor is confined to Scribe's successful input path, excluding lost items; writer requires `Acknowledged` | `queued_origins_require_their_own_input_ack`; `queued_origins_share_ack_gated_admission`; negative lost-input path | PASS |
| Queue acceptance and expected cursor move commit/roll back together | `accept_scheduled` insert/replay and `ADVANCE_EXPECTED_CURSOR_SQL` use caller's same TenantConn; writer owns one final commit | SQL `acknowledged_admission_preserves_scheduling_state`: rollback, concurrent contenders, replay, edited cursor | PASS |
| Delayed group ACK cannot erase newer or recreated activity | Pending migration defaults each new/existing row to a UUID; activity upsert changes it in the same operation; acceptance deletes only matching generation | Same SQL test: rollback, newer activity, delete/recreate, same-content contenders | PASS |
| Existing dedupe retains winning execution/input identity | Existing unique binding/window and binding/group/digest constraints arbitrate inserts; losers query original ID; no update replaces its archived input | SQL contention/replay and tenant/input-identity tests; group failure/retry test | PASS |
| Frozen binding/authority/Trigger/operators/attempt metadata survives handoff retry | Prepared admissions retain resolved target and ExecutionAuthority; accepted writes bind those fields without re-resolution; selectors and recorded_at live in the retained ResultRow | `failed_handoffs_retry_without_consuming_source_work` kills blocked SQL writer, edits live Trigger metadata, checks original execution IDs/Trigger and immutable snapshot | PASS |
| Multiple same-tenant origins share admission transaction; no held publication transaction | Shared writer uses one TenantConn for submitted batch and sorted background candidates; preparation drops its connection before Scribe staging | `queued_origins_share_batched_admission`; failed-handoff and SQL rollback proofs | PASS |
| Tenant identity remains authoritative | TenantConn/RLS applies to binding existence, dedupe lookups and source transitions; prepared foreign-tenant binding is invisible, so insert selects nothing and refusal moves no source | `queued_admission_preserves_tenant_and_input_identity` explicitly attempts both candidate origins under another tenant | PASS |
| Accepted retries use own archived input and existing authority/lease rules | Runner `recorded` addresses `run.lease.execution_id`, grants internal table reads serving accepted authority, loads schedule/group references; settled results remain behind existing applied-lease fence | Existing distributed polling, stale-lease and grouped/window journey tests inspected; no new alternate runner path | PASS |
| Existing storage and data survive migration | One new UUID column/default; no destructive SQL, payload store, reset or edited applied migration; original RLS/FKs remain | Static migration/source proof; no database executed in this review | PASS |
| Owning normative architecture describes the ACK-first order consistently | New paragraphs describe ACK-first admission, but older mandatory descriptions still prescribe enqueue/consume then selector publication | Direct document/source comparison; DP-001 below | FAIL |

Sibling/existing-owner tracing: `verification.rs::project_bindings` keeps immutable exact-Card binding targets on conflict; schedule writers are initial arm and queue-owned cursor transitions. Group activity has one existing SQL upsert, reused by `ScribeSink::schedule_groups`; generation is updated there rather than a second service. SQL convenience `enqueue_*`/`schedule_due` methods remain lower-level SQL owners, while production scheduler calls prepare/settle and the shared ACK-gated server writer calls accept. Submitted idempotency remains the existing authority-scoped key-holder/acceptance owner. Payloads remain in Bifrost and no new persistent staging mechanism was introduced.

Credible races checked: two prepared schedule contenders, old-cursor admission after a newer cursor, old group ACK after an activity update or delete/recreate, same snapshot contenders, cross-tenant admission, SQL rollback/commit uncertainty replay, and lost publication releasing in-flight ownership for re-preparation. PostgreSQL uniqueness and conditional mutations preserve these invariants; accepted limitations remain orphan inputs and changed pre-acceptance selection on process loss. UUID generation is ordinary non-reused activity identity within the approved design; no mixed-version writer guarantee is claimed.

## Proposed finding

### DP-001 — INCORRECT: owning architecture still requires the superseded commit-before-publication flow

**Obligation:** TASK-009 revision 2 explicitly requires updating Wyrd/Bifrost owner sections and conflicting old task instructions before behavior; REQ-038 requires retirement of superseded instructions. Root AGENTS requires resolving contradictions in the owning document, whose text remains authoritative over code.

**Locations:** `architecture/wyrd-design.md:1638` through the group-selection publication description at 1643, and `architecture/bifrost-design.md:1827` through 1830. Related Wyrd concurrent-activity wording at 1656–1659 should be reconciled as part of the same correction.

**Evidence:** Wyrd says the preparation transaction enqueues the ordinary execution and deletes the scheduling row, then stages the group input immediately after commit. Bifrost says group/schedule selectors are staged after their PostgreSQL commit and an early runner claim retries as evidence unavailable. In the same candidate the revised Wyrd paragraph at 1751–1754 and Bifrost table contract at 387–389 instead require input ACK before admission. Source implements the latter: scheduler candidate preparation drops its TenantConn, stages input plus admission metadata, and only successful Scribe processing constructs Acknowledged for `SubmissionSink::write`, which inserts and conditionally consumes source state together.

**Consequence/materiality:** A maintainer reading the owning group/storage contract is still instructed to implement the exact ordering race this task removes, including premature source consumption. This is a concrete contradiction in normative authority about the task's central persistence invariant, and the explicit owner-update acceptance obligation remains incomplete. It is not a demand for stronger runtime guarantees or optional wording polish.

**Smallest correction:** Replace the stale sequence clauses in these existing owner sections with the approved preparation-without-write → release transaction → publish/own-input ACK → shared tenant admission/conditional source transition order. Clarify that a delayed group candidate only consumes its selected generation; retain the current locked preparation query, no-execution refusal/unchanged-content behavior, existing source-visibility/retention limits, and ordinary unavailable-input retry handling. No new implementation owner, guard, protocol, or API is needed. Do not change immutable historical migrations to correct their old explanatory comments.

**Focused closure proof:** Re-read both complete owning paragraphs against scheduler staging, Scribe acknowledged construction, and SQL acceptance, checking that no remaining normative instruction tells a queued schedule/group candidate to commit execution/consume source before input publication. This is a bounded authority-text correction; runtime tests are already targeted at the correct ordering.

## Verification limits

Static review only: no Cargo, Postgres, benchmark, or publication operation ran. Named tests above were inspected in source; task-reported execution is supporting context, not an independently rerun result. No source edits, commits or external operations were made. The finding is confined to authority reconciliation; no unsupported stronger durability, source-retention or physical exactly-once promise is requested.
