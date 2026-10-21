# TASK-003a / R1 independent behavior review

Result: **PASS**. No material behavior finding is proposed.

## Immutable subject and authority

- Cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Candidate: `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4` (HEAD checked during this review).
- Latest-fix base: `7cda4e93159ab328de2a74068d5e1978a2590818`.
- Authority: current approved specification revision 18; original TASK-003a body at the cumulative base; prior TASK-003a verdict and validation ledger; amended TASK-003a-R1 and its recorded evidence. Revision 18 overrides the older recommendations for FIND-1/2.
- Read repository agent rules, task-review role, spec-driven development and maintainer guide; applied relevant verification doctrine, grouped-verification design and Bifrost ingestion/query authority. Used CodeGraph before expanding source/caller searches. Did not read this round's sibling reports or receive their conclusions.

This is the behavior review of the complete cumulative task, with latest-fix inspection for closure. Historical implementation PASS claims were treated as evidence to compare with code, not acceptance authority.

## Review findings

### Critical

None.

### Important

None confirmed.

### Suggestions

None: optional hardening and accepted product limits are excluded.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| A1: after-only Trigger, refreshed expiration, six support facts, no producer scheduling SQL and retained attribution/source negatives | `wyrd-spec/src/card/trigger.rs` parses/serializes the exact authored duration and rejects >365 fixed days; `components/cards/service.rs` projects after and frozen sources; `scribe_outbox.rs:755` schedules after ACK; `verifier_executions.rs:200` uses PostgreSQL statement time plus authored interval. Existing execute/enqueue request owners still reserve/stage inputs/results rather than resolving group bindings. | C1 has six exact facts with real Python/Node child SDK contributions to the same bound deployment; P3 also covers raw standalone/inline registration rejection and corrected one-year registration. C7/P8 served schema and C13 ACK/debounce assertions. | PASS |
| A2: visible input/result union, deduplication, deterministic alignment and nulls, late evidence and immutable old values | `verification/groups.rs:270–549`: one successful UNION statement, exact tuple/owner/group predicates; earliest physical row retained by logical ID; selected result greatest ingest/result ID; shared execution ordering; absent payload selection null; `Visible::pin` freezes declarations. No source queue/status lookup appears in the read/pin path. | C8 `available_rows_form_expiring_group_snapshots` reads a result before PG settlement, aligns input-only null, replays a result, forces failed query, then asserts old/new snapshots. C3/C6 and media assertions retain source-value/late/equal-time proof. | PASS |
| A3: digest idempotence, overlapping distinct snapshots, removal of coverage/finality/discovery | `verifier_executions.rs:1034–1086` hashes canonical binding/group/ordered IDs/inputs through existing serde_jcs+SHA-256; INSERT_GROUP_SQL and EXISTING_SNAPSHOT_SQL return the same digest's execution; migration has one snapshot identity index, no outstanding index. Poller serves claimed pending identity without historical discovery. | C9 admits distinct snapshots before settlement and reuses the same digest; C8 replay and C6 overlapping late evidence. Cumulative removals checked against active consumers. | PASS |
| A4: baseline/equal/change/recovery and replay-safe settlement-order reactions | `verifier_executions.rs:295–317,2015–2098`: binding lock, fenced completion, replay short circuit, latest other completed group verdict by settlement time/ID and existing dispatch insertion. Refused/errored executions have no group verdict and cannot become a predecessor. | C9 reversed settlement order/replay/dispatch assertions; C2 refund fixture and SDK webhook assertions retain correction and unchanged-verdict behavior. | PASS |
| A5: original coding without tracing; optional authorized live trace lookup | Group payload extraction preserves JSON values, trace IDs and file/Artifact declarations. Hosted execution remains the existing owner; removing source ceilings did not replace or bypass its query authorization. | Rust C3 retains optional/live trace denial and no-trace checks, artifact/evidence assertions and the corrected Verifier-kind negative. Supplied original C3 parity results and R1 C3 rerun assessed against source. | PASS |
| A6: ordinary 20/21-frame evaluation, failed and empty selection, per-item retry | Removed source-ceiling config, synthetic outcome and runner bypass; `runner.rs:441` runs grouped pinned inputs through ordinary engine. Existing media and queue retry paths remain. | Rust media journey retains 20 and 21 values, failed/empty/foreign/denied cases and source counts; MCP C10 uses same ordinary source path. Supplied original C4/C10 evidence; R1 C10 rerun. | PASS |
| A7: exact tenant/owner/group/version isolation; ordinary ungrouped behavior; mixed runtime conversation | TenantConn/RLS for pending/queue; frozen card+Verifier tuples and owner/group predicate in Sources; no bound-name invocation requirement added to row provenance. Current auth metadata propagation remains. | C1 direct HTTP/ownerless/denied and mixed-runtime exact evidence; C5 independent group and ungrouped retained rows; C8 tuple/pinning seam; SQL tenant tests remain present. | PASS |
| A8: scheduled requirements and bounded staging/neighbor progress preserved | ClaimLoop retains lease/retry/drain ownership. Runtime composes one shared execution semaphore with group preparation and runner. Scribe scheduling completes before consuming submission reservation; ordinary cron has its own loop. | Original C12 scheduled regression retained. R1 Python/TS C11 now registers eligible support first, refuses its grouped source at capacity, retries that same source/group after drain and reads exactly one pinned value, while preserving ungrouped and neighboring tenant progress/count assertions. | PASS |
| A9: thin SDK/projection/stub/schema/MCP/docs alignment | Minimal GroupSnapshot sources projection and after settings agree with Rust, authored Python stub and generated TS. Runtime execution status can omit group/result for truthful pre-admission errors. Architecture/user guide describes terminal owner rejection and accepted ACK/visibility/loss/backlog limits. | Supplied codegen/static lane results; C7/P8 runtime contract; C10 runtime MCP; focused webhook/fixture consumer changes examined. | PASS |
| A10: original functional closure without benchmark claims | Cumulative scenario owners remain; R1 strengthens the previously missing acceptance observations rather than replacing original behaviors. No new benchmark engine or performance verdict in runtime diff. | Original C1–C13 evidence and R1 P1–P8/C2/C3/C5/C10 exact selector/count records. Their assertions were compared with required behavior. REQ-015/AC-009 explicitly pending. | PASS |
| A11: ACK updates, transactional consume/rollback, concurrent arrival preservation, bounded polling and capacity/cron isolation | `ScribeSink::write:1019` submits frames, records group activity, then hands off reserved submissions. Scheduler `hand_off_groups:363` acquires permit before TenantConn/claim, locks one pending row through pin/enqueue/delete/commit and stops tenant on retryable failure. Loop limits 64 tenants/32 items; independent cron loop. | C13 consumes an armed query fault only after capacity/idle passes, observes a real Scribe ACK's blocked SQL upsert during held handoff, asserts fresh pending after commit, >32-item remainder, actual scheduler run cron admission while group query is stalled, and cancellation rollback. | PASS |
| Revision 18: retryable blockage versus true owner/auth failure and observable error | `authority.rs:221–337` preserves typed rejected/unavailable distinction; scheduler `hand_off:428–507` retains missing issuer/SYSTEM/owner identity and retryable read/enqueue failures, records genuine refusal through queue then consumes atomically. SQL status projection does not decode authority for terminal readback. | C13 exercises missing authority, read failure/recovery, lapsed and retired owner, query denial and injected refusal-write rollback. C9 proves authority-free terminal shape never claimed and runnable authority invariant; C1 reads latest binding execution error through SDK. | PASS |
| INV-001–005, no client-owned durable behavior, existing query/queue boundaries | Existing Rust service owners own registration, source resolution, query authority, pinned queue state and settlement; no additional client queue, recovery service, resource budget or platform-specific mechanism. Shared decoders remain in IO-free wyrd-spec. | Source and cumulative diff assessment; supplied boundary/static checks. | PASS |
| Non-goals and accepted limits | No source-terminal wait, visibility barrier, recovery ledger, completeness/high-water model, serialization/adoption gate, custom source ceiling or fairness/quota machinery introduced. | Cumulative source diff and current design/docs. Benchmark qualification deliberately excluded. | PASS |

## Prior stable finding closure

| Stable ID | Disposition | Source-grounded closure |
|---|---|---|
| FIND-TASK-003a-1 | CLOSED under revision 18 | The previous unconditional pending consumption is replaced by Option handoff and one shared consume/commit only after inserted/existing/terminal-error execution. Genuine auth/activity refusal is separated from unavailable infrastructure, and terminal insert failure propagates before consume/commit. Inactive principal identity survives `binding_activity` into DueGroup. No fabricated accepted authority, snapshot or result is produced. Claim/status/cron siblings were checked. |
| FIND-TASK-003a-2 | CLOSED under revision 18 | Shared TriggerDuration boundary rejects all units above exactly one fixed year and preserves accepted text; SQL directly adds the accepted interval, without clamp/fifth argument. Raw standalone and inline writes are tested through real HTTP; exact one-year SQL due-time comparison is immediate, with no year-long wait. Live long-wait fixture is 365d. |
| FIND-TASK-003a-3 | CLOSED | Existing C1 host invokes public Python and TypeScript SDKs in separate child processes against its existing bundle, endpoint and Service credentials/group. Its final exact six-value array makes missing/foreign contributions observable. No Python/Node lifetime is hosted inside Rust. |
| FIND-TASK-003a-4 | CLOSED | C13 now tests scheduler.run rather than only manually joined passes; cron cursor is made due after the query stall is entered and durable schedule execution is required while the group remains blocked. Real turn/flush and pg_stat_activity observation establish ACK/upsert lock behavior. Armed fault carryover observes no reads on capacity/idle passes. |
| FIND-TASK-003a-5 | CLOSED | Both Python and TS C11 pressure the registered eligible support source before retrying the same run/group and reading its pinned group evidence; exact cardinality/ID or value assertions exclude refused attempts. Existing neighbor and ordinary-call assertions remain. |
| FIND-TASK-003a-6 | CLOSED for the cited cause | `Visible::input/result`, local Firsts alias and `Support::activity` now document identity/order/payload or panic contracts. New changed owner documentation inspected alongside behavior; broader standards assessment belongs to the independent repository reviewer. |
| HUMAN-CLEANUP 1 | CLOSED | DueGroup removed unused due_at/card_uid/verifier_card_uid projections; persisted due time, scheduling selection/order, exact source identity and cron DueBinding fields remain active. |
| HUMAN-CLEANUP 2 | CLOSED | Retired runner source-ceiling bypass prose, only-automatic-Trigger claim and MCP media coverage wording removed. Current docs describe current execution behavior. |
| HUMAN-CLEANUP 3 | CLOSED | Six fixture identities/path references and refund webhook changed together with Rust/Python/TS/MCP helpers/callers. Source grammar and originating Verifier names, versions, auth and Operator body remain intact. Invalid fixtures still have explicit rejection assertions and were moved out of valid-fixture sweep. |

## Shared decoder and diagnosis assessment

`InlineableRef<T>` still chooses identity ref, sibling, inline body and path forms in the prior declaration order. The new decoder preserves the embedded spec's error for non-reference objects, making the approved inline duration refusal actionable. Its existing consumers are the shared Card inline positions, not only group Triggers; sample ref/sibling/body/path/over-limit tests and supplied 768-test spec run support that regression boundary. No extra durable representation or reference inventory was added.

`InputSlot` still accepts source strings and typed maps. The visitor directs maps to a remote tagged mirror with deny_unknown_fields, preserving typed-slot errors rather than discarding them in an untagged fallback. The remote derive is checked against the actual InputSlot fields; defaults and required fields match the original variants. This fixes the existing scheduled-samples field-rejection failure at its shared decode owner without weakening the source parser or typed validation. The supplied diagnosis links the failed existing test to the untagged Source fallback; no timeout/assertion weakening was used.

`reserved-typed`, `reserved-source` and `coding-child` are intentionally invalid Verifiers. Relocating them to `fixtures/invalid` follows the sweep's positive-corpus contract; Rust/Python/TS callers still execute the negatives. `coding-child` absence checks now request Verifier, matching its envelope instead of vacuously requesting Service. The fixture failure diagnosis explains variable failing Card selection from the valid corpus. Reported concurrent-Postgres setup collisions resulted in sequential reruns, not product code changes. These are bounded corrections with concrete regression consequences, not unearned parallel mechanisms.

## Open questions

None affecting correctness or scope.

## Verification notes and limits

Independently ran read-only `git diff --check 22335c2c...3d0b301c5` (exit 0) and checked HEAD identity. Inspected the source bodies, producers and sibling consumers above plus supplied original and remediation exact-selector/count evidence. No heavy test, build, Postgres, provider, benchmark or synthetic-load process was started by this reviewer; supplied dynamic results are not fresh executions by this reviewer.

Source remained immutable; this reviewer wrote only this report. Accepted ACK visibility/post-ACK notification loss, outbox/retention loss, continuing-arrival postponement, overlapping settlement-order reactions, SQL backlog and absent global fairness remain product limits, not findings. Benchmarking is still deferred.
