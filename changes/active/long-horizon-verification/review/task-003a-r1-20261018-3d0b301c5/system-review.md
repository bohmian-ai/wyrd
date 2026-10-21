# System resilience review

Result: **FAIL**. One material error-classification defect remains within the existing FIND-TASK-003a-1 correction boundary. Prior SYS-001 / FIND-TASK-003a-4 is closed by source and assertion inspection.

## Immutable subject and scope

- Base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Candidate: `3d0b301c51cfe6e50cbbde41b8b2ee3dfc2e9fb4`.
- Latest-fix base: `7cda4e93159ab328de2a74068d5e1978a2590818`.
- Authority: approved spec revision 18; original TASK-003a; prior verdict, validated findings and updated R1; AGENTS/agent-rules; task-review system role; spec-driven-development and maintainer-style; applicable grouped verification, runtime/query and Bifrost architecture.

Reviewed cumulative changed runtime owners and their shared callers, not merely the latest fix: Scribe ACK/activity/submission handoff, pending SQL/claim/consume, authority resolution, group reads and snapshot pinning, execution semaphore/claim loop/runner, settlement, runtime composition and supervision. Read the cumulative file inventory, relevant production and contract diffs, C13 and supplied remediation proof/diagnoses. Inspected shared inline-reference/input-slot decoder changes for their stated source of failure and request-boundary effect; they add no background service or persistence path. This is a system review, not independent exhaustive standards or SDK parity qualification. No current-round sibling report or intended verdict was read.

## Review findings

### Important

**SYS-R1-001 — INCORRECT; existing FIND-TASK-003a-1 remains partially open: a peer/query infrastructure rejection is consumed as an owner rejection.**

- Changed location: `crates/wyrd/wyrd-server/src/verification/groups.rs:63–70`, specifically `error.status() == 403`.
- Violated obligation: revision-18 retryable Wyrd/query/configuration blockage retains pending; terminal consumption is reserved for actual owner inactivity/retirement or owner/query permission denial. R1 explicitly says malformed internal state/runtime configuration faults are Wyrd problems and requires typed causes to survive the existing owners. This is also the architecture's grouped-verification failure boundary (`architecture/wyrd-design.md:1306–1323`).
- Producer-to-consumer proof: `BifrostError::QueryPeerSecurity` has HTTP 403 (`crates/wyrd-spec/src/vala/error.rs:147–156`), but represents peer credential/fence/replay validation, not the owner's table permission. `OraclePeerAuthority::verify_forward_query` rejects a selected peer's stale role fence or expired envelope (`crates/wyrd/wyrd-server/src/oracle/peer_authority.rs:115–157`). `ReadyOracleForwarder::accept` converts that rejection to `QueryPeerSecurity` (`oracle/forwarding.rs:299–306`). The peer handler serializes that exact error through the shared query status owner (`oracle/peer_service.rs:617–620`); the caller restores it through typed gRPC conversion. Its remote response also reports `QueryPeerSecurity` for missing/invalid schema or deadline metadata (`:412–429`). `forward_remote` propagates delivery errors (`:407–411`), and `forwarded_query_error` restores the typed Vala error (`:514–520`). `query/service.rs:215–226` returns non-QueryForbidden Bifrost errors unchanged. `ScheduledQueryCaller::dispatch` uses that same public service (`query/scheduled.rs:208–213`), and `run_with` preserves dispatch failure (`:147–157`). `Reader::run` maps it through `GroupPollError::from` (`verification/groups.rs:164–185`). The new status-only classification turns it into `Denied`; `VerificationScheduler::hand_off` treats Denied as owner refusal (`scheduler.rs:456–480`), inserts an authority-free errored group execution, then `hand_off_groups` consumes and commits pending (`:387–400`).
- Reachable deployed scenario: a group-hosting ingress uses a remote Oracle. Cached ready membership can name its preceding role fence during rolling replacement; envelope delivery is intentionally once, and receiver validation correctly refuses the stale fence. A compatible leader later becoming available repairs the query path, but the pending group is gone. Similarly, an incompatible/malformed peer response is infrastructure failure even when its public code uses 403. With no new source evidence there is no later pending item or historical repair scan. This loses the outstanding verification notification and attributes a cluster fault to the owner. It is introduced by this remediation's broad classification; the prior Read path retained it.
- Smallest correction: at the existing `GroupPollError::from` classification owner, match actual typed caller permission denials (the public query capability's `WyrdError::PermissionDeniedRbac` and table-level `WyrdError::Vala { error: BifrostError::QueryForbidden }`) rather than all status-403 errors. Preserve all peer checks and fail the snapshot closed; `QueryPeerSecurity` must remain a traced Read blockage so the ordinary transaction rolls back. No peer retry, identity fallback, new recovery worker, public error, or status change is needed. `BackgroundAuthority` already independently classifies genuine owner recovery rejection; cron continues using its existing `.ok()` behavior.
- Focused closure: a small pure classification regression should assert capability/table permission denials become Denied while QueryPeerSecurity remains Read and retains its stable reason. Extend the existing C13/fault owner only as needed to return this existing typed peer error at the snapshot seam, assert pending survives with no errored execution, then healthy handoff succeeds without another source write. Keep the present explicit permission-denial case terminal and its no-Operator/no-repeat assertions. This proves retention without weakening peer security or requiring rolling-deployment machinery.

### Critical

None.

## Deployed path and failure-path assessment

`wyrd-server` owns public and peer listeners, authentication and shared lifecycle; Scribe/Oracle may be local or peer capabilities (`architecture/bifrost-design.md:15–47`). Runtime composition requires the operator pool; hosted blocks compose group preparation, and the runner additionally requires a route to Scribe (`verification/mod.rs:361–455`). One semaphore is created from existing execution_slots and passed to both preparation and runner. No new process, budget, queue or health dependency is introduced.

| Path/fault | What stops and remains available | Surviving state and resumption | Assessment |
|---|---|---|---|
| Scribe unavailable or pending-upsert SQL fails | That bounded outbox slice retries; originating audit/verify remains nonblocking. Scheduling finishes before submission reservation consumption. | Identical Scribe batch IDs absorb replay; pending activity transaction commits atomically. Accepted outbox loss remains explicit. | Source: `scribe_outbox.rs:750–798,1013–1066`, boot composition supplies SQL owner. No new finding. |
| Preparation capacity exhausted | Group handoff acquires nothing and performs no claim or snapshot; runner owns its permits until work returns. Cron has a separate loop/future. | Pending stays durable; next normal pass can acquire capacity. | Source: `scheduler.rs:158–213,366–400`; `claims.rs:247–290`; `verification/mod.rs:382–396`. |
| Snapshot timeout/degradation/truncation or malformed data | No partial execution is enqueued; tenant pass ends. Other tenant passes and cron continue. | Transaction drop retains pending; ordinary later polling retries. | `groups.rs:133–150,164–192`, `scheduler.rs:387–395,464–466`; see SYS-R1-001 for the 403 exception. |
| Missing issuer/SYSTEM or enqueue readiness refusal | No authority is fabricated; no provider runs; tenant pass ends with warning. | Pending stays; configuration/target restoration permits later handoff. | `authority.rs:253–289`, `scheduler.rs:439–475`. |
| Genuine inactive/retired owner or caller permission denial | That pending group receives one terminal error; no Verifier/provider, verdict, result or Operator. | Error and consumption share one tenant commit. Latest binding execution then authorized status read exposes it; new source activity is required to schedule again. | `scheduler.rs:442–497`; queue `refuse_group`/`INSERT_GROUP_ERROR_SQL`; terminal row is not eligible for CLAIM_SQL. |
| Error-record insert fails | No partial terminal record and no consume commit; other tenant work continues. | PostgreSQL rollback retains pending, later ordinary pass retries recording. | `scheduler.rs:478–489`; C13 injected trigger rejection. |
| ACK arrives during held handoff | Background activity upsert waits on the exact pending row, rather than request-side SQL or provider execution. | Successful handoff delete/commit lets the upsert create a fresh pending item; rollback lets it update retained pending. | `RECORD_GROUP_ACTIVITY_SQL`, claim/consume transaction, C13's actual Scribe path and observed SQL wait. |
| Cancellation before commit / process death | Preparation future drops transaction and permit; query stream owns its ordinary drop cleanup. A selected commit is awaited. Runner interruption uses ordinary leases. | Uncommitted pending survives; committed execution remains in durable queue. Crash between ACK and scheduling commit can lose signal by accepted policy. | `scheduler.rs:386–405`; query stream lifecycle/admission drop owners; `claims.rs:302–325`; runtime supervision documentation/source. |
| Capability task panic | Runtime marks that capability down, backs off and respawns same owner; shared HTTP and sibling capabilities are not deliberately crashed. | SQL pending/queue remain; leases reclaim interrupted runner work normally. | `verification/mod.rs:245–296`. No new process-wide fail-closed policy. |
| Distinct snapshot overlap / repeated settlement | Preparation releases transaction and permit before evaluation; normal runner claims, leases and retries. | Digest conflict identifies same snapshot at any status. Short binding lock orders completion, comparison and dispatch creation; replay bypasses recomparison. | Queue `INSERT_GROUP_SQL`, `EXISTING_SNAPSHOT_SQL`, `LOCK_GROUP_BINDING_SQL`, `complete`; runner Grouped dispatch. Accepted overlap/backlog semantics preserved. |

The terminal-row migration permits absent accepted authority only on pre-admission errored group rows; the production insertion leaves inputs/routes/snapshot/result/verdict/lease absent, and claim decodes authority only after selecting runnable status. Status readback does not manufacture a GroupSnapshot. This assessment does not substitute for the independent data review's full SQL constraint audit.

## C13 and prior-finding closure

Prior SYS-001 was missing composed scheduler proof, not a demonstrated production concurrency failure. Current C13 (`pg_verification_groups.rs:648–1034`) closes that scoped issue:

- A query fault armed before full capacity survives the starved pass and is consumed by the next real read; the same mechanism proves idle post-handoff passes issue no read (`:685–724`).
- A real source turn enters Scribe while the pending row is claimed; SQL activity observes its background INSERT waiting on that row before snapshot/handoff commit; flush completes and fresh pending remains (`:753–800`). This now exercises ACK-to-upsert, not only a direct second SQL writer.
- Two tenant transactions prove SKIP LOCKED exclusion, and 33 new groups plus the prior arrival prove 32-item pass bounds and remainder (`:724–736,802–818`).
- It invokes actual `VerificationScheduler::run`; after the group query signals its schema stall, it makes a real schedule due and observes a durable schedule-origin execution before cancelling the stalled group (`:835–892`). This distinguishes concurrent cron progress from a pre-stall occurrence.
- Cancellation returns and pending remains. Unconfigured authority later recovers without another source write; failed terminal insertion leaves neither consumed pending nor partial error, then succeeds; inactive/retired and actual query-denied owners are consumed once (`:894–1034`).

No healthy-path-only claim is used to infer the new error-classification recovery behavior. The missing peer-error distinction in SYS-R1-001 is not covered by these existing negative cases.

One residual test limitation is explicit in C13: its daily-cron setup has a 30-second UTC-midnight claim-delay window (`:846–855`). That test timing limitation does not demonstrate a deployed service failure and is not a separate system-resilience blocker here.

## Verification notes and limits

Assessed R1's recorded P1/P2/P3 and static results against source assertions. Recorded diagnoses for inline untagged decode, typed input fallback, invalid fixtures in valid corpus and concurrent managed-Postgres runners are consistent with the changed owners and tests; none licenses weakening runtime admission or query failure handling. The new decoders affect authored parse errors and retain the existing closed ref/input shapes; no availability machinery was added.

Ran no tests, builds, Postgres, providers, benchmarks, outage injection or deployment restart. No dynamic claim is made for stale-fence recovery; the finding follows the exact typed production error path above and calls for focused existing-owner proof. Did not requalify every SDK journey or replace specialist security/data/standards coverage. Source was not edited; this reviewer wrote this report only in the assigned review directory. HEAD was checked against the candidate.

Accepted ACK/query-visibility divergence, post-ACK scheduling-signal loss, outbox/retention loss, indefinite debounce, overlapping settlements, durable backlog, lack of global fairness/quota and pending benchmarking remain excluded from findings.
