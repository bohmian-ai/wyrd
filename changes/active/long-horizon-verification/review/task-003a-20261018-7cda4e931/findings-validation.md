# Independent findings validation

**Result: FIX_REQUIRED.** Six deduplicated, bounded findings remain. No new product, concurrency, recovery or persistent-data decision is required by the corrections below. Human-directed cleanup is recorded separately and has no retrospective FIND IDs.

Subject: base `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`, candidate `7cda4e93159ab328de2a74068d5e1978a2590818`. HEAD remained the candidate. Authority is revision-17 `changes/active/long-horizon-verification/spec.md`, the original TASK-003a read from the base commit, AGENTS, agent-rules, applicable design/doctrine/Bifrost contracts, reference router, spec-driven-development, maintainer-style and testing-workflows. The candidate task appendix is evidence, not approval of its deviations. All seven discovery reports, claims comparison, follow-up, subject and verification report were read. This is a fresh validation role; agreement among discovery reviewers was not used as proof.

## Discovery disposition ledger

| Discovery ID | Disposition | Final boundary |
|---|---|---|
| B-01 | CONFIRMED | FIND-TASK-003a-1; enqueue Refused consumes unhanded work |
| INVREV-001 | CONFIRMED | FIND-TASK-003a-1; same cause includes unavailable owner/authority |
| B-02 | REVISED | FIND-TASK-003a-2; exact representable duration, including live fixture closure below |
| INVREV-002 | REVISED | FIND-TASK-003a-2; reject additional infinity/maximum-duration policy/proof demand |
| DATA-001 | REVISED | FIND-TASK-003a-2; correct shared SQL, preserve ordinary error path and adapt live long-wait fixture |
| B-03 | CONFIRMED | FIND-TASK-003a-3; missing composed runtime acceptance proof |
| B-04 | REVISED | FIND-TASK-003a-4; consolidate cron, query-observation and writer-race proof in C13 |
| INVREV-003 | CONFIRMED | FIND-TASK-003a-4 |
| SYS-001 | CONFIRMED | FIND-TASK-003a-4; production scheduler loop must be exercised |
| B-05 | CONFIRMED | FIND-TASK-003a-5; pressure/retry and group readback must identify the same eligible source |
| STD-001 | CONFIRMED | FIND-TASK-003a-6; explicit rustdoc gate, batched with behavioral remediation |
| Maintainer/security material ledgers | Validated empty | Their scoped PASS does not settle the distinct acceptance/durability/documentation gaps |
| Follow-up new material ledger | Validated empty | HUMAN-CLEANUP 1–3 below are explicit human additions |

Rejected portions are removed from remediation scope: no maximum-duration runtime guarantee or infinity encoding is inferred from the old fixture; no ordinary cron skip change; no visibility/recovery barrier, source-settlement wait, quota, serialization gate, fairness promise or benchmark demand. The retained proof findings assert missing evidence, not unobserved production failures.

## Validated findings

### FIND-TASK-003a-1 — Preserve pending work until successful handoff

**CONFIRMED; INCORRECT; important.** Discovery B-01/INVREV-001. Locations: `crates/wyrd/wyrd-server/src/verification/scheduler.rs:361–388`, especially `None => None`, `Ok(outcome) => Some(outcome)`, and unconditional `consume_group`.

**Obligation.** REQ-004 consumes pending work only on execution enqueue or identical-snapshot conflict; task locked decision 7 explicitly includes refusal among rollback/end-tenant-pass outcomes. Design grouped verification likewise says failures retain pending work. This is ordinary transactional retention, not extra recovery.

**Independent trace.** Direct verification and runner result builders record authenticated owner/subject/source-Verifier/group. `ScribeWrite::group_activity` extracts only grouped input/result metadata; `ScribeSink::write` ACKs non-lost frames, `schedule_groups` writes the exact matching tuple through `record_group_activity`, then consumes submit reservations. This is the sole production pending writer. `claim_due_group` at `verifier_executions.rs:1750` reads `binding_activity` and may legitimately return `owner: None`; `BackgroundAuthority::resolve` may separately return None. `binding_activity` requires active exact owner/principal and a qualifying exchange newer than 86,400 seconds by default. Thus `after: 2d` with no later exchange reaches the unavailable-owner branch without any crash. The scheduler deletes its sole notification. Authentication renewal only arms schedule cursors (`queries/verification.rs:79,426`), so renewal cannot restore the lost group work.

When authority exists, `GroupPoller::poll` performs the authorized snapshot and calls `enqueue_group`. Its `resolve_binding`/`ResolvedTarget::refusal` checks Verifier readiness and subject availability; Refused creates no execution. The scheduler still deletes/commits the row. Restoring eligibility without new source evidence leaves nothing to retry. All enqueue/claim/consume callers were searched: production group handoff has one scheduler caller; other callers are the existing server/SQL tests. Ordinary `schedule_due` deliberately advances cron cursors on skipped/refused occurrences; manual enqueue reports/refers refusal to its caller. Neither supplies the finite group's missing future notification. Query failures already retain the transaction by not committing.

**Smallest correction.** At the existing scheduler handoff owner, consume/commit only Enqueued or AlreadyEnqueued. Unavailable owner/authority and Refused end that tenant's pass with the pending transaction uncommitted. Retain current authorization/readiness checks, error propagation, permit release and cancellation semantics; correct the affected rustdoc/telemetry description and appendix's contrary claim. No new retry owner or helper API is needed.

**Closure.** Extend existing C13 to cover unavailable owner/activity and enqueue refusal, assert no group execution and unchanged pending identity, restore eligibility and prove later handoff succeeds without another source write. Use exact P1 below. Do not change cron semantics.

### FIND-TASK-003a-2 — Delete the private shortening of representable after values

**REVISED; INCORRECT/DRIFT; bounded contract defect.** Discovery B-02/INVREV-002/DATA-001. Locations: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:191–210,1730`.

**Obligation and trace.** REQ-004/task decisions 1–2 retain TriggerDuration and database statement time plus authored after. `TriggerDuration::parse` at `wyrd-spec/src/card/trigger.rs:92` accepts positive nine-digit s/m/h/d counts. Registration freezes after in GroupEndSettings unchanged. Every grouped input/result writer converges on RECORD_GROUP_ACTIVITY_SQL, which caps epoch seconds at 3,155,760,000. `1000000h` is valid and PostgreSQL-representable (3,600,000,000 seconds), yet the stored pending due time is about fourteen years early while inspection still reports the authored duration. Claim predicates subsequently use that incorrect due time. No other SDK/schema/registration limit supplies authority for this cap.

**Smallest correction.** Delete the constant, extra bind and LEAST shortening at the existing SQL owner; derive representable due times directly from database statement time and authored duration. Keep tuple matching, batched upsert, database clock, TenantConn and ordinary SQL-error/outbox retry behavior. Do not introduce a maximum-duration policy, infinity representation, new validator or shared TriggerDuration restriction.

**Reachable fixture closure, not silent test weakening.** `fixtures/cards/verify_in_real_time/forever.yaml:17` currently uses `999999999d`. Its only live executable consumer is Rust `grouped_executions_include_late_evidence` at lines 2765–2879. That test emits a real grouped source turn before the late conversation, and later requires the neighboring binding not to execute. Removing the cap blindly would overflow PostgreSQL timestamp arithmetic: Scribe ACK → schedule_groups SQL error → Scheduling → identical tenant slice retry; the unconsumed slice could obstruct later evidence in that tenant. Therefore include this consumer in the same correction. Set this test fixture's after to a PostgreSQL-representable long duration beyond the old cap, such as `1000000h`, and replace the fixture/test's largest-duration descriptions with its actual long-wait purpose. Preserve its no-due and neighboring late-evidence/progress assertions. The original task explicitly permits removing forever/ceiling-only cases when unused, deletes obsolete timing helpers/tests, and requires late evidence/coexistence; it does not require a new successful extreme-timestamp scheduling policy. The old Ended overflow arithmetic was deleted with that old mechanism. A representable long-wait fixture keeps the useful regression while the SQL proof establishes exactness. Do not claim all parser-accepted durations are now schedulable; out-of-range arithmetic retains the ordinary failure path.

**Closure.** Extend existing C9 with an immediate database-clock exact due-time assertion for `1000000h`, retaining its ordinary `1h`, debounce, claim, identity and settlement assertions. Run P2 and, because the live fixture changes, P4. No wall-clock wait or infinity case is required.

### FIND-TASK-003a-3 — Prove one conversation composed from all three real SDK runtimes

**CONFIRMED; MISSING; acceptance-proof gap.** Discovery B-03. Locations: Rust `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1335`, Python `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py:470`, TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:521`.

Task Scenario 1 expressly requires a real shared conversation populated by Rust, Python and TypeScript under the same server/tenant/owner/group. Each complete current C1 body creates its own server/support state; four direct turns, submit and replayed HTTP enqueue all originate in that single language host. No other C1 caller invokes the other SDK runtimes against that common owner. Three parity tests therefore do not prove the required composition; no production cross-runtime defect is asserted.

**Smallest correction.** Extend the existing Rust C1 host's shared conversation with real Python and TypeScript SDK calls against its existing bound server/credentials/bundle/group. Retain six exact facts, ordinary verify/submit, replay, ownerless/denied negatives and pinned readback. Reuse the existing SDK runtime/process support and public hydration/client APIs; existing cross-runtime process patterns already occur in Python local-development integration support. Do not create another server, general adapter API or test framework, and do not initialize Python/Node lifetimes inside Rust. Let foreign code run in its own process. Reversible local orchestration remains implementation-owned.

**Closure.** P3 selects one host test which must fail if either foreign SDK contribution is missing or assigned to another owner/group. Existing separate parity/negative journeys stay. No full SDK suites are required.

### FIND-TASK-003a-4 — Make C13's mandatory concurrency observations non-vacuous

**REVISED; MISSING; important proof gap.** Discovery B-04/INVREV-003/SYS-001. Locations: `crates/wyrd/wyrd-server/tests/integration/pg_verification_groups.rs:584–600,630–662,692–704`.

Task C13 explicitly requires actual query invocations, a source item ACKed during held handoff, and ordinary cron work scheduled while a group snapshot blocks. The full test and Support setup show: support.yaml has no schedule binding; the final `batched.pass` merely returns Ok and no queue state is checked; the manually joined pass methods bypass `VerificationScheduler::run`'s new separate-loop composition. Empty group-pass vectors do not observe absence of reads. The concurrent arrival directly invokes record_group_activity using a second TenantConn, proving the SQL row-lock seam but bypassing the Scribe writer/ACK ordering required by the scenario. C8 proves result ACK rescheduling separately, C11 proves pressure/drain separately, and C12 scheduled scoring separately; none closes these combined seams.

Production source currently has separate loops (`scheduler.rs:136`) and capacity acquisition before claim (`:353`); this is a missing regression proof, not a claim they currently fail. The production topology shares the runner semaphore with group preparation, while ordinary cron admission has its own future.

**Smallest correction.** Strengthen the existing C13 owner using current server, scheduled fixtures, queue readback and query-fault/trace controls. Exercise the actual scheduler run owner; while its group query is observably stalled, make a real ordinary cron occurrence due and assert its durable queue execution before releasing/cancelling the group read. This must distinguish simultaneous progress from a cron occurrence that ran before the stall. Observe query activity for full-capacity and idle/post-handoff cases rather than only returned vectors. Send/ACK a real matching source item during the held transaction and verify its background upsert leaves fresh pending work after handoff; retain the useful direct SQL lock/claim exclusion proof. Keep rollback/cancellation/32-item assertions. No new fault API, instrumentation framework, harness or production mechanism is prescribed.

**Closure.** P1 alone. The current independent P1 PASS in verification.md proves current assertions execute, not these absent observations.

### FIND-TASK-003a-5 — Couple Python/TypeScript pressure recovery to the observed group

**CONFIRMED; MISSING; runtime-owned proof gap.** Discovery B-05. Locations: Python test `:1021–1057`, TypeScript test `:1055–1095` in the files above.

Task Scenario 7/C11 requires grouped pressure refusal, retry/drain and resulting group execution. The complete Python/TS bodies pressure and retry assistant answer-check, which lacks a group-end binding; after pressure ends they create support's binding and submit a new turn. Python even uses another group string; TS reuses the string with another owner/source. Exact frozen tuple matching excludes the pressured source from support's group, so this tail cannot establish the required retry-to-pending/snapshot path. Rust C11 establishes support before pressure, retries that source and asserts its one pinned fact; it is the existing sibling pattern to reuse.

**Smallest correction.** In each existing Python/TS C11, register/hydrate the eligible support source before pressure, pressure its actual grouped verify/submit, retry the same eligible source/group after drain, then read that group's execution and assert accepted retry once/refused attempts absent. Preserve typed retry timing, neighbor progress, ungrouped coverage and accepted-row accounting; remove redundant independent happy-path tails. No production change or extra harness.

**Closure.** P5/P6 only, in their owning interpreters. Rust C11 needs no new run solely for these test changes.

### FIND-TASK-003a-6 — Complete changed-item rustdoc

**CONFIRMED; VIOLATION; explicit merge gate.** Discovery STD-001. AGENTS §16 and agent-rules explicitly require substantive rustdoc on touched private items/aliases/tests and # Panics when a panic remains possible. `groups.rs:407` introduces Firsts with ordinary // only; `Visible::input/result` at `:385–401` merely repeat their operation names despite different execution/result identity keys and shared Earliest retention; test `Support::activity` at `pg_verification_groups.rs:252–257` accepts &str and expects GroupId validation without documenting its panic.

Full bodies, Earliest::keep, Sources::decode, Visible::executions/pin and activity's C13 callers confirm these exact contracts. Document the alias's two optional time anchors, the insertion methods' respective keys/earliest-payload retention and the invalid-group panic. No behavior/refactor/test is required for prose. Batch with the same remediation, inspect against source and run Rust format/lint checks. This explicit rustdoc rule, unlike optional naming preferences, can block by itself.

## Separate human-directed cleanup ledger

The user's added stale/unused/retired-code instruction is the authority for these additions. They are not independently discovered behavioral FINDs.

| Follow-up group | Disposition | Independently validated correction and preservation |
|---|---|---|
| HUMAN-CLEANUP 1: DueGroup unused projections | CONFIRMED | Delete DueGroup.due_at/card_uid/verifier_card_uid, assignments and unused select/decode values. All DueGroup references are definition/claim, GroupPoller::poll, Visible::pin and existing SQL assertion; none reads those fields. Scheduler uses binding/group/settings/owner. Narrow only DUE_GROUP_SQL returned columns and GROUP_BINDING_SQL target tuple. Keep pending due_at storage/filter/order/indexes; DueBinding.due_at remains live in cron. Keep persisted verification binding targets, GroupActivity exact identities, source predicates, enqueue_group resolve_binding/readiness/authz and TenantConn/RLS. P1/P2 already compile/exercise this boundary; no mirror test needed. |
| HUMAN-CLEANUP 2: stale descriptions | CONFIRMED | Remove runner.rs:402–403's nonexistent ceiling bypass (Grouped dispatch now always uses pinned lists); remove how-verification-works.md:166's only-automatic-Trigger claim while keeping the Bind link; remove MCP media rustdoc's retired coverage wording. These are descriptions of removed behavior, not requests to restore it. Source dispatch/Trigger and scoped docs/format proof suffice. |
| HUMAN-CLEANUP 3: live review fixture/helper names | CONFIRMED, atomic rename only | Six fixtures are live. Rename filename and metadata.name together: conversation-review→conversation-verifier, coding-review→coding-verifier, event-review→event-verifier, late-review→late-verifier, refund-review→refund-verifier, undeclared-review→undeclared-verifier. Update all corresponding Service YAML refs listed in followup-review.md, local grouped names/messages and helper callers across Rust/Python/TS/MCP; keep assertions. Rename fixture webhook to /hooks/refund-verification together with Python's two receiver lookups and TS's two expected paths. Preserve provider markers/Agent claim text where it describes actual live analysis, source grammar, exact source Verifier names, Card version pins, permissions and Operator action/body. |

For cleanup 3, independently searched typed active files (excluding generated dependency trees/history) and read all six Verifier fixture bodies plus refund Operator. Their sources reference turn-check/coding-turn/frame-verifier/event-fact/answer-judged/refund-turn, not the renamed group Verifier itself, so atomic names/path changes preserve selection identity semantics after fresh registration. Service references reach all six; C3's ambiguous/undeclared/external/unbound negatives remain meaningful. Read the full Rust group_reviews/assert_no_new_review, Python group_reviews, TS groupReviews and MCP group_review/ids bodies: they query/read retained executions; rename them, do not delete their readback, polling, no-repeat or payload checks. Update all callers, including Rust grouped_reads_need_the_owner_grant; keep that grant-denial test and its grants. No compatibility aliases, wire changes, schema generation or authz weakening is justified by these names.

Retirement exclusions are independently upheld: shared cron cursor/authentication arming, manual request idempotency, queue claims/leases, Scribe reservations/audit, GroupPoller/Reader/Sources/Visible/Earliest/Payloads, persisted group_sources and PinnedGroup.snapshot have active consumers. Do not delete them. Removed clock keys in rejection fixtures prove refusal. No dead production GroupReview export was found to remove. Unrelated software-review language and live Agent span-review text remain outside the rename.

Cleanup proof uses existing focused journeys only where behavior/fixture references are affected, plus compile/type/static checks for pure local renames. The primary commands P1–P6 cover the changed SQL/server/support/late/pressure owners. Add existing C2 Python/TS for changed webhook paths; Rust C3 covers coding and negative fixture registration, Rust C5 covers retained group helper/coexistence, and C10 MCP media covers renamed shared MCP helper and frames. Use those exact original-task command lines/selectors, not full journey suites. Do not rerun all C1–C13 as a default. Static search must show no active references to six old basenames or four old helper names; search is review evidence, not a new permanent gate. No cleanup item above authorizes deleting active assertions.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| A1 after-only contracts, six facts, owner identity, no-IO producers | Trigger/registration and C1/C7 supplied evidence; shared SQL shortens after; mixed-runtime story absent | FAIL — FIND-2/3; other sub-obligations PASS |
| A2 visible input/result union, identity dedup/order/null alignment/failed values and immutable pin | Full Sources/Visible/Payloads/PinnedGroup, queue claim reconstruction; C3/C6/C8 supplied evidence | PASS |
| A3 digest idempotence, overlapping distinct snapshots, remove coverage/finality/discovery | Queue digest/conflict index and no outstanding gate; complete removal diff; C6/C8/C9 | PASS |
| A4 settled-order baseline/equal/change/recovery/replay | Binding lock, COMPLETE replay flag, predecessor/dispatch SQL; C2/C9 | PASS |
| A5 coding originals, no-trace source values and optional authorized live Agent queries | Existing hosted/query boundary retained; C3 | PASS |
| A6 normal 20/21-frame evaluation, failed/empty and per-item retry | Ceiling configuration/helper/runner bypass removed; C4/C10 and retained Rust queue-retry story | PASS |
| A7 tenant/owner/group/exact-source isolation, mixed runtime, ordinary ungrouped behavior | TenantConn, source tuple predicates, authenticated owner; C1/C5; missing composed C1 | FAIL — FIND-3 only; isolation/ungrouped PASS |
| A8 scheduled samples and staging/neighbor progress retained, group retry readback | ClaimLoop shared capacity and retained schedules; C11/C12; Python/TS disconnected group tails | FAIL — FIND-5; retained ordinary behaviors PASS |
| A9 minimal wire/stub/generated/OpenAPI/MCP/docs closure | Source contracts and declaration diff; C7/C10 and reported static lanes | PASS for shape/removed fields; stale descriptions/terminology separately require HUMAN-CLEANUP 2/3; no public ceiling permitted by FIND-2 |
| A10 credible original functional scenario closure, benchmark deferred | Supplied C1–C13 plus independently rerun C13; missing composed/asserted proofs | FAIL — FIND-3/4/5; benchmarking correctly pending |
| A11 ACK updates, RLS one row, transactional success/rollback, arrival and capacity/cron bounds | Scribe/queue/scheduler full paths and C8/C13; unwanted consumption plus omitted writer/read/cron assertions | FAIL — FIND-1/4; bounds/schema/normal success source-proven |
| INV-001–005 and repository gates | Existing Rust owners, accepted query authority, snapshot inputs and finite permits retained | FAIL only for FIND-1/2 and explicit rustdoc FIND-6; no separate security/tenant finding |
| Non-goals and accepted limits | No new recovery/visibility/adoption/quota engine or benchmark machinery in range | PASS; REQ-015/AC-009 intentionally unqualified |

## Focused closure catalogue

These are proposed remediation proof commands, not new runs by this validator. Use tracing on failure and confirm nonzero exact selection counts. Existing prerequisites in TASK-003a apply only when native builds change; do not rebuild every SDK merely to rename local text.

**P1 — existing C13; FIND-1/4 and cleanup 1**

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff)"'
```

**P2 — existing C9; FIND-2 and cleanup 1**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)"'
```

**P3 — existing Rust C1 host; FIND-3**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_support_resumes)"'
```

**P4 — existing Rust C6; FIND-2 live fixture closure**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_executions_include_late_evidence)"'
```

**P5/P6 — existing Python/TS C11; FIND-5**

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_verification_staging_refuses_and_recovers'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^verification_staging_refuses_and_recovers$"'
```

For explicit cleanup 3's additional affected fixtures/webhooks, reuse only these original-task exact selectors (each retains its original managed-Postgres command): C2 Python `::test_grouped_refund_changes_verdict`, TS `^grouped refund changes verdict$`; C3 Rust `test(=verify_in_real_time::grouped_coding_uses_declared_sources)`; C5 Rust `test(=verify_in_real_time::grouped_conversations_coexist_with_drift)`; C10 MCP `test(=verification::pg_tests::grouped_frames_use_visible_outcomes)`. Their complete command forms are already fixed in the original TASK-003a C2/C3/C5/C10 catalogue. No new test names/selectors/harness are prescribed.

Static lanes for the actual bounded write set: Rust fmt/lints, check:tenant-isolation for changed SQL; Python format/lints/typecheck for touched Python; TypeScript format/lints/typecheck for touched TS; docs:check for guide correction; git diff --check. Reuse existing mise tasks. No schema/wire/dependency change is recommended, so codegen regeneration, OpenAPI journey rerun, dependency audit and broad gate are not independently demanded by these corrections. Keep any new/changed Rust rustdoc complete. Add a specialized owning lane only if the implementer's actual native support write set changes that boundary. Run Cargo-backed lanes sequentially on this checkout.

## Verification and coverage limits

Read the complete cumulative changed-file inventory and diff, expanded the changed owners into surrounding producer/consumer bodies and searched pending writers, claim/consume/enqueue callers, authority/activity and cron siblings, fixture references and test helpers. CodeGraph was used before source navigation, then bounded source/diff/search reads. No missing required report or unresolved material authority decision remains for this ledger. Generic CodeGraph edges were not assumed exhaustive; explicit caller searches and source bodies supplied the proofs above.

This validator ran no tests, builds, providers, databases or benchmarks. Read-only `git diff --check` passed. Root's exact traced C13 rerun passed with one selected test and two setup migration-idempotence tests; other C1–C13/static results are candidate-supplied evidence assessed against assertions, not fresh executions. Source tracing, rather than stress or process-kill tests, establishes the reachable branch defects. The full deployment and all SDK paths were not dynamically qualified. No report here claims benchmark PASS or repairs accepted ACK visibility/post-ACK loss, outbox/retention loss, continuous postponement, overlapping settlement order or durable backlog.

One bounded remediation task can carry FIND-TASK-003a-1 through -6 plus HUMAN-CLEANUP 1–3 using existing owners and harnesses. No source edits were made; this validator wrote only findings-validation.md in the repository.
