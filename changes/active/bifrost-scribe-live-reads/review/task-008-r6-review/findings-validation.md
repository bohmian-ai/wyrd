# Independent structured Ponytail validation

**Recommendation: PASS. Validated retained finding ledger: empty. FIND-007-13 is CLOSED.**

## Subject and inputs

Immutable candidate `8955e75b71ded9d39daf7649a0c985be0803e266`, correction parent `1a66d8a7b4583c9798c2f1573dc6ab1f3eadb027`; cumulative TASK-007 base `a7582db587c6170a290760f1741673125612b797`, TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`. HEAD was independently checked against that candidate.

Inputs include subject/navigation, cumulative and correction diffs, approved revision-20 specification and original TASK-007/008, remediation chain and R5 verdict/R6 task, all eight current discovery reports, claim comparison and verification. Applied the task-review skill, current user constraints and standing decisions, AGENTS.md, agent rules, reference hierarchy, spec-driven-development and maintainer-style, and applicable Wyrd/Bifrost ownership, tenancy, live-source, shutdown/recovery and reliability authority. No CodeGraph index exists.

This is independent validation after discovery, not another discovery role. No intended verdict was supplied. Every discovery report proposes an empty ledger. I independently checked their central closure claims against actual source and its callers rather than accepting agreement as proof. No focused follow-up is needed: reports have no material conflict or unreviewed reachable path, and all explicitly distinguish the benchmark seating timeout from successful queue qualification. Prior R5 follow-up resolved the prose-versus-runtime distinction; R6 removes the definite obsolete guarantees it identified.

## Source validation and smallest correction boundary

The former shutdown residue producer no longer exists. `ScribeImpl::shutdown` (`scribe/mod.rs:1315–1410`) closes admission, flushes/drains shards, closes/drains accepted persistence and then closes execution lanes and finalizes owners. There is no ready-key sweep. The only production caller of `publish_residue(Drain)` is the separate `flush_staged` workflow (`:2450–2467`), which drains, publishes residue and retires committed Arrow generations. `PersistenceWorker::publish_residue` (`persistence.rs:2036–2059`) resumes retryable claims and uses `ready_keys` through the existing fenced publication owner. Its partition-scoped sibling is test-support behavior, not a hidden shutdown caller.

`replay_wal_async` (`mod.rs:2481–2534`) restores staging, reconciles publication and resumes durable claims before WAL replay and readiness. The retained production lifecycle worker (`wyrd-server/src/app/server.rs:551–579`) calls check_age/publish_due every second; `publish_due_claims` (`persistence.rs:2006–2014`) takes target/dwell-due claims. Corrected prose therefore assigns durable retention to shutdown, restoration to startup, due publication to the tick, and deliberate residue sweeping to explicit flush. Valid admitted-work publication drain remains distinct from forcing below-target residue into publication.

Direct parent-to-candidate diff inspection establishes:

| R6 site | Validated result |
|---|---|
| Reliability guide `analytical-operations-reliability.md:39–46` | Removes “closes residue claims”; preserves admitted publication drain and describes retained staging/startup/tick. |
| `assembly.rs:437,981–985` | Drain documents explicit flush; ready_keys no longer promises shutdown sweeping. Enum value and method body are unchanged. |
| `staging_runtime.rs:1215–1218` | Test rustdoc identifies explicit flush. Test name, attributes and body are unchanged. |
| `wyrd-testing/tests/bifrost/scribe/lifecycle.rs:103–124,148,160–163` | Journey prose describes retained authority, restart and zero-or-complete publication. Assertions, expected values, diagnostics, test identifier and attributes are unchanged. |

All changed lines in the three R6 Rust files are `//` or `///` comments. The fourth site is Markdown. Thus item 1 changes **no executable statement or assertion**. The lifecycle journey still requires exact acknowledged-row readback after restart, permits zero or a complete first-claim published count, restores staging plus WAL after abrupt interruption, and separately proves publication/readback through explicit flush (`lifecycle.rs:163–239`). It does not newly prove a combined restart/tick execution; the separate idle-publication journey and inspected production owner supply the retained clock seam. A focused active-source description search found no remaining definite promise of the removed shutdown sweep. Preserved assertion diagnostic text is executable text expressly excluded from this correction.

The Ponytail ladder stops at existing behavior and prose correction: no helper, guard, runtime change, dependency, renamed enum/test, new check or harness is needed. Deleting explicit flush or ordinary due publication would weaken adjacent behavior and is rejected. Restoring shutdown publication would violate the standing decision and is rejected. There is no remaining remediation to prescribe.

## Prior-finding closure

| ID | Independently validated disposition and evidence |
|---|---|
| FIND-007-3 | **Accepted unchanged under maintainer decision, not newly fixed.** Existing shared hot_stream charge-until-next-pull convention and original TASK-007 disposition remain. No R6 accounting change. |
| FIND-007-4 | **CLOSED.** Residual `provider_error` and `is_tenant_refusal` signatures use imported bare `IcebergError`/`DataFusionError` (`catalog/bifrost_catalog.rs:1801`, `oracle/mod.rs:4330`); latest edits introduce no declaration. |
| FIND-007-5 | **CLOSED.** Original TASK-007/008 integrated proof retains exact package/target/features/environment and selected names/results, with historical/current/deferred attribution. R6 does not alter that proof. |
| FIND-007-6 | **CLOSED.** Managed-envelope test has substantive Panics documentation matching its canonical field-order, absent-tenant and non-null identity assertions (`schema/managed_columns.rs:48–78`). |
| FIND-007-7 | **CLOSED.** Fresh correction and complete cumulative committed-range whitespace checks both exit zero. |
| FIND-007-8 | **CLOSED.** Active live-tail authority (`bifrost-design.md:312–318`) states shallow references, no batch/byte limit and query execution-pool governance; no removed cap is revived. |
| FIND-007-9 | **CLOSED.** Actual shared streamed status classifier preserves Aborted as TenantInvariant (`dispatcher.rs:1813–1821`); shared reader verifies before pruning/decode (`exec.rs:2960–2963`), and leader first-refusal wrapper consumes authenticated context once (`oracle/mod.rs:2484–2519`). No availability conversion bypass remains on the inspected live path. |
| FIND-007-10 | **CLOSED.** Private listener selects stopping IO only for actual Scribe capability (`app/server.rs:678–689`); Oracle-only retains the original graceful runner. StoppingIo polls cancellation independently of body demand and preserves TCP ConnectInfo (`wyrd-tonic/src/server/mod.rs:267–407`). |
| FIND-007-11 | **CLOSED.** Five fallible IO methods retain actual Errors contracts; Connected::ConnectInfo documents forwarded identity (`server/mod.rs:321–404`). |
| FIND-007-12 | **CLOSED.** R5 evidence retains exact classifier and Postgres-wrapped three journey selectors with 1-pass/3-pass attribution; current named source paths remain present. No fresh execution is inferred. |
| FIND-007-13 | **CLOSED.** Every specifically identified false active guarantee is corrected at its description, without changing runtime code/assertions, as proven above. |

The cumulative tenant boundary remains producer-owned: generation encoding checks binding against seal/frozen identity (`parquet_writer.rs:388–405`); sealing stamps authenticated tenant (`:603–611`), claim assembly takes binding tenant, and Forge supplies its binding tenant through the shared writer recipe. File readers compare that independent expected tenant before rows, including metadata-cache hits; missing/valueless/duplicate/foreign tenant metadata refuses without fallback (`footer.rs:44–52,156–168`; `exec.rs:1037–1051,1141–1151`). Follower live_leaf uses MemorySourceConfig, shared HotParquetExec, session partitions and staged lease ownership (`follower.rs:779–869`). None of these producer/consumer boundaries changes in R6. Excluded Postgres columns, chosen public error and no-cap policy stand.

## Authorized TASK-006 correctness-only delta

Read the complete `Bench::overload_queue` body and its metrics/report consumers. Holders launch one scan per observed local slot limit, retain their query stream after its first batch and await the release token. The added bounded seating loop (`bifrost_query_capacity/run.rs:397–413`) polls actual `bifrost_oracle_local_slot_units{kind="used"}` before creating waiters, removing the normal holder/waiter admission race at its producer. The gauge uses the locked aggregate ledger's active-query occupancy and limit, emitted at admission/release (`resources.rs:310–327` and callers). The benchmark scrapes its isolated real server, and `Metrics::sum` selects the exact family and used kind (`server.rs:334–346`). No new abstraction or server policy is introduced.

The loop can expire after QUEUE_SETTLE without observing full occupancy and then proceed; **it is not an unconditional seating guarantee**. This is not a false qualifying result: unchanged `Report::overload` (`report.rs:203–214`) fails fewer than 1000 observed queued requests, an overflow other than queue-full, or missing drain. Scrape failure propagates an error; owned JoinSets drop spawned work. Normal cleanup aborts/joins waiters, observes queue drain, cancels release and joins holders. The correction adds no weakened assertion, manufactured metric or bypass. Correctness-only static inspection found no new blocker; it does not claim fresh queue seating or performance qualification.

## Validated finding ledger and limits

**Explicitly validated empty ledger:** no CONFIRMED or REVISED retained finding; no new stable finding ID. No independently established blocker. No remediation task is needed.

Static only: no cargo, nextest, mise, builds, tests or benchmark execution; no source edit or commit. Supplied prior runtime outcomes remain attributable evidence. Fresh read-only `git diff --check a7582db587c6170a290760f1741673125612b797 HEAD` and `git diff --check HEAD~1 HEAD` both exited zero. Only this assigned report was written. Immutable HEAD remains `8955e75b71ded9d39daf7649a0c985be0803e266`.

**Verdict recommendation: PASS. Findings: none. FIND-007-13 CLOSED; FIND-007-4..12 closure preserved; FIND-007-3 accepted unchanged.**
