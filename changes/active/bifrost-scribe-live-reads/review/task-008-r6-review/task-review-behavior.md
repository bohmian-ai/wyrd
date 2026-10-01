# Independent behavior review

**Result: PASS. Proposed material findings: none.**

## Subject and limits

Candidate `8955e75b71ded9d39daf7649a0c985be0803e266`; correction parent `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. Reviewed original tasks, spec revision 20 REQ-014/015 and AC-016/017, remediation chain and R5/R6 inputs, repository rules, applicable design/doctrine and Bifrost lifecycle authority, actual diffs and current source. Discovery was independent of sibling R6 reports.

Static only: no cargo, nextest, mise, builds, runtime tests, benchmark or commits. Only this report was written. Supplied test outcomes are attributable evidence, not fresh execution. HEAD was rechecked at the candidate. Both `git diff --check 97e9c156 HEAD` and `git diff --check HEAD~1 HEAD` completed with exit zero. Capacity qualification remains caller-owned; the authorized adjacent TASK-006 change is inspected only for correctness.

## Acceptance matrix

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-007 shared staged/hot/published Parquet scanning; remove custom staged decoder and per-batch predicate compilation | `follower.rs:779–868` builds the existing `HotParquetExec`; `tail_rpc.rs:572–601` returns shallow rows/runs/lease; cumulative diff removes staged decoder and `retain_signed`; `exec.rs:2960–2990` uses metadata, row-group selection and page selection before decode | Supplied exact staged pruning test, follower suite and selective distributed journey | PASS |
| Memory uses engine source, closure projection and signed predicates | `follower.rs:790–814,859–869` uses `MemorySourceConfig`, `project_batch` and one compiled conjunction above memory/staged union | Supplied nonzero-ordinal projection/filter and source-shape proofs | PASS |
| Session partition count, including sparse cuts; Scribe-local IO and query governance | `follower.rs:790–799,842–851` uses session target partitions and follower runtime memory pool; resolver is local to Scribe; staged metadata keys retain authenticated tenant/table/epoch; staged lease moves into scan | Supplied shape, dropped-scan release and open-read/publication tests | PASS |
| Native Arrow without IPC/hash while remote wire/footer rules remain | `LiveFrame::Batch/Complete` native producer/decoder path; `live.rs:742–759` counts Arrow rows/bytes and reconciles native completion; wire branch still validates schema/hash/terminal | Supplied incremental-frame, completion and peer-service proofs | PASS |
| TASK-008 remove row tenant from schemas/writes/projections and delete per-row tenant filter/tripwire/codec | Cumulative managed-column/schema/write/projection diff removes row identity; `schema/managed_columns.rs:48–95` proves canonical envelope absence; provider deletion and codec removal; memory source is seal/assignment-bound | Supplied envelope, writer, closure and write/read journey proofs | PASS |
| Authenticated footer producers, including claim assembly and Forge | `parquet/footer.rs:30–50,80–126`; `claim_assembly.rs:88,266` carries tenant to writer; Scribe writer obtains seal/table binding; Forge executor passes `self.binding.tenant` into both rewrite constructors | Supplied writer identity, claim assembly and promotion/rewrite/recovery proofs | PASS |
| Missing/foreign footer refused before rows, including cache hits and count aggregation; no fallback | `exec.rs:1040` verifies mandatory published loader result before returning metadata; `1141–1151` verifies hot/staged metadata; `2960–2963` applies this before row groups/pages/decode; `footer.rs` rejects missing/duplicate/foreign keys | Supplied focused footer/hot negative tests and local/distributed COUNT refusal journey | PASS |
| Remote streamed tenant refusal stays typed, fatal and audited once | `dispatcher.rs:1813–1821` maps Aborted to TenantInvariant; `live.rs:641–642` preserves public error; leader `mod.rs:2477–2519` audits first typed refusal through retained canonical owner; terminal mapping preserves code | Supplied classifier and remote staged footer refusal journey | PASS |
| R5 listener correction preserves Oracle-only/public graceful behavior and releases parked Scribe fragments | `app/server.rs:678–689` selects stopping IO only for existing Scribe capability; `wyrd-tonic/server/mod.rs:310–404` token-aware IO independently wakes socket driver and forwards ConnectInfo; public runner untouched | Supplied blocked-window/lost-Scribe journeys and peer 11/11 | PASS |
| R6 descriptions reflect retained staging, restored authority and later due publication; explicit flush preserved | Reliability guide:40–45; `assembly.rs:437,981–985`; staging test rustdoc:1215–1218; lifecycle journey:103–124,148,160–163; compare shutdown `mod.rs:1315–1400`, recovery:2480–2506, flush:2453–2459, tick `app/server.rs:551–579` | Actual correction diff and current producer/caller source; no runtime proof needed for wording | PASS |
| R6 changes no executable statement or assertion | Latest source hunks in assembly, staging runtime and lifecycle are exclusively `///`/`//` replacements. Test name, attributes, body, assertion condition and diagnostic strings are unchanged. Reliability guide is prose | Direct parent-to-candidate diff inspection | PASS |
| Authorized TASK-006 seating change correctness | `run.rs:397–413` polls existing actual occupancy gauge before waiter spawn; `resources.rs:310–327` emits locked-ledger used/limit; LocalServer metrics reads separate server process; report:203–214 still requires 1000 queued, queue-full overflow and drain | Static call/data-flow proof; no claim of fresh benchmark success | PASS |
| Standing exclusions, immutable source, static restriction | FIND-007-3 unchanged; Postgres tenant columns excluded; existing public error retained; no cap added; shutdown does not force residue sweep | Diff/source and HEAD check | PASS |

## Caller-to-result and closure assessment

The live read caller authorizes a Scribe assignment, opens local tail authority and leases unserved staged sources. The resolver projects memory and constructs the shared Parquet leaf under its follower session. File metadata is proven before its reader can prune or decode. Therefore pruning cannot bypass the footer proof, and cached metadata cannot bypass it either. A lazy remote refusal travels from file proof through the peer Aborted status, shared streamed conversion and leader typed error/audit owner; it is not a pre-row availability omission. Native local frames preserve incremental delivery and terminal accounting while avoiding wire encoding.

Shutdown stops admission, flushes/drains generations and persistence, then closes execution lanes. It has no ready-key residue sweep. `flush_staged` remains a separate operation that calls `publish_residue(Drain)`. Recovery restores staging/reconciles/resumes before WAL replay and readiness; the lifecycle worker performs ordinary due publication. The corrected reliability guide retains admitted-work drain, which is valid and distinct from forcing all residue. `ready_keys` now documents the explicit-flush consumer accurately. Lifecycle prose matches its existing zero-or-complete published count and exact readback, without claiming that the test proves every row was published at shutdown. Its later explicit flush still proves publication of restored rows; the separate idle journey covers clock-driven publication.

No new abstraction, dependency, compatibility path, configuration or test harness was introduced by R6. Reusing existing descriptions and the benchmark's existing gauge is the smallest sufficient correction.

| Prior ID | Current disposition and independently inspected evidence |
|---|---|
| FIND-007-3 | **Maintained accepted-unchanged disposition, not newly fixed.** Existing shared scan charge lifetime remains the operator convention documented in TASK-007; no R6 memory-accounting change. Exact buffer-lifetime redesign is excluded by maintainer decision. |
| FIND-007-4 | **CLOSED.** Previously residual `provider_error` and `is_tenant_refusal` use module-top `IcebergError`/`DataFusionError` imports and bare signatures (`bifrost_catalog.rs:10,1801`; `oracle/mod.rs:16,4330`); R6 adds no declarations. |
| FIND-007-5 | **CLOSED.** Original task integrated-proof section records package/target/features/environment and exact names, one selected pass each, with historical/current/deferred distinctions; retained R1 verdict is corroboration. |
| FIND-007-6 | **CLOSED.** `schema/managed_columns.rs:48–60` has substantive Panics documentation matching the canonical-envelope and identity assertions; R6 leaves body unchanged. |
| FIND-007-7 | **CLOSED.** Both actual cumulative and correction-range whitespace checks exit zero, including previously cited evidence artifacts and TASK-006. |
| FIND-007-8 | **CLOSED.** Active Bifrost live-tail authority:312–318 describes cap-free shallow references and query execution memory, not removed count/byte caps. |
| FIND-007-9 | **CLOSED.** Shared streamed Aborted conversion preserves TenantInvariant (`dispatcher.rs:1815`) through leader fatal error and first-refusal audit; recorded classifier and remote journey provide focused proof. |
| FIND-007-10 | **CLOSED.** Scribe capability branches at private listener owner (`app/server.rs:678–689`), so parked Scribe connections get stopping IO while Oracle-only keeps original graceful runner; supplied blocked-window/lost-Scribe and peer proof retained. |
| FIND-007-11 | **CLOSED.** Five fallible StoppingIo methods now document actual socket/cancellation Errors and ConnectInfo is documented (`server/mod.rs:321–404`), matching unchanged behavior. |
| FIND-007-12 | **CLOSED.** R5 task evidence contains exact dispatcher classifier and Postgres-wrapped three journey selectors/results; names match present source. Attribution remains supplied proof. |
| FIND-007-13 | **CLOSED.** All specifically identified active obsolete shutdown guarantees replaced. Source-path search found no remaining definite shutdown/residue-sweep promise across architecture, Scribe and Scribe journeys. All R6 runtime-file hunks are comments; assertions are identical. |

## TASK-006 correction boundary

The holders retain their open query streams while awaiting the release token. The added loop observes the same locked-ledger gauge used to report the actual execution limit, rather than assuming spawned tasks have acquired slots. This removes the reported normal-path race: waiters are spawned only after the observed occupancy reaches the limit on successful seating. Gauge scrape failure returns an error and JoinSet ownership drops spawned holders. A 60-second seating timeout can fall through into the existing measured overload step, but cannot manufacture a passing result: queue occupancy, overflow refusal and post-disconnect drain remain independently required by the report. This bounded fallback is not an executable acceptance regression; no timeout extension, new hook or capacity claim is proposed.

## Findings

Empty proposed finding list. **PASS** within the requested static confirmation scope.
