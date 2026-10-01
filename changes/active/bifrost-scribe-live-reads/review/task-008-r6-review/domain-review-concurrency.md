# Concurrency and resource ownership review

Result: **PASS**. Proposed findings: **none**.

## Immutable subject and authority

Reviewed candidate `8955e75b71ded9d39daf7649a0c985be0803e266`, parent `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`, cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797` and TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD remained the candidate at the final identity check. No CodeGraph directory exists.

Applied supplied user constraints and standing decisions; AGENTS.md ownership, async, resource and testing rules; architecture/agent-rules.md; spec-driven-development and maintainer-style references; Wyrd design/doctrine; Bifrost design live-resource, recovery/shutdown and admission sections; approved spec revision 20 REQ-005, REQ-014, REQ-015 and INV-004; original TASK-007/008 and R6 remediation. Inspected cumulative owner changes and the latest parent-to-candidate diff. TASK-006's benchmark delta was reviewed for correctness only as expressly authorized.

## Boundary and source coverage

| Boundary | Source evidence | Assessment |
|---|---|---|
| Staged source ownership through asynchronous construction and partition streams | `scribe/hot_source.rs:239–262,655–788`; `oracle/follower.rs:756–874`; `oracle/exec.rs:2667–2741,2886–2940` | The locked staged-source snapshot acquires leases with the selected runs. The live leaf owns the lease during awaited local metadata calls, then the scan retains it in an Arc. Every partition stream retains a clone. Failure or cancellation drops local construction state; last plan/stream drop releases the lease. Staged scan reads use asynchronous OpenDAL filesystem IO on the Scribe owner, not a leader opening another pod's path. |
| Publication versus already-open readers | `scribe/claim_publication.rs:177–246`; `scribe/hot_source.rs:724–788` | Publication advances authority before cleanup waits for existing leases, preventing new staged readers for the published generation. Directory deletion follows lease drain. Cancelling cleanup leaves the staged directory intact. |
| Incremental local and remote fragment lifetime | `oracle/live.rs:364–399,492–585`; `oracle/dispatcher.rs` LiveFrame/NativeOutputTally and transport paths; `wyrd-server/src/oracle/peer_service.rs:244–410,572–608` | Native batches travel without wire encoding; remote output remains framed. The returned stream owns follower execution and source state. Leader cancellation/deadline/drop ends consumption and drops its child. Unexpected incomplete footer remains failure; intentional plan drop needs no footer. No independent live timeout or newly introduced cap appears. FIND-007-3's standing accounting disposition is preserved. |
| Accepted peer connections stalled under HTTP/2 flow control | `wyrd-tonic/src/server/mod.rs:252–407`; `wyrd-server/src/app/server.rs:660–710` | The actual Scribe capability selects stopping IO for the private listener. Cancellation registers/wakes the connection-driver waker through the token future and refuses read/write/flush IO, dropping response owners even when body polling is stalled. TCP ConnectInfo is forwarded for TLS auth. Oracle-only private and public gRPC listeners retain the original graceful runner. This closes the affected FIND-007-10 resource boundary without changing the authorized topology. |
| Shutdown and explicit residue flush | `scribe/mod.rs:1325–1460`; `scribe/persistence.rs:890–1018`; `assembly.rs:434–440,979–990`; lifecycle journey latest diff | Shutdown closes admission, flushes/drains admitted staging work within one deadline, then finalizes retained owners. It does not invoke residue sweeping. Explicit flush retains the separate residue mechanism. R6 changes only comments/rustdoc and guide prose in this path; executable statements, tests, names, enum values and assertions are unchanged. FIND-007-13's concurrency/durability wording now matches this preserved ownership boundary. |

## TASK-006 full-queue correction

Inspected `bifrost_query_capacity/run.rs:369–450`, server process/metrics ownership and `Metrics::sum` at `server.rs:332–345`, report validation at `report.rs:200–225`, and actual gauge emission at `vala-bifrost-redux/src/resources.rs:309–328`. The benchmark launches one isolated server process; the scrape observes that process's aggregate local slot ledger. The `used` gauge is refreshed at admission/release and counts active Oracle slot units, while `limit` is the same ledger's total capacity. The exact family/label selection separates used from limit; it is not the queued-query metric or a per-query counter.

Holders retain their query streams after one batch and await the release token. The new poll waits for all slots to be occupied before creating selective waiters, removing the demonstrated holder/waiter admission race. A 50 ms asynchronous polling interval and existing 60 s bound do not manufacture host load or weaken server admission. Holder cleanup remains release-token cancellation followed by joining; waiter cleanup remains abort/join and observation that the queue drains.

The seating loop can expire without observing full occupancy, then proceeds to the existing queue probe. This is not a false success: the unchanged report requires all 1,000 queue places observed, the exact queue-full refusal, and drain confirmation. A failed/expired holder or missing metric cannot bypass those checks. The change repairs ordering at the benchmark producer and does not change production queue limits, deadlines, assertions, or result acceptance.

## Verification limits

Static review only. No cargo, nextest, mise, build, test, benchmark, commit or source modification was performed. Supplied historical focused lease/pruning/native-stream tests and peer shutdown journey results were read as attributable evidence, not rerun results. R6 wording closure is independently established by parent-to-candidate source diff and the full shutdown/explicit-flush consumers. Runtime benchmark throughput or actual queue seating was not measured in this review.

Final domain verdict: **PASS**. Findings: **empty**.
