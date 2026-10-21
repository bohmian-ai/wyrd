# Focused follow-up review

**RESOLVED** for both assigned uncertainties. Subject: base `22335c2c40a5ad24c27bfa0d4686f4da49f922c5` through immutable candidate `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`; HEAD matched candidate during review. This is discovery evidence for independent validation, not a final task verdict.

## A. Owner-grant journey

**SEC-001 is source-supported.** The assertion that the original owner-grant scenario remains effective is incorrect.

Inspected path:
- `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1268–1276,2776–2828,3218–3320`
- `sdks/wyrd-sdk-rust/tests/integration/support/mod.rs:386–417`
- `fixtures/cards/verify_in_real_time/support.yaml` and `conversation-verifier.yaml`
- `crates/wyrd/wyrd-testing/src/server.rs:1235–1257`
- `crates/wyrd/wyrd-server/src/scribe_outbox.rs:275–295,766–800`
- `crates/wyrd/wyrd-server/src/verification/scheduler.rs:363–399,428–504`
- `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:200–232,264–278,1804–1843`

The journey grants turns-only, revokes workload, then calls `support.turn` exactly twice for group guarded. These are its only source writes. Each is an ordinary grouped verify. The fixture binds the conversation to those exact turn-check inputs/outcomes with after=3s.

The first eight-second `assert_no_new_group_execution` repeatedly calls `group_executions`. Its flush settles the Scribe outbox then publishes Scribe. ACK of source inputs/results can create/move pending through the existing scheduling SQL. Actual query denial is classified terminal by scheduler hand_off; refuse_group inserts the error and hand_off_groups consumes pending in the same committed transaction.

The subsequent operations are denial readback, role grant/revoke, the second quiet assertion/flushes, role restoration, and the completed-execution wait. There is **no subsequent source verify/submit**. Audit writes from those reads/role operations do not substitute: ScribeWrite::group_activity explicitly returns None for Audit, Capture and ServerSpans. Repeated flushes settle/publish existing writes; they do not author new turn evidence. The first quiet phase already settles the prior source writes. No documented activity producer remains to recreate pending after consumption.

The quiet helper queries only `vala.verification.results`, not execution status. A terminal pre-admission error produces no result row, so the first assertion hides the error. The trace-only role phase can remain empty without any new query attempt and cannot prove its advertised object-scope rejection. Finally `Deployment::group_execution` returns only Completed with a visible result. Restoring workload alone cannot satisfy that predicate after terminal consumption.

Authority is explicit: approved spec revision 18 `spec.md:350–374` requires terminal query-auth denial, atomic consumption and says “Reactivation alone does not resurrect consumed pending work; new source activity follows normal scheduling.” R1 `TASK-003a-R1-retain-pending-work-and-remove-stale-code.md:83–93` repeats the rule and uses latest binding/execution status for error visibility. Original TASK-003a preserves existing auth rules and requires original functional scenarios and caller/test closure; AGENTS §11 requires real negative-path journeys. This does not reopen the superseded blanket-retention correction of FIND-TASK-003a-1. The production terminal behavior should remain.

**Smallest correction boundary:** update this existing journey and its rustdoc to read the first terminal error through existing binding latest ID/execution status, preserve no-result/no-snapshot assertions, create fresh ordinary source activity while trace-only is installed and prove another actual object-scope denial, then restore workload and create ordinary new source activity before expecting successful verification. Expected evidence must reflect the newly written turns. Do not change production scheduling, add a recovery mechanism, weaken authorization, or merely extend waits. Keep this distinct from FIND-1's production classification closure.

The orchestrator reported an exact selected-one run failed with one errored-group metric. Full trace/panic evidence was not yet available here; this report does not attribute that dynamic failure to a particular panic. The source contradiction above stands independently.

## B. Narrow persisted terminal exception

**DATA-1 has an explicit task-required constraint basis; it is not merely a hypothetical public-writer exploit.** Its scope should be narrowed to the newly permitted NULL-authority states. The broader totalization of old ordinary-input predicates is not necessary to close this claim.

Inspected path:
- `crates/wyrd/wyrd-sql/migrations/20261018000000_verification_group_pending.sql:38–78`
- prior input/group definitions in `20261016000000_verifier_executions.sql:51–78`
- all four current execution INSERTs and claim/settlement transitions in `queries/verifier_executions.rs:89–139,244–278,328–424,481–508`
- refusal owner `refuse_group:1824–1843`, scheduler caller `scheduler.rs:478–504`
- execution readback `verifier_executions.rs:2185–2235,2635–2658`
- existing P2 checks `tests/integration/pg_verifier_executions.rs:2897–2983`

The sole pre-admission production writer INSERT_GROUP_ERROR_SQL correctly omits authority, all input/window/record/route fields and snapshots. Scheduled, submitted and successfully pinned group INSERTs provide authority and their normal inputs. Claim only admits pending/retrying/expired-running; completion/retry/release/termination require a running lease. No current production malformed-row producer or public exploit is demonstrated.

However, R1's **selected persistence boundary** is stronger than merely ensuring today's writer emits NULLs. At `R1:66–79` it enumerates `inputs/route_overrides/group_snapshot_sha256/group_sources: absent` and explicitly directs narrowing the existing SQL constraints to permit **the above** terminal shape. Spec revision 18 `spec.md:355–364` says the pre-admission error has no source inputs/snapshot and “Only this terminal shape may omit authority/pinned inputs”; architecture `wyrd-design.md:1308–1316` agrees. This is the skill's exception for an explicitly required otherwise dormant boundary. It requires no new persistence decision: the permitted shape is already selected.

Actual CHECK behavior contradicts that bounded obligation:
1. refused_shape checks terminal metadata when authority IS NULL but does not require absent input/window/record/route/snapshot fields.
2. input_shape's existing ordinary group object arm accepts object inputs/routes even with NULL authority; the other ordinary arms are also not excluded by authority NULL.
3. group_shape equates false with “digest is not exactly 32 bytes” and “sources is not an object.” That accepts non-NULL one-byte digests and array sources as well as NULL. A terminal row with NULL inputs/routes, one-byte digest and array sources satisfies those expressions.
4. StatusRow decodes non-NULL group_sources as a BTreeMap. An array violates that decoder and cannot provide the prescribed normal error readback. This is a consequence of a schema-admitted invalid state, **not** a claim that today's public path emits it.

DATA-1's supplied migrated-CHECK-clone experiment corroborates these exact expressions; I did not rerun it. The clone bypassed FK/RLS and does not establish public reachability. That limitation does not negate the specifically required constraint obligation. Conversely, arbitrary unrelated SQL corruption, ordinary legacy NULL-input gaps and general schema hardening remain outside this follow-up.

**Smallest correction boundary:** retain the existing migration and queue owners; make the authority-NULL alternative itself require the full specified absent input/window/record/route/snapshot shape, so no ordinary-input arm or malformed-snapshot false-equivalence can admit extra states. Preserve all authority-present ordinary submission, schedule and running-failure shapes. No new state, writer, decoder fallback or broad constraint rewrite is required. Extend existing P2 with rejected writes of non-NULL object inputs/routes and malformed non-NULL snapshot fields on an otherwise valid refusal, alongside its existing valid refusal/readback/nonclaimability/runnable-authority checks. Such constraint-negative proof is expressly task-required even though application writers are currently correct.

## Verification and scope

Read the follow-up role, subject and claims comparison, relevant discovery reports, original task, R1 owner amendments, revision 18 requirements, applicable architecture and agent rules. CodeGraph was used before code search; its broad result did not contain this test, so direct source inspection followed. Both latest-fix and cumulative changed-surface evidence were inspected for these two boundaries only. Reports are evidence, not votes.

No source edits, tests, managed Postgres sessions, builds, broad audit, benchmark or accepted-limit repairs were performed. Root owns the sequential traced journey rerun. Independent Ponytail validation must assess SEC-001 and the narrowed DATA-1 proposal before assigning final findings/verdict.

