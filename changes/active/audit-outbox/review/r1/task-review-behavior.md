# Behavior Review — audit outbox revision 1

## Immutable subject

- Base: `ce5c09ef3b559c35e65d6a3e73beebe0a69ae5bd`
- Candidate: `f451d52be01acba66bae003b8509c95086fc8558`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 1
- Original tasks: `01-publication-progress.md`, `02-one-outbox.md`, and `03-remove-audit-unavailable.md`
- Review result: **FAIL**

The candidate remained at the stated commit throughout this review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 / INV-004: one process outbox and no other production staging append | `boot/mod.rs:1021-1131` constructs one outbox and shares it with Gate, Oracle, peer security, and `AppState`; `state.rs:2208-2215` reuses it; `audit_staging.rs:70` makes the canonical append crate-private. Repository search finds production calls only from `audit_outbox.rs:299`; direct public helpers are `test-support` only. | Task-reported workspace check/lints plus source search. | PASS |
| REQ-002: drain waiting work, batch by tenant, one chain-head lock/commit per batch, bounded cross-tenant concurrency | `audit_outbox.rs:195-268` groups queued events by tenant, caps `JoinSet` work at four, and prevents two in-flight commits for one tenant; `audit_staging.rs:70-179` takes one chain-head lock and performs one batched insert/update. | `pg_audit_outbox::two_outboxes_commit_one_gap_free_chain_and_drain_on_shutdown`; `a_contended_tenant_does_not_delay_another_tenants_audit` (task-reported green). | PASS |
| REQ-003: request does not wait for or fail on audit commit; full/lost work is counted | `AuditOutbox::stage` is synchronous and enqueue-only (`audit_outbox.rs:111-126`); failed commits are logged/counted (`296-329`). Changed caller paths stage outside operation transactions. | Gate/start-run journey and Card/auth/admin/gateway failure tests cited in T02. Oracle-specific closure is missing; see BEH-003. Public docs still advertise audit-caused failures; see BEH-002. | FAIL |
| REQ-004 / AC-006: audit-unavailable contracts and documentation removed | Error variants, mappings, proto value, generated SDK constants, and route-specific stable codes are removed; the durable `AuditErrorCode::AuditUnavailable` history value remains intentionally. | Task-reported codegen, proto drift, OpenAPI, docs, and crate tests. However live served-OpenAPI annotations and Rust/API docs still promise audit-caused 500/503 responses; see BEH-002. | FAIL |
| REQ-005 / INV-001: decide first, stage the known verdict, then perform/refuse the effect | Shared server authorization stages denial or returns an allowed event (`audit/mod.rs:117-168`); changed routes stage the allowance before effects. Gate records the verdict before Scribe dispatch (`gate/mod.rs:509-525`); `start_run` stages through `VerificationService::stage_decision`. | Changed unit/integration/journey tests assert allowed and denied cardinality and non-blocking effects. | PASS |
| REQ-006 / AC-004: publisher progress is independent of append lock and retirement follows the watermark | Migration creates tenant-RLS `vala.audit_publication` and removes progress from `audit_chain_head`; freeze locks only the progress row (`audit_staging.rs:344-405`); settlement advances then deletes through the watermark (`456-487`). | `publication_proceeds_while_an_append_holds_the_chain_head`, competing/restart/stale-settlement cases, and server publication journeys (task-reported green). | PASS |
| REQ-007 / AC-007: shutdown stops intake, drains to the deadline, and reports remainder | `AuditOutbox::shutdown` cancels the writer and returns pending count (`audit_outbox.rs:128-137`); server calls it after request/Bifrost shutdown and logs nonzero remainder (`app/server.rs:876-883`). | `two_outboxes_commit_one_gap_free_chain_and_drain_on_shutdown` also proves a post-shutdown stage is refused. | PASS |
| INV-002: committed per-tenant chains stay gap-free and ordered | Batched append derives every row from one locked chain head (`audit_staging.rs:88-177`). | Two-outbox 1,000-event Postgres test verifies sequence and hash links. | PASS |
| INV-003: every batch commits under its own tenant binding | `commit_tenant` opens `ValaPostgres::tenant_conn(tenant)` for each dispatched tenant (`audit_outbox.rs:296-301`); publication table has forced tenant RLS. | Two-tenant contention and settlement-isolation tests (task-reported green). | PASS |
| AC-002: every named surface family succeeds and increments its surface counter when audit commit fails | Gate/start-run, Card, token grant, and admin paths have injected-failure tests with counter assertions. The cited Oracle peer test is healthy-path content/routing coverage, not an injected commit failure. | No Oracle query/peer injected audit-commit failure plus `surface="bifrost"` counter assertion was found; see BEH-003. | FAIL |
| AC-003: two replicas produce one gap-free chain | Two independently constructed outboxes concurrently stage 500 events each for one tenant. | `pg_audit_outbox::two_outboxes_commit_one_gap_free_chain_and_drain_on_shutdown`. | PASS |
| AC-005: the canonical capacity benchmark meets saturation and two-replica scale-out SLOs | No implementation evidence is needed beyond the candidate behavior, but the approved acceptance gate requires the canonical benchmark. | None of the three task records reports `mise run bench:capacity`; the command is not defined in this candidate's `mise.toml`. See BEH-004. | FAIL |
| Non-goal: no durable crash-safe process queue; hard-kill loss remains accepted and counted | The bounded queue is process memory and graceful shutdown drains it; no WAL/relay was introduced. | Source inspection. The accepted hard-kill/unflushed window is not treated as a defect. | PASS |
| Non-goals: event content/hash chain/retained history/single publisher and engine lineage remain unchanged | Existing event constructors and `AuditPublisher` remain the owners; append batching preserves the canonical hash encoding; no engine-mechanics audit path was introduced. | Schema/publication and existing engine suites reported green. | PASS |
| Governing repository principle: dependency slowness is retried rather than dropped | `commit_tenant` makes one acquire/append/commit attempt and records every event lost on any error (`audit_outbox.rs:296-308`). | No transient dependency outage/recovery test exists. See BEH-001. | FAIL |

## Proposed findings

### BEH-001 — VIOLATION: transient database failures drop whole audit batches instead of retrying

- **Violated obligation:** The user-approved repository principle says derived server-owned outboxes retry dependency slowness rather than drop work. The accepted eventual-consistency and abrupt-kill windows do not authorize loss during a recoverable dependency outage.
- **Exact location:** `crates/vala/vala-sql/src/audit_outbox.rs:292-308`.
- **Evidence:** `commit_tenant` performs one `tenant_conn` acquire, one append, and one commit. Any returned error immediately calls `record_commit_failure` for every event. `AuditOutboxWriter::settle` then decrements the entire batch from `pending` (`audit_outbox.rs:271-288`), so no state remains to retry after Postgres recovers. A pool acquire timeout, connection reset, or transient database restart therefore takes the same terminal-loss path as a deterministic rejected row.
- **Observable consequence:** During a temporary Postgres slowdown or outage, acknowledged operations continue, but their audit decisions are permanently absent even if the dependency recovers while the server remains alive. This is broader loss than the approved full-queue, shutdown-deadline, and hard-process-kill windows.
- **Required testable correction:** Keep transient acquire/connection/commit failures owned by the outbox and retry them with bounded backoff without holding a writer connection during the delay or blocking unrelated tenants. Preserve tenant order and the global pending bound. Deterministic non-retryable event failures, queue overflow, shutdown-deadline exhaustion, and hard-kill loss may remain counted terminal loss. Add a Postgres integration test that makes one tenant's append fail transiently, restores the dependency, and proves the same queued decisions commit once while another tenant continues to commit.

### BEH-002 — INCORRECT: live API documentation still advertises audit-caused request failures

- **Violated obligation:** REQ-003, REQ-004, AC-006, and T03's promised removal from docs/OpenAPI surfaces.
- **Exact location:** Representative live annotations and docs include `crates/wyrd/wyrd-server/src/components/platform/credentials.rs:61-80,120-139,186-208`, `components/platform/routes.rs:79,146,197,269,303,341`, `components/platform/identity.rs:195,300,359,425,525,592`, `components/auth/routes.rs:339-343`, and `components/gateway/routes.rs:160-165,1267,1392,1436,1469,1504,1545,1575,1613,1647`. SDK/server docs also retain the old contract, for example `crates/shared/wyrd-client/src/principals/handle.rs:132-135` and `crates/wyrd/wyrd-auth/src/callback.rs:65-73,165-173`.
- **Evidence:** The `utoipa` response descriptions still say a platform decision "could not be audited" and gateway responses still include "audit is unavailable" as a 503 cause. The platform credential Rustdoc says audit failure prevents minting, while `PlatformAuthorization::authorize` now stages non-blockingly before opening the operation transaction. T03's evidence at `03-remove-audit-unavailable.md:69` claims these served OpenAPI descriptions were removed, but the candidate source contradicts that claim.
- **Observable consequence:** Generated/served API documentation and SDK guidance tell clients to handle audit-driven 500/503 refusals that the implementation and approved contract prohibit. Maintainers are also told that effects and allowance audit still commit atomically when they no longer do.
- **Required testable correction:** Remove every live claim that an audit commit can fail, delay, or roll back a request, and update transactional comments to describe the outbox boundary. Strengthen the served OpenAPI contract test to reject audit-unavailable language and audit-caused error descriptions, not only removed stable code strings. Preserve legitimate descriptions of audit event content and retained-history decoding.

### BEH-003 — MISSING: AC-002 has no injected Oracle audit-commit failure proof

- **Violated obligation:** AC-002 requires an injected audit commit failure for each named family, including Oracle, with a successful request and a counter increment.
- **Exact location:** `changes/active/audit-outbox/spec.md:137-141`; the claimed Oracle evidence is `changes/active/audit-outbox/tasks/02-one-outbox.md:120`, while the cited test is `crates/wyrd/wyrd-server/src/oracle/peer_audit.rs:109-164`.
- **Evidence:** `oracle_peer_postgres_audit_routes_security_identity_and_detail` shuts down a healthy outbox and asserts two committed rows. It neither injects a staging failure nor observes `audit_outbox_commit_failures_total`. Repository search found injected failures for Gate/start-run, Cards, auth, verification, admin, platform, and gateway, but not an Oracle read decision or peer-security decision. The shared writer makes the intended behavior plausible, but that does not satisfy the explicit per-family acceptance proof.
- **Observable consequence:** The candidate has no acceptance evidence that an Oracle request remains successful and its `bifrost`-labelled failure is counted when the concrete Oracle-to-outbox composition encounters a commit failure.
- **Required testable correction:** Add the smallest Oracle-path test that installs an audit-staging failure for an Oracle operation, executes a real Oracle request/decision through the composed outbox, proves the query or applicable refusal retains its non-audit result, settles the outbox, and asserts the `bifrost` surface counter increments with no staging row.

### BEH-004 — MISSING: the mandatory capacity acceptance gate has no evidence

- **Violated obligation:** AC-005 requires the canonical `mise run bench:capacity` run to pass every judged saturation step and two-replica scale-out.
- **Exact location:** `changes/active/audit-outbox/spec.md:146-149`; all recorded command evidence in `tasks/01-publication-progress.md`, `02-one-outbox.md:169-183`, and `03-remove-audit-unavailable.md:82-94`.
- **Evidence:** None of the task records includes a capacity run or its artifact. In this immutable candidate, `mise.toml` defines narrower `bench:verification:capacity`, `bench:bifrost:ingest-capacity`, and `bench:bifrost:query-capacity` tasks, but not the exact approved `bench:capacity` command. Passing correctness suites cannot establish the stated throughput and two-replica scale-out outcome.
- **Observable consequence:** The change's motivating product outcome—removing audit-chain contention without regressing the canonical saturation contract—has not been accepted.
- **Required testable correction:** Run the approved canonical capacity lane from the integration state that defines it, preserve its result artifact, and record the judged-step and two-replica outcome against this cumulative implementation. A narrower or synthetic audit-only benchmark is not a substitute.

## Verification notes

- Inspected the complete `base..candidate` diff, the outbox writer and canonical append, publication migration/freeze/settle path, boot/state/shutdown ownership, Gate and Oracle adapters, representative server/auth call paths, public errors/proto/SDK removals, and the cited Postgres/server tests.
- Independently ran read-only source searches, `git diff --check base..candidate`, and candidate identity checks. Heavy repository lanes were not rerun; the review treats task command summaries as claims and cross-checks their named tests against source.
- The reported correctness lanes are broad and credible for compilation/regression coverage, but they do not close BEH-001, BEH-003, or AC-005. The reported OpenAPI lane did not catch BEH-002 because its assertions do not reject the remaining generic audit-failure descriptions.

## Overall

**FAIL.** The core one-outbox, batching, tenant isolation, publication-progress, non-blocking request, and graceful-shutdown mechanics are present. Acceptance is not complete because transient dependency failures are terminally dropped contrary to the governing retry principle, live API documentation still exposes the prohibited audit-failure contract, Oracle lacks the explicit injected-failure proof required by AC-002, and the mandatory capacity gate has no evidence.
