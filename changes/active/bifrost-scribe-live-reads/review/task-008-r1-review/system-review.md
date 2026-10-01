# System-resilience review

Result: **PASS**. Material proposed findings: none.

## Subject, authority and limits

Fresh independent system review of candidate `23eafa368bca19208faf8311eb7b5421e3660b38`, immediate parent `6e7add054e33701ca5ecb52a5c859948b15161a3`, original TASK-007 base `a7582db587c6170a290760f1741673125612b797`, and TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Reviewed cumulative TASK-007/008 runtime scope and the immediate R1 correction. Unrelated TASK-006 benchmark/lifecycle scope remains excluded as instructed.

Authority: approved spec revision 20 REQ-014/015, AC-016/017 and INV-001–008; original tasks and R1 remediation; AGENTS and agent rules; spec-driven-development and maintainer-style references; Bifrost design system boundary, durability/visibility and resource rules; analytical-operations-reliability reference. The supplied navigation was expanded by current-source caller searches. No CodeGraph index exists.

Static only: no tests, source edits, commits, Postgres wrappers or full lanes were executed by this reviewer. The task exact-proof table and `review/task-008-review/final-named.log` are supplied execution evidence, not my runtime proof. Full Bifrost and capacity benchmark remain caller-owned. FIND-007-3 is accepted unchanged; Postgres tenant columns are excluded; the approved error is `WYRD_VALA_500_QUERY_TENANT_INVARIANT`.

## Deployment and source coverage

`wyrd-server` owns public/private peer listeners and lifecycle. Vala Scribe owns local rows, staged files and publication; Oracle owns admission, pinned published sources, distributed workers and live routing; Forge owns promotion/rewrites. These roles may share one process. A source integrity or resource fault must refuse its query/work owner rather than crash the common process.

| Changed path | Producer-to-consumer source evidence | Assessment |
|---|---|---|
| Scribe staged source | `scribe/tail_rpc.rs:563–585` opens the bounded snapshot and lease, excluding served generations. `oracle/follower.rs:780–861` builds the local memory/shared `HotParquetExec` leaves; async filesystem `staged_run_io` is at `:719`. `oracle/exec.rs:2922–2982` loads retained metadata then prunes/decodes. | Scribe opens its own files. The leader dispatches the signed fragment rather than opening another pod's path. No second decoder or blocking reader is introduced. |
| Partitions and memory ownership | `follower.rs:789–804,840–843` retains session partition count for sparse/empty memory and staged sources. `exec.rs:2889–2900` transfers lease and metadata cancellation guard into each stream; `:2980–2983` uses admitted scan governance for decoded batches. | Existing grant and source lease remain authoritative. The accepted shared Arrow accounting convention remains unchanged. |
| Native local live path | `wyrd-server/src/oracle/peer_service.rs:343–392` builds follower resources, records checked native totals and emits completion after drain. `oracle/live.rs:750–845` reconciles totals/fingerprint and terminal ordering. | Arrow passes directly. Native branch performs no IPC encode/decode or payload-hash update. Stream failure cannot emit successful completion. |
| Remote sibling | `peer_service.rs:545–585` encodes through `AttemptEncoder`; `live.rs:773–824` retains wire schema/batch/hash/footer validation. | Native counters do not replace remote encoded-byte/digest conventions. |
| Footer proof | `parquet/footer.rs:46–61,157–177` refuses missing/duplicate/foreign fields. `exec.rs:1024–1039,1109–1148` proves published/hot/staged metadata before decoding; `:1424–1429,1565–1568` refuses encrypted/unproven published paths. | Cache hits still prove tenancy. Missing proof is integrity failure rather than degraded availability or compatibility. |
| Footer production and Forge output | `scribe/parquet_writer.rs:388–403` checks binding/seal authority; `:603–627` stamps then inspects schema/object/tenant. `claim_assembly.rs:249–269` supplies authenticated tenant. Forge executor `:198–202,285–289` passes `binding.tenant` to policy `:255–276` and writer properties `:119–131`. | Existing authenticated binding owns proof. No new durable authority, publication fence or retry identity appears. Promotion preserves bytes; rewrite records the same tenant. |
| Failure propagation and audit | `peer_service.rs:649–680` distinguishes loss/capacity/fault/tenant errors; status `:630–639` and `oracle/dispatcher.rs:1785` preserve the remote tenant class. `live.rs:488–599` only degrades eligible pre-row availability loss. `oracle/mod.rs:2486–2526` emits one leader security event; `query_stream.rs:1270–1290` preserves the stable terminal. | Tenant faults fail even before rows. No changed refusal branch crashes the server or becomes eligible retry/degradation. |

## Failure and recovery assessment

| Credible failure/interruption | Boundary and sibling availability | Recovery/proof |
|---|---|---|
| Foreign/missing footer | Refuses the reader before that file decodes rows, then fails the owning query. Shared process and unrelated queries/services stay available. | No retry bypasses integrity. Correct/recreated bound data becomes readable; proof also executes on metadata hits. Footer/hot refusal and distributed `count(*)` refusal tests are recorded PASS. |
| Staged metadata IO, schema/projection or resource fault | Local follower faults are terminal rather than interpreted as absent live rows. Query-owned stream/grant cleanup releases the work. | Existing owners clean up; no new process panic, restart or retry amplification. Peer-service failure-class test is recorded PASS. |
| Known live source lost before/after rows | Closed availability loss before rows degrades; loss after rows fails. Selected analytical-peer failure remains terminal. | A later query can discover a new roster; this attempt does not silently become successful through retry. `selected_peer_failure_is_terminal` is recorded PASS. |
| Drop/cancellation/deadline/LIMIT | Drops owning fragment and scan, releasing staged lease and cancelling metadata work. Typed deadline failure remains query-local; an unneeded early-dropped child owes no footer. | Publication retains leased runs until reader ownership ends. Immediate-drop and open-read-across-publication tests are recorded PASS. Existing DataFusion coalescing teardown is not a new synchronous teardown guarantee. |
| Corrupt native completion or unexpected EOF | Reconciliation failure or missing needed completion prevents successful terminal acceptance even if internal rows flowed. | Query-local failure; no IPC/hash fallback. Native reconciliation, incremental/footer and empty-executor tests are recorded PASS. |
| Shutdown with pending peer reservation | `Oracle::shutdown` closes admission and shuts the worker, then drains pending reservations before its report (`mod.rs:2877–2902`; `dispatcher.rs:449–457`). | Capacity for unactivatable graphs is released without altering ingestion/publication authority. Immutable-fenced-destination restart journey is recorded PASS. |
| Restart across promotion/rewrite | New footer identity does not change WAL ACK, claims, fenced publication, CAS/reconciliation or reader protection. | Forge promoted rewrite/recovery and hot promotion journeys are recorded PASS. This review makes no universal crash-injection or performance claim. |

## R1 regression and prior findings

The immediate source correction imports existing concrete `DataTenantId`, `ArrowReaderMetadata` and test `WriterProperties`, uses identical bare declaration names, removes a duplicate test import, and documents the schema test's panic conditions. No function body, constructor, feature gate, lifetime, error mapping, dependency, deployment, synchronization or durable behavior changes. No R1 runtime or recovery regression is present in this scope.

FIND-007-4's named declarations retain the same concrete identities through imports. FIND-007-6's schema assertions remain unchanged and its panic doc describes their actual ordering/nullability conditions. FIND-007-5's exact-proof section supplies package/target/features/selectors and current results supported by the retained log, including the failure/recovery checks above. Final artifact compliance remains the standards/evidence reviewers' scope; these prior hypotheses leave no system finding.

## Finding ledger

Explicitly empty. **PASS** for system resilience within the stated static and supplied-proof limits.
