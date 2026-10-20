# Independent structured Ponytail validation

**Result: FIX_REQUIRED.** Five bounded findings remain: stable FIND-TASK-003a-1, -2 and -6, plus new FIND-TASK-003a-7 and -8. Prior -3, -4 and -5 are closed. HUMAN-CLEANUP 1–3 are closed separately. There is no unresolved material product or architecture decision; no spec revision is needed for the selected corrections.

## Subject and authority

Cumulative base `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`; candidate `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`; latest-fix locator `7cda4e93159ab328de2a74068d5e1978a2590818..3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`. HEAD independently matched the candidate. The cumulative inventory and relevant original/latest-fix source diffs, full affected function bodies, shared writers, callers and sibling paths were inspected. CodeGraph preceded code location; explicit source/caller searches expanded its results. Only this report was written.

Read the original TASK-003a at the cumulative base, approved specification revision 18, prior TASK-003a verdict/validation, owner-amended R1 and implementation evidence, all seven current discovery reports, claims-comparison, followup-review, subject and verification reports. Applied AGENTS, agent-rules, the reference router, spec-driven-development, maintainer-style, testing-workflows, errors, and applicable design/doctrine/Bifrost ownership, grouped-verification, authority and query boundaries. Original task A1–A11 remain controlling except the explicit revision-18 replacements for failures and duration maximum. Earlier reviewer recommendations and implementation PASS labels are not product authority.

The full captured root rerun was read before correlating the owner-grant failure: `/tmp/wyrd-r1-owner-grant-3d0b301c5.log:80` records the actual terminal `guarded` RBAC refusal; `:308–309` records the actual completion timeout at `support/mod.rs:409`. This is evidence from the orchestrator's run, not a test executed by this validator.

## Discovery disposition ledger

| Discovery claim | Disposition | Independent resolution |
|---|---|---|
| SYS-R1-001 | REVISED | Retain FIND-TASK-003a-1 for typed peer-error misclassification. The production diagnosis is confirmed. Require the small pure classification check and existing transaction regression; do not require a new peer fault API or rolling-deployment harness. |
| DATA-2 | CONFIRMED | Retain FIND-TASK-003a-2 for fixed elapsed-day exactness. Write-time maximum/clamp removal are closed, but direct day-bearing timestamptz arithmetic is not equivalent to the parser's 86,400-second days. |
| STD-001 | REVISED | Retain FIND-TASK-003a-6 for the new associated type and fallible visitor documentation. Reject the claim that `visit_str` itself validates/refuses source grammar: SourceText decoding accepts strings verbatim. Document that actual contract. |
| MREV-R1-01 | CONFIRMED, consolidated | Same FIND-TASK-003a-6 cause; its distinction between verbatim string decoding and later source validation is correct. No second finding per visitor method. |
| DATA-1 | REVISED | New FIND-TASK-003a-7, restricted to the explicitly selected authority-NULL pre-admission exception. Reject bundled totalization/hardening of pre-existing authority-present input arms. The sole production refusal writer currently emits correct values; no public malformed-write exploit is claimed. |
| SEC-001 | CONFIRMED | New FIND-TASK-003a-8, existing security journey fails and its trace-only phase is vacuous under the approved terminal-denial lifecycle. Production terminal consumption remains correct. |
| Behavior review empty proposed ledger / general prior-closure claims | REVISED | No additional finding to retain. Its general FIND-1/-2/-6 closure and effective owner-grant scenario conclusions are narrowed by the concrete source counterexamples below. Other acceptance conclusions survive the cumulative audit. |
| Invariant review empty proposed ledger / rejection of arbitrary SQL corruption | REVISED | Its rejection of unrelated privileged mutation hardening stands. It does not dispose of the explicit R1 requirement to constrain the newly permitted authority-NULL shape. Its general FIND-1/-2/-6 and owner-grant closure claims are narrowed below. |
| Follow-up A (SEC-001) | CONFIRMED | Source and the preserved root trace independently support the same existing-journey regression; no additional cause or ID. |
| Follow-up B (narrow DATA-1) | CONFIRMED | R1's selected persistence boundary expressly requires this constraint despite no current malformed production writer. Broader legacy hardening remains rejected. |
| Literal import/qualified-type observations and C13 UTC-midnight limitation | REJECTED as remediation findings | No additional material finding established at the required threshold in these reports. Do not turn them into optional advice or expand this remediation. |

No unique proposed material claim was discarded for lacking votes. Conversely, agreement among reports did not establish closure. No Critical finding is confirmed; the retained behavioral/proof findings are Important, and -6 is the explicit mandatory rustdoc gate.

## Validated finding ledger

### FIND-TASK-003a-1 — Keep peer infrastructure rejection retryable

**REVISED; INCORRECT; Important.** Discovery SYS-R1-001. Location: `crates/wyrd/wyrd-server/src/verification/groups.rs:63–72`, especially `error.status() == 403` at line 67. Violates revision-18 REQ-004 (`spec.md:347–369`), R1's typed-cause requirement (`R1:52–64`), and `architecture/wyrd-design.md:1305–1323`.

**Source trace and consequence.** The owner of this distinction is `GroupPollError::from`, used by Reader::run and the authenticated query constructor; its only production consuming scheduler is `hand_off`. `BifrostError::QueryPeerSecurity` is a typed 403 about peer credential/fence/replay checks (`wyrd-spec/src/vala/error.rs:147–156`), not owner permission. `OraclePeerAuthority::verify_forward_query` checks audience/fence/expiry (`oracle/peer_authority.rs:115–157`); `ReadyOracleForwarder::accept` maps failure to QueryPeerSecurity (`oracle/forwarding.rs:299–306`). The full remote path preserves that type: peer `forward_query` → `query_status`, `forward_remote` → `forwarded_query_error` → shared gRPC error conversion → `query/service.rs:215–226` → ScheduledQueryCaller::dispatch/run_with → Reader::run. Missing/invalid forwarding response metadata and TLS configuration also produce this type (`forwarding.rs:380–429`). No table grant denial is implied.

A stale cached role fence during replacement, or invalid peer configuration/response, therefore returns a genuine failed snapshot while the owner's grants remain valid. Current status-only conversion makes it Denied; scheduler `hand_off:465–493` writes an errored execution, then `hand_off_groups:379–393` consumes/commits pending. Repairing the peer does not restore that notification. Source evidence is discarded as outstanding work and a Wyrd fault is attributed to the owner. This remains the prior failure-classification boundary, so preserve -1 rather than creating a per-error ID.

**Selected minimum correction.** At GroupPollError's existing conversion, select actual typed owner-query permission failures: `WyrdError::PermissionDeniedRbac` and `WyrdError::Vala { error: BifrostError::QueryForbidden }`. Other errors, including QueryPeerSecurity, remain Read with stable reason/code. Correct its status-based rustdoc too. Keep rejection closed: no partial snapshot, no weakened fence/credential check, no peer retry or identity fallback. Existing scheduler None/rollback behavior already implements retention; cron's separate `.ok()` authority projection and normal query/sibling services need no change.

**Closure.** Add one pure regression in the existing `verification::groups::tests` module covering both real denial variants, peer security and an ordinary read failure. Assert peer failure is Read retaining its code, and real denials remain Denied with their error. This changes a pure classification branch, so this isolated proof plus existing P1's composed read-failure-retention/recovery and terminal-denial transaction proof is sufficient; do not add a new peer injection API/harness to simulate topology. Run C-CLASS and P1 below.

### FIND-TASK-003a-2 — Add fixed elapsed seconds rather than calendar days

**CONFIRMED residual; INCORRECT; Important.** Discovery DATA-2. Location: `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:202`. Obligations: revision-18 REQ-004 exact accepted duration (`spec.md:305–312`), original task locked decision 2, R1's fixed-year/exact-seconds replacement, and `architecture/wyrd-design.md:1283–1286`.

**Source trace and consequence.** TriggerDuration::parse (`trigger.rs:103–132`) scales `d` to 86,400 seconds and keeps authored text. Binding projection freezes those settings; the only production activity writer is ScribeSink::schedule_groups (`scribe_outbox.rs:766–802`) calling `record_group_activity`. Its SQL adds the authored interval directly to timestamptz; pending claim later consumes that due_at unchanged. `pool.rs:194–219` retains DSN/default settings and `TenantConn` binds the tenant without changing TimeZone. No UTC-only production policy supplies the missing invariant.

The data review's authorized managed-PG fixed-date proof used the actual operation and showed, at `2026-03-07 12:00:00-05`, New York `+ '1d'::interval` advances 82,800 elapsed seconds versus 86,400 for `24h`; UTC advances 86,400 for both. A supported database setting therefore makes equivalent accepted values schedule differently across DST. Existing P2's current-date/default-zone year assertion passes without excluding this case. No claim is made that its specific `365d` sample differed from `8760h`.

**Selected minimum correction.** Correct the one SQL activity owner to turn accepted duration into an interval of elapsed seconds before timestamp addition, e.g. extract epoch from the accepted interval and multiply by `INTERVAL '1 second'`. The accepted whole-second/year bound makes this an exact finite quantity. Preserve authored wire/storage text, statement-time coordination, tuple matching, batched upsert, RLS and ordinary SQL-error/outbox behavior. Do not introduce a global timezone setting/policy, new duration abstraction, alternate maximum or clock API.

**Closure.** Extend existing P2 with fixed-date DST-observing-session assertions for `1d` and `24h` using the corrected production arithmetic, including an exact 86,400-second advance. Preserve current one-year, normal-hour, debounce, claim, digest and settlement checks. No wall-clock wait or new journey is required for this SQL arithmetic seam. Run P2 below; the unchanged shared parser already has root-selected P7 proof.

### FIND-TASK-003a-6 — Document the newly added slot visitor accurately

**REVISED; VIOLATION; mandatory rustdoc gate.** Discovery STD-001/MREV-R1-01. Location: `crates/wyrd-spec/src/card/verifier/mod.rs:322–341`. Authority: AGENTS §16 (`:734–748`) and `agent-rules.md:32`, explicitly applicable to private associated types and every fallible method. The task-review skill preserves missing rustdoc as a blocker.

Original Firsts, Visible::input/result and Support::activity sites are now documented. The new Slot visitor still has undocumented `type Value = InputSlot`; `expecting`, `visit_str` and `visit_map` have no Errors sections. The outer deserialize's Errors section does not cover these separate new items. Full method bodies were read: expecting propagates the formatter write error; visit_map forwards the tagged remote decoder's missing/unknown tag, field and value errors. SourceText (`card/verifier/reference.rs:177–200`) is a transparent string wrapper deliberately accepting any string; its grammar is validated later. STD-001's proposed source-grammar refusal inside visit_str is therefore rejected.

**Selected minimum correction.** Add concise substantive rustdoc to the associated output and hooks, with Errors sections describing actual behavior: no practical decoding error for an already supplied string and deferred source-grammar validation, tagged map decode failures, and formatter write failure. Keep behavior, schema and public types unchanged. Do not manufacture validation or a new test to support prose. Inspect these exact items and run affected Rust format/lint checks; batch this with the behavioral corrections.

### FIND-TASK-003a-7 — Constrain the new authority-free exception to its selected shape

**REVISED; VIOLATION; Important.** Discovery DATA-1 and follow-up B. Location: `crates/wyrd/wyrd-sql/migrations/20261018000000_verification_group_pending.sql:46–75`. Explicit authority: revision-18 REQ-004 says the pre-admission error has no source inputs/snapshot and only this terminal shape may omit authority (`spec.md:355–364`); R1's selected persistence boundary expressly lists absent inputs/routes/snapshot and directs SQL constraints narrowly to permit that shape (`R1:66–79`).

**Producer, writers and consumers.** Enumerated all four production execution INSERTs: submitted, scheduled, pinned group, and refused group. `refuse_group:1824–1843`, called only by scheduler hand_off in production, correctly uses INSERT_GROUP_ERROR_SQL and omits authority, all input fields and snapshots. Manual/scheduled/group writers still provide their existing authority and proper input shapes. CLAIM_SQL admits only pending/retrying/expired-running; retry/release/termination require a running lease; completion requires lease fencing. Proper refusal records are never claimed or dispatched. StatusRow/execution_status (`verifier_executions.rs:2185–2235,2635–2658`) need no authority but decode any non-NULL group_sources as a BTreeMap. Public status authorization remains in VerificationControl::get_binding/get_execution.

The new refused_shape CHECK restricts terminal metadata but omits input/snapshot absence. Its input_shape ordinary object arm can admit authority-NULL object inputs/routes. Its group_shape accepts false equality for a one-byte non-NULL digest or array-valued non-NULL sources as well as NULL. The data review's migrated CHECK/NOT NULL clone admitted both counterexamples; array sources then conflict with StatusRow's map decoder. This proves a schema-admitted invalid exception shape, not an emitted application state or public exploit. The invariant review's broader arbitrary-corruption rejection does not override the explicitly required constraint boundary. The existing production writer is already correct.

**Selected minimum correction.** Strengthen the existing authority-NULL terminal alternative itself to require window_start/window_end, input_record_id/input_event_time, inputs/route_overrides, group_snapshot_sha256 and group_sources all SQL NULL, in addition to its present terminal metadata. Keep inherited no-errorless-terminal/no-retry-deadline/no-running-lease checks and ordinary authority-present shapes. One authoritative condition prevents falling through any ordinary input arm or malformed false-equivalence; no decoder fallback or new writer is needed. Reject DATA-1's broader totalization of old authority-present ordinary-input predicates: no newly unsafe writer or separate required correction was established there.

**Closure.** Extend existing P2 with database-rejected updates of an otherwise valid refusal to object inputs/routes and malformed non-NULL digest/source fields, while retaining valid refusal insert/readback/attribution, no claim/dispatch, and rejection of authority-free pending status. Use the existing fixture/transaction pattern. Run P2; no new schema, lifecycle state, migration compatibility or public security decision is required.

### FIND-TASK-003a-8 — Repair the existing owner-grant journey for terminal denials

**CONFIRMED; REGRESSION/MISSING effective proof; Important.** Discovery SEC-001 and follow-up A. Location: `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:3276–3289`, with contrary rustdoc at `:3204–3217`. Violates original task preservation of existing auth scenarios/caller closure and AGENTS §11 negative-path journeys; approved revision-18 terminal behavior (`spec.md:350–374`) governs the required test correction.

**Source and actual failure.** Full journey body `:3218–3320` writes its only two matching source turns while turns_only is active, then waits eight seconds with after=3s. `group_executions:2776–2803` queries only Bifrost results, so no-result assertions conceal the SQL terminal error. Changing to traces_reader authors no additional evidence. ScribeWrite::group_activity returns None for audit/capture/spans; grant/status reads and repeated flushes cannot recreate pending. After the first consumed denial, the second quiet phase can pass without exercising object permission. Restoring workload likewise authors no source activity. `Deployment::group_execution` (`support/mod.rs:386–417`) returns only Completed plus visible result, so it cannot accept the terminal error and times out.

Root's exact selected-one rerun with full tracing confirms this path: the actual guarded `WYRD_PERMISSION_403_DENIED_RBAC` refusal is logged, then the same binding fails the 90-second completion predicate. This is a real regression in the existing journey and missing effective object-denial proof, not production grant widening.

**Selected minimum correction.** Keep this journey, role scopes, real server and public SDK. After the first actual denial use existing binding latest execution ID then Execution status to assert terminal Errored, the actual capability-denial code/group/owner and absent result/snapshot/dispatch; preserve owner-attributed audit and no-provider-call assertions. While traces_reader is active, send new ordinary matching source activity and inspect a new terminal object-denial execution (the table-level QueryForbidden code), so this phase demonstrably attempts the source query. Restore workload, send new ordinary matching source activity, then require successful group evaluation and adapt pinned-value/cardinality expectations to all the facts actually written. Preserve the no-new-activity/no-repeat assertions for consumed attempts. Update stale retry/pending rustdoc. Do not change production denial lifecycle, weaken scope assertions, extend waits or introduce a recovery mechanism.

**Closure.** Run only C-GRANT below for this existing journey. It must prove both distinct real denial stages and successful new-activity recovery. No Python/TS wire change, new harness or full SDK suite is needed.

## Prior stable closure ledger

| Stable finding | Closure status | Source-grounded assessment |
|---|---|---|
| FIND-TASK-003a-1 | PARTIALLY OPEN | Rev18 replaces old blanket-retention advice. Stored inactive owner identity, typed AuthorityRefusal, terminal insertion plus consumption and recording-failure rollback are implemented. Status-403 query classification still loses the required cause as above. |
| FIND-TASK-003a-2 | PARTIALLY OPEN | Shared write-time fixed-year validation, both Trigger forms, direct raw HTTP negatives, exact authored text, clamp removal and `365d` long-wait fixture are implemented. Fixed elapsed-day arithmetic remains incomplete. |
| FIND-TASK-003a-3 | CLOSED | Grouped::foreign_turn (`Rust journey:1329–1390`) executes actual uv/Python and Node public SDK processes against the host's common bundle/endpoint/key/group, with isolated config home. C1 includes their two values in the six exact facts. No embedded foreign interpreter lifetime. Supplied P3 evidence supports this observation. |
| FIND-TASK-003a-4 | CLOSED | C13 now preserves an armed query fault across starved/idle passes, observes a real source ACK's SQL upsert waiting on the held row, asserts fresh pending, races claimers, proves 32-item bounds, and runs actual scheduler.run with cron made due after observable query stall. Durable cron admission is required before cancellation, and pending rollback remains asserted. Root P1 selected/passed one. |
| FIND-TASK-003a-5 | CLOSED | Python C11 (`:1007–1100`) and TS C11 (`:1011–1121`) register eligible support before pressure, refuse/retry the same source/group and assert exactly the accepted pinned value (TS also its execution ID). Neighbor and ordinary staging/count assertions remain. Supplied runtime-owned P5/P6 evidence applies. |
| FIND-TASK-003a-6 | PARTIALLY OPEN | Original cited Firsts/Visible/activity sites repaired; new visitor remains within the same changed-item gate, not a new ID. |

The new schema exception and existing owner-grant caller regression are distinct causes and receive next unused IDs -7/-8. They are not retroactive human additions or per-caller duplicates of -1.

## Separate owner-directed cleanup closure

| Human addition | Result | Independent source evidence and preservation |
|---|---|---|
| HUMAN-CLEANUP 1 | CLOSED | DueGroup now contains binding/group, owner Card/settings/principal and active only (`verifier_executions.rs:1000–1015`). DUE_GROUP_SQL returns binding/group; GROUP_BINDING_SQL returns owner/settings. Pending due_at storage/index/filter/order, binding/source exact matching, enqueue resolve_binding/readiness, ordinary DueBinding cron state remain live. |
| HUMAN-CLEANUP 2 | CLOSED | Runner dispatch describes ordinary pinned lists and has no ceiling bypass; guide removed only-automatic-schedule assertion; MCP media describes selected provenance. Current source agrees. No retired behavior is restored. |
| HUMAN-CLEANUP 3 | CLOSED | Six fixture basenames/metadata/reference callers and refund webhook were changed together; active-source search found no remaining targeted old grouping names/helpers. Rust/Python/TS/MCP readback and negatives remain; coding-child absence uses actual Verifier kind. Three intentionally invalid fixtures remain intact in fixtures/invalid with active rejection callers. No source grammar/version/auth change was needed for renames. |

## Reconciled cumulative A1–A11 acceptance

| Obligation | Implementation and proof assessed | Result |
|---|---|---|
| A1 after-only Trigger, refresh, exact duration, six facts, producer attribution/no SQL | Trigger/shared decoders, binding projection, Scribe activity SQL; C1 three-process six-value proof, raw HTTP bounds, P7/C7/P8 | FAIL only for fixed-day exactness (-2); other sub-obligations PASS |
| A2 visible input/result union, deterministic earliest logical dedup/latest result selection, aligned null/failed values, immutable inputs | Full Sources/Visible/Earliest/Payloads/pin bodies and queue reconstruction; C8/C3/C6 supplied evidence | PASS |
| A3 digest idempotence, equal-time/new IDs, distinct overlap, retired discovery/finality/coverage removed | PinnedGroup JCS/SHA-256, exact digest conflict lookup and index, pending-only poller, cumulative deletion/caller search; C8/P2/P4 | PASS |
| A4 settled-order baseline/equal/change/recovery and replay-safe Operators | complete, binding lock, COMPLETE/PREVIOUS_GROUP SQL and existing dispatch identity; P2/C2 supplied evidence | PASS |
| A5 original coding/no tracing, optional authorized live query and effective denial regression | Normal hosted/query boundary preserved, coding/negative fixtures and C3; existing owner-grant journey source and traced failure | FAIL for owner-grant regression (-8); coding behavior PASS |
| A6 ordinary 20/21 media evaluation, empty/failed selections, per-item retries | Ceiling config/helper/runner bypass removed; ordinary Grouped engine; retained media/MCP cases and C4/C10 evidence | PASS |
| A7 tenant/owner/group/version isolation, mixed runtimes, ordinary ungrouped | TenantConn/RLS, frozen exact tuple/owner/group predicates, C1 composed subprocesses and C5/C8 | PASS |
| A8 scheduled samples/checks, bounded staging/retry/drain and neighboring tenant progress | Unchanged cron sibling, shared ClaimLoop permit ownership, Scribe schedule-before-reservation; C11 corrected runtime-owned tails and retained C12 | PASS |
| A9 thin contracts/SDK typing/schema/MCP/docs and truthful terminal readback | Minimal GroupSnapshot, authored Python/source-generated TS, source decoding parity and status authorization; supplied codegen/runtime contract proof | FAIL only for narrow persisted exception contract (-7); ordinary projection/terminal writer behavior PASS |
| A10 original functional closure with credible focused proof, benchmarks deferred | Cumulative scenario source/evidence and strengthened P1/P2/P3/P5/P6; root exact owner-grant failure | FAIL for -8 and unclosed -1/-2/-7; no benchmark claim |
| A11 ACK scheduling, consumption/rollback/arrival, bounds/capacity/cron progress | Sole Scribe writer and queue/scheduler transaction bodies; actual C13 proof and root P1 | FAIL only for peer-error misclassification (-1); other concurrency/atomicity sub-obligations PASS |
| INV-001–005 / repository constraints | Existing server owners and authority, RLS, immutable pinned values, finite shared permits; nullable terminal persistence exception and rustdoc gate | FAIL for -1/-2/-6/-7; no separate authority bypass or new owner |
| Accepted limits and non-goals | No visibility barrier, source-terminal wait, coverage inventory, recovery ledger, hard finality, custom source ceiling, adoption/serialization or quota machinery | PASS; REQ-015/AC-009 intentionally remain pending |

PASS rows are source-level task acceptance supported by supplied focused evidence; they are not a claim of fresh full deployment qualification. General discovery PASS matrices cannot erase concrete counterexamples in FAIL rows.

## Focused closure catalogue

These are future remediation commands. PostgreSQL and Cargo-backed work must be sequential and selected counts nonzero. No test was run by this validator.

**C-CLASS — planned one pure classification regression in the confirmed existing module**

The name below is a proposed new focused test, not an existing selected/passed test. If the implementer chooses an equivalent name, record its exact real selector and nonzero count in the task evidence.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::groups::tests::query_failures_preserve_permission_kind)'
```

**P1 — existing transaction/retention/terminal-denial proof for -1**

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::pending_groups_debounce_and_stop_after_handoff)"'
```

**P2 — existing SQL exactness and terminal-shape owner for -2/-7**

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_snapshots_are_idempotent_and_react_in_settlement_order)"'
```

**C-GRANT — existing failed security journey for -8**

```bash
WYRD_LOG=info,wyrd_server::verification=debug mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::grouped_reads_need_the_owner_grant)"'
```

Use existing `mise run fmt`, `mise run lints`, `mise run check:tenant-isolation` and `git diff --check` for the bounded Rust/SQL write set. Source-inspect -6 directly. No public/schema-generated type/dependency/language-runtime change is selected, so no new Python/TS checks or schema regeneration is independently required by these corrections. Do not run full journey suites, `gate`, benchmarks or new fault/topology frameworks as task-remediation proof; final integrated change review owns broader qualification.

## Verification notes and remaining decisions

Independently checked immutable HEAD and read-only cumulative `git diff --check` (exit 0). Root P7 selected/passed one; root P1 and P2 selected/passed one each with managed migrations. The coordinated data-domain SQL session established migrated constraint behavior and fixed-date DST arithmetic; its temporary CHECK clone bypasses FK/RLS and proves no public exploit. Root C-GRANT first selected-one run failed; the full-trace rerun also selected one, failed one, skipped 130, exit 100, 109.997 seconds, run `091a7dc6-1d46-424c-8d68-436fa0adb007`. Its preserved trace and actual timeout were read and match the diagnosed source path. Other SDK/MCP/static results remain supplied implementation evidence, compared with their assertions rather than claimed as fresh runs.

The complete retained correction set stays within existing GroupPollError conversion, activity SQL, migration CHECK, InputSlot rustdoc and the existing Rust security journey. Existing helpers/owners already provide query rejection, transaction rollback, SDK status, fixture lifecycles and native SQL interval arithmetic. There is no missing product, public API, persistent-state, concurrency, resource, security-policy or cross-service choice left for an implementer. No source or test was changed to obtain these results. A bounded remediation can proceed without another specification decision.

Skipped fresh builds/tests/PG/provider/deployment/benchmark runs in this validation role. Accepted visibility/crash-signal/outbox/retention loss, postponement, overlap/settlement order, durable backlog and fairness limits remain accepted; no performance qualification is claimed.
