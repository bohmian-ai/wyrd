# Concurrency and lifecycle domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved specification: `changes/active/audit-outbox/spec.md`, revision 2
- Remediation input: `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`
- Prior concurrency hypothesis: `changes/active/audit-outbox/review/r1/domain-review-concurrency.md`

The repository has no `.codegraph/` directory. `HEAD` matched the candidate before review and before this report was written. This report covers only base-to-candidate regressions and closure of the r1 concurrency/lifecycle finding; it does not reopen unchanged behavior.

## Reviewed boundary

The review traced the revised generic outbox and audit composition end to end:

- `Outbox::stage`, `settle`, and `shutdown` admission and accounting;
- `OutboxWriter::run`, receive/dispatch ordering, one in-flight batch per tenant, retry-front insertion, exponential backoff, `JoinSet` completion, panic/cancellation, and deadline abandonment;
- `AuditSink::write`, stage-time event identity, canonical append deduplication, chain-head serialization, transaction commit, and unknown commit outcomes;
- publisher freeze/read/Scribe append/settlement, concurrent publisher reuse, watermark advance, and staged-row retirement;
- server boot ownership of the single `AuditOutbox`, publisher supervision, transport/Bifrost drain order, publisher cancellation, and final outbox shutdown;
- generic, Postgres, publication, and server-journey proofs for REQ-003/003a/007/008/009, INV-002, and AC-002/005/007/008/009.

Normal returned sink errors now remain pending, re-enter the front of the same tenant queue, back off from 50 ms to 5 s, and do not block a healthy tenant. The process composes one audit outbox and shares it with the audited surfaces. During graceful server shutdown, supervised publisher work stops before the outbox is drained last, after request and Bifrost producers have drained. Those r1 lifecycle defects are closed on their ordinary paths.

## Authority coverage

| Boundary | Governing authority inspected | Result |
|---|---|---|
| Retry, accepted loss, ordering, and generic ownership | Approved spec rev. 2 REQ-003, REQ-003a, REQ-007, REQ-008, REQ-009; INV-002; AC-002, AC-007, AC-008, AC-009 | Three findings below |
| Audit concurrency and integrity | `AGENTS.md` §§2, 5, 6, 11-12; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` “Audit integrity and privacy” | Unknown outcomes must not duplicate; loss is restricted to the two approved boundaries |
| Canonical audit and publication lifecycle | `architecture/wyrd-design.md` runtime identity/Bifrost boundary; `architecture/bifrost-design.md` “Read audit and terminal contract” and system invariants | Finding `CONC-R2-001` |
| Failure, cancellation, and recovery | `architecture/operations/reliability-and-recovery.md`; `architecture/operations/runbooks.md`; `architecture/references/domain/vala-architecture.md` | Findings `CONC-R2-001` and `CONC-R2-002` |
| Generic Rust async ownership | `architecture/references/languages/rust-core.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/maintainer-style.md` | Findings `CONC-R2-002` and `CONC-R2-003` |
| Review authority and regression boundary | `architecture/references/languages/spec-driven-development.md`; `.agents/skills/wyrd-task-review/SKILL.md`; r1 verdict and validated findings | Covered |

The controlling concurrency principle is also explicit in Bifrost's system invariants: structured cancellation stops admission, joins descendants, and reconciles uncertain effects; uncertainty retains identity and evidence until reconciliation. Audit-specific authority additionally requires a retry after an unknown commit outcome to stage a decision exactly once.

## Source coverage

| Area | Source and tests inspected | Assessment |
|---|---|---|
| Generic outbox | `crates/shared/wyrd-runtime/src/outbox.rs` in full, including all five unit tests | Returned-error retry and healthy/deadline shutdown are covered; task panic and concurrent shutdown admission are not |
| Audit sink and append | `crates/vala/vala-sql/src/audit_outbox.rs`; `queries/audit_staging.rs`; `20261003000001_audit_staging_event_id.sql` | Event identity exists only while its staging row survives |
| Audit SQL proofs | `crates/vala/vala-sql/tests/pg_audit_outbox.rs`; `pg_audit_staging.rs` | The AC-009 test repeats a known-successful write before retirement, not an unknown outcome across retirement |
| Publisher | `crates/wyrd/wyrd-server/src/audit/publication.rs`; publication SQL and server journeys | Publisher retirement is valid in isolation but can erase the outbox retry's only dedup evidence |
| Process lifecycle | `crates/wyrd/wyrd-server/src/boot/mod.rs`, `app/server.rs`, `state.rs`, and `wyrd-testing/src/server.rs` | One shared owner is composed; publisher stops before final outbox drain; normal serving still runs publisher and writer concurrently |
| Surface recovery | `crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs` and named server/auth/gateway proofs in the remediation evidence | AC-002 exercises definite trigger rejection and recovery, including Oracle, not an ambiguous commit |

## Scoped obligation assessment

| Obligation | Source and proof assessment | Result |
|---|---|---|
| REQ-003 | A returned sink error is requeued at the tenant front with bounded backoff; later same-tenant items remain behind it and other tenants dispatch independently. A spawned sink panic instead destroys the owned batch and releases it as lost. | FAIL — `CONC-R2-002` |
| REQ-003a | Deadline abandonment and late staging are counted. A sink-task panic loses accepted items while the process remains alive, outside either approved loss boundary. | FAIL — `CONC-R2-002` |
| REQ-007 | The server drains producers before its final outbox shutdown, and pre-shutdown work retries to the common deadline. The generic handle does not synchronously close admission when shutdown begins, so late producers can still enqueue and biased receive can postpone closure. | FAIL — `CONC-R2-003` |
| REQ-008 | The new SQL-free generic owner provides unbounded staging, grouping, one in-flight write per tenant, bounded cross-tenant concurrency, retry, metrics, settle, and deadline shutdown. Its panic and shutdown-admission transitions do not satisfy the generic lifecycle contract. | FAIL — `CONC-R2-002`, `CONC-R2-003` |
| REQ-009 | `(tenant,event_id)` uniqueness prevents a repeat while the original row remains in `vala.audit_staging`. Publication can retire that row before the outbox retries an ambiguously successful commit, after which the same event id is fresh again and receives a new sequence. | FAIL — `CONC-R2-001` |
| INV-002 | Returned-error retries stay ahead of later same-tenant work, chain-head locking serializes replicas, and committed rows remain gap-free. The retirement race creates a second gap-free chain entry for the same decision rather than a sequence gap. | PASS locally; exact-once still fails under REQ-009 |
| AC-002 | The real Gate/write/run-start/Oracle journey proves requests succeed under a definite staging rejection, failures are counted, and decisions commit once after recovery. | PASS for the specified definite-failure path; it does not close AC-009 |
| AC-005 | The remediation packet explicitly defers `mise run bench:capacity` and `mise run gate` to integration. This domain review has no integrated capacity artifact to assess. | NOT VERIFIED — approved integration deferral |
| AC-007 | Existing generic and Postgres tests prove recovery before the deadline and counted remainder at the deadline, but do not prove synchronous admission closure against a concurrent stager. | FAIL — `CONC-R2-003` |
| AC-008 | Five focused generic tests prove returned-error retry/order, tenant independence, 50,000-item admission, healthy recovery, deadline loss, and pending returning to zero. They omit sink panic and staging concurrent with shutdown. | FAIL — `CONC-R2-002`, `CONC-R2-003` |
| AC-009 | `rewriting_a_committed_batch_stages_each_event_once` performs a successful first `AuditSink::write` and immediately repeats it while both event IDs are still staged. It neither produces an unknown commit result nor permits an `AuditPublisher` to retire the first rows before retry. | FAIL — `CONC-R2-001` |

## Material findings

### CONC-R2-001 — publisher retirement erases the only unknown-commit dedup evidence

- **Classification:** INCORRECT
- **Violated obligation:** REQ-009 and AC-009 require a retry after an unknown audit commit outcome to produce no duplicate staged or retained event. Audit authority also requires uncertain effects to retain identity and evidence until reconciliation.
- **Exact locations:**
  - `crates/vala/vala-sql/src/queries/audit_staging.rs:111-129` checks event IDs only in `vala.audit_staging`;
  - `crates/vala/vala-sql/src/queries/audit_staging.rs:489-520` advances the watermark and deletes every staging row through it;
  - `crates/wyrd/wyrd-server/src/audit/publication.rs:272-285,422-438` publishes and retires concurrently with the serving outbox;
  - `crates/vala/vala-sql/tests/pg_audit_outbox.rs:185-215` repeats the batch only while its original rows remain staged.
- **Evidence:** `AuditSink::write` can return a commit error after Postgres committed. `OutboxWriter::finish` then retains the same `StagedAuditEvent` and waits at least 50 ms before retry. During normal serving, an `AuditPublisher` in this or another Scribe replica can freeze, publish, settle, and delete the committed row. The retry locks the unchanged chain head, finds no matching `event_id` in staging, assigns the same decision the next sequence and hash, and inserts it again. The publication watermark then treats that new sequence as new work, so retained history receives the authorization decision twice. A later event queued behind the retry stays ordered, which is why gap-free-chain assertions do not detect the duplicate.
- **Observable consequence:** one authorization decision can appear twice in the canonical chain and retained audit history after the precise unknown-commit recovery REQ-009 was added to prevent. This corrupts audit cardinality without producing a chain gap.
- **Proof deficiency:** the claimed AC-009 test models “known successful write followed by immediate repeat,” not “commit returned unknown, publisher retired the row, then retry.” Its timing removes the competing publisher named by the remediation's own residual-risk note.
- **Required testable correction:** Preserve the stage-time event identity across the complete uncertain-commit reconciliation window, including after concurrent publication retirement, so the canonical append can distinguish a retry from a new decision without changing per-tenant order, adding another writer/publisher/WAL, or retaining duplicate history. Add a deterministic Postgres/server test that makes the first audit transaction durable while reporting an unknown outcome, lets a real competing `AuditPublisher` durably publish and retire that row, then releases the outbox retry and proves one retained decision, no second staged row, no extra sequence, and the later same-tenant event behind it. The current authorities simultaneously require transient staging retirement, forbid another audit table/relay, and leave `event_id` out of retained content; selecting the durable reconciliation owner therefore needs an explicit architecture/spec decision rather than a timing delay or a wider test assertion.

### CONC-R2-002 — a sink-task panic drops accepted items while the process stays alive

- **Classification:** INCORRECT
- **Violated obligation:** REQ-003, REQ-003a, REQ-008, and AC-008 permit accepted-event loss only at abrupt process stop or an expired graceful-shutdown deadline. They require failed writes to remain at the tenant queue front and retry.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:288-297,321-374`.
- **Evidence:** `dispatch` moves the only owned `Vec<Item>` into a `JoinSet` task. If `OutboxSink::write` panics, `join_next_with_id` yields a `JoinError`; `finish` retains only `(tenant,count)` in `in_flight`, so it cannot restore the items. It increments the lost counter, decrements `pending`, and continues the writer. The process did not stop, no shutdown deadline elapsed, and later work for that tenant can now proceed past the missing batch. Join cancellation at deadline is separately covered by `abandon_remaining`; the unauthorised path is the explicitly handled panic branch.
- **Observable consequence:** an invariant panic in `AuditSink` or the future Eval sink silently creates an audit/evaluation gap while the server remains available and `settle` can return zero. Per-tenant ordering cannot recover an item whose ownership was destroyed.
- **Proof deficiency:** every `MemorySink` test returns `Err` or hangs; none panics. Green returned-error retry tests never enter the `JoinError` branch.
- **Required testable correction:** Keep recoverable ownership of each in-flight batch across task unwind and route a panicked attempt through the same front-of-tenant retry/backoff lifecycle, or make the owning process actually stop instead of reporting a live, drained outbox. Add a generic sink that panics once, stage a later same-tenant item, and prove the original batch remains pending and then commits once ahead of the later item; also prove shutdown deadline remains the only terminal loss boundary if the panic repeats.

### CONC-R2-003 — shutdown does not close staging admission when it begins

- **Classification:** INCORRECT
- **Violated obligation:** REQ-007 and the generic shutdown part of REQ-008/AC-007/AC-008 require graceful shutdown to stop accepting new events, then drain the already accepted set to its deadline.
- **Exact location:** `crates/shared/wyrd-runtime/src/outbox.rs:136-148,175-190,234-269`.
- **Evidence:** `shutdown` only cancels `stop`; the sender remains usable until the writer later selects the stop branch and calls `requests.close()`. In the biased select, ready write completions and `requests.recv_many` precede `stop.cancelled`. A stage racing after shutdown begins can therefore send successfully and increment pending, and a continuously ready receive branch can postpone admission closure until deadline abandonment. The server currently reduces this risk by draining request and Bifrost producers before `audit_outbox.shutdown`, but the generic public owner itself does not provide the specified transition and its next approved consumer has independent producers.
- **Observable consequence:** work produced after shutdown begins can be committed as though accepted before shutdown, or can enlarge the set reported lost at the deadline. Shutdown duration and loss accounting are therefore functions of late producers rather than the fenced pre-shutdown backlog.
- **Proof deficiency:** `shutdown_flushes_items_that_recover_before_the_deadline` stages only before shutdown, and the post-shutdown assertion stages only after `shutdown(...).await` has completed. Neither test stages after shutdown starts but before the receiver closes.
- **Required testable correction:** Establish a synchronous, one-way admission fence at shutdown invocation. Items linearized before the fence must drain/retry; `stage` calls linearized after it must never enter the writer queue and must follow the documented late-stage accounting. Add a deterministic test with an in-flight blocked sink, begin shutdown, stage another item after the fence, release the original write, and prove only the pre-fence item reaches the sink, pending reaches zero, and shutdown does not wait on the late producer.

## Verification limits

- I ran `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/^outbox::/)'`; all 5 focused tests passed. Their omissions are described above.
- I did not rerun the recorded Postgres, full server, TypeScript, documentation, codegen, capacity, or aggregate lanes. Source inspection confirms the named SQL and journey tests exist and match the remediation evidence, but those lanes do not create the missing interleavings.
- A production-shaped unknown Postgres commit outcome is difficult to induce without a connection/proxy fault seam. A credible AC-009 proof must control “commit durable, client observes failure” and publisher retirement explicitly; simply invoking `write` twice after a known success is not equivalent.
- The publisher's own concurrent freeze/replay/settle tests remain valid for publication ambiguity. They do not prove writer retry idempotency after the publisher legally deletes the writer's dedup row.
- `settle` correctly avoids a lost `Notify` wake by enabling the waiter before reading `pending`, but it is a global-idle wait rather than a snapshot fence: concurrent staging may extend it to the deadline. No scoped acceptance criterion requires snapshot semantics, so this is not a finding.
- Server lifecycle ownership is otherwise coherent: production boot creates one outbox, `AppState` reuses it, supervised publisher cycles are cancelled before Bifrost and request producers finish draining, and the outbox is shut down last. The retirement defect remains reachable during ordinary serving and across replicas, not specifically during server teardown.
- `mise run bench:capacity` and `mise run gate` remain the remediation packet's explicit integration-stage obligations. Their absence here is recorded as a limit, not reopened as a new r2 regression finding.

## Overall result

**FAIL**

The ordinary returned-error retry, per-tenant ordering, cross-tenant progress, and server-owned drain path close the r1 drop-on-error defect. The candidate does not satisfy the complete concurrency/lifecycle contract: concurrent retirement can defeat unknown-commit deduplication, a sink-task panic drops accepted work outside the approved loss boundaries, and shutdown admission remains open for a race after shutdown begins.
