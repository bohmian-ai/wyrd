# Persistent data remediation review

Result: **FAIL**. FIND-TASK-004-6 is closed. FIND-TASK-004-7 retains one bounded required-proof gap, source-local ID **DATA-R1-01**. No production data-loss defect is asserted.

Immutable subject: `/home/thorrester/Documents/GitHub/wyrd`, prior candidate `ed82f63bf35ca124c0a2980c160f0d64535eebd1` → candidate `0c2bfe70d5186dff4dc224c7e402b57c24f354d9`; original base `9692bfd06c6bdf2585e88c791884a90669a5dc26`. HEAD matched the candidate during this inspection. Only this assigned report was written.

## Scope and authority

Read the round's `scope.md`, prior domain/system reports, final validated F6/F7 ledger, remediation diagnoses and appended execution evidence. Inspected the actual incremental diff and affected producers, row/key owner, caches, derived batch identity, consumers and recovery lifecycle. CodeGraph was consulted before code searches; its broad-name/path queries returned some unrelated matches, so source inspection filled the specific gaps.

Authority is root AGENTS.md, `changes/spec-driven-development.md`, `agent_docs/{index,rust,maintainer-style}.md`, TESTING.md, `architecture/README.md`, Bifrost design's canonical metrics/index contract (lines 367–418), and approved spec revision 27 REQ-058/059/060 and AC-037/038. The caller-authorized accepted-context reconciliation is carried forward. No nested AGENTS.md was found under the affected Vala tree. Original TASK-004, superseded TASK-022 and Bloom revision-task references remain acceptance context; the current remediation and revised spec own these index/recovery obligations.

Unchanged fingerprint encoding, tagged-WAL compatibility, Bloom/fork/permission behavior and stable attribute-derived series-id acceptance carry forward from the prior domain report. The incremental production changes only widen the series-row key and its derived batch identity; they do not replace those unchanged mechanisms. UI math and other repository rules are assigned elsewhere.

## Acceptance and closure matrix

| Obligation | Implementation / independent source assessment | Verification evidence | Result |
|---|---|---|---|
| Every admitted semantic context is represented without changing stored series identity | `tables/metrics/series.rs:504–527,567–598`: context contains all copied promoted dimensions plus subject/run/workload/principal correlation; SHA-256 over Arrow row encoding distinguishes null/value and field boundaries; the point's existing `series_id` is copied unchanged | New accepted-context persistence case checks seven contexts, native Runs within/across batches, accepted environments/workloads, unchanged IDs and repeated-context suppression; root independently ran it successfully | PASS; F6 closed |
| Same key reaches all suppression owners | Local `asked` in `series_rows`, cross-slice `admitted` and process `SeriesSeen` all use the same widened `SeriesKey`; tenant remains outside that key in process storage | Source trace plus cache tests; reported owning 17-case lane passed | PASS |
| Retry identity distinguishes selected contexts | `scribe/series_index.rs:132–145` uses versioned v2 digest over original points batch id and ordered hour/id/context tuples | Existing identity test extended to distinguish changed context; reported owning lane passed | PASS |
| Cost remains series/context scaled and cache bounded | Request id and admission instant are excluded; `SEEN_CAPACITY` remains 262,144; insertion follows successful derived admission; overflow forgets, never suppresses an unwritten key | Repeat assertions and bounded-cache test; source trace | PASS |
| Native and OTLP semantics, accepted stamping, scoped discovery remain intact | Existing `Bifrost::write_batch` → Gate → Scribe native Run extraction preserved; OTLP `RecordCorrelation::extract` preserved; shared stamping still owns accepted dimensions; index queries apply their predicates to matching context rows | Accepted-context persistence case; affected Metrics journey adds the same OTLP series under two Service releases and reads each workload-filtered rail; supplied journey passed | PASS |
| Index durable before point dispatch; failure prevents points | Unchanged `ingress.rs:803–814` awaits `admit_series_first` before dispatch; `series_index.rs:198–201` remembers only after ordinary derived admission succeeds | Existing refusal/retry and repeat cases retained; source trace | PASS |
| Required actual interruption/restart/read proof at durable, unanswered boundaries | New tests retain WAL and invoke ordinary replay on a new epoch, but `crash_once_durable` observes readable COMMIT rather than completed sync or reached post-sync fault | Root independently ran the exact two recovery cases successfully; they recover 1 series/0 points then retry 4 points, and 1 series/8 points respectively. These counts do not identify the interruption boundary | FAIL; DATA-R1-01 / same F7 |

## Producer-to-consumer integrity trace

The existing native SDK write door (`crates/shared/wyrd-client/src/bifrost/facade.rs:760–797`) transmits the canonical batch. Gate `dispatch_native_frame` resolves Card scope and accepted dimensions and awaits Scribe; `execution_lanes.rs:1208–1226` lifts independent native Run correlation, then `stamp_correlation_columns` and `stamp_dimensions` install the accepted context. OTLP `metrics/projection.rs:763` derives correlation from retained attributes, and Gate can still supply independent accepted workload/environment values. These sibling paths explain why the context is separate from the attribute-derived series id.

The candidate derives the context key from already-prepared/stamped points, after these producers have resolved values. Its context list covers every copied promoted field in `SERIES_FIELDS` and every independently variable semantic managed correlation field. The remaining series fields/attribute sets are already fixed by stable series-id input. Per-admission request/time fields are intentionally excluded. The same complete key flows through local selection, cross-slice selection, process suppression and deterministic derived admission identity. There is no new downstream fallback or alternate stamping owner.

The existing consumers remain valid: `observe.ts::metricNamesQuery` groups matching index rows by metric meaning; `seriesLabelsQuery` filters before choosing newest row per series id, and its returned attribute sets are fixed for that id; `groupKeysQuery` reads those same attribute sets. Chart identities/totals remain series-based. Additional context rows therefore restore scoped coverage without changing stored measurement identity. The new context discriminator reuses installed Arrow row encoding and SHA-256; a second schema/table/service or hashing implementation would be unnecessary.

## Recovery and lifecycle assessment

The new tests use actual WAL files in one retained TempDir, a fresh `WalWriter` and Scribe in epoch 2, and ordinary `replay_wal_async`. Assertions are nonvacuous: the before-response case requires eight recovered points, including four from the interrupted batch. The first case also proves the writer retry returns four points and does not add a second series row. Fresh in-memory owners and epoch-filtered tail reads avoid accidentally querying the old memtable. This is substantial improvement over the prior healthy/refusal-only proof.

The fixture has no SQL fence or persistence tier. Ordinary replay reconstructs rows and the no-persistence branch retires replay input without staging it (`shards.rs:1648–1661`). Consequently it establishes one restart only; this disclosed fixture limit is not a separate defect or a requirement for another restart. Production persistence/fence behavior is unchanged in the incremental diff. No new cross-table transaction or wider failure matrix is justified.

Dropping Scribe is also not a synchronous stop barrier. Its shard runtime retains ordinary JoinHandles, not abort-on-drop handles. Dropping its pressure senders eventually causes the shard run loop to exit (`shards.rs:1446–1466`), but an awaited write/sync inside `process_group` may finish first. WAL IO work is detached onto the retained Rayon pool (`execution_lanes.rs:2200–2225`); dropping its async waiter does not cancel an already-started filesystem operation. This matters to the specific boundary proof below; it is not an independent production lifecycle regression.

## Material proposed finding

### DATA-R1-01 — Readable COMMIT does not establish the required post-sync interruption

**Carry forward FIND-TASK-004-7; MISSING required proof.**

**Violated obligation:** spec revision 27 AC-038 and the independently validated remediation explicitly require interruption after durable series/before points, plus points durably admitted before response, followed by ordinary retained-volume recovery. The candidate claims precisely those post-sync boundaries in the new tests' rustdoc and appended evidence.

**Location:** `crates/vala/vala-bifrost-redux/src/scribe/tests/scribe_persistence_path.rs:1539–1573` (`crash_once_durable`), used at `1623–1630` and `1688–1695`.

**Source-proven gap:**

1. The helper stops when `replay_wal_directory` can decode a terminal COMMIT from the directory. It observes neither `sync_data` completion nor consumption of the installed post-sync fault.
2. `ShardOwner::append_and_sync_batch_commits` first submits `WritePrepared` and awaits that result (`shards.rs:3797–3826`), then separately submits/awaits `SyncWal` (`3833–3836`, `4483–4502`). A complete, readable COMMIT therefore exists before the sync is even submitted or completed. Filesystem readability is not the durable-admission boundary the test claims.
3. `crash_once_durable` may observe that earlier state while the writer task is suspended on the completed-write notification or the sync. It drops the outer admission future and returns. No assertion proves `take_post_sync_failure_for_test` in `process_group` (`3287–3296`) has actually run. Installing a future fault is not observing that fault.
4. Dropping `(scribe, wal)` does not synchronously terminate retained shard/filesystem owners. Already-started IO can finish after the nominal stop; subsequent ordinary replay still recovers the bytes. Thus both tests can pass and return the asserted recovered counts without establishing their advertised interruption after durable commit and held acknowledgement.

**Observable consequence and materiality:** the new recovery proof may stop at an earlier write boundary and allow work to finish after the nominal crash. Green execution then leaves the explicit post-sync interruption criterion unproven. This is the same bounded missing-proof finding retained by the prior ledger, not evidence that production loses points, nor a reason to widen the crash matrix.

**Smallest testable correction:** keep the two existing cases, retained directory, new epoch and ordinary replay. Observe that the existing post-sync injection has actually reached its held, unanswered owner before stopping the old ownership graph; stop it without graceful flush before constructing the replacement. Reuse the existing post-sync fault and Scribe abort/lifecycle facilities; add only the minimum test-support observation needed if current inspection cannot expose that boundary. Preserve the existing recovered-row/retry assertions and record exact focused executions. No production durability change, subprocess harness, additional service or second-restart obligation is needed. A delay or another WAL readability poll would not establish the missing event.

## Verification limits

Independent review here was immutable source/diff/caller inspection; no Cargo execution or source instrumentation was performed by this reviewer. Root reported independent execution of all three exact new context/recovery cases: **3 passed, 915 skipped**, and UI unit/metric-math checks. The supplied implementation evidence reports the full scoped Rust lane, fmt/clippy and the filtered Metrics journey passing. Those results are useful, but do not resolve the source-proven boundary-observation gap.

Skipped unrelated historical/Bloom re-execution and any second-restart expansion. Remaining risk is acceptance of an inaccurately identified interruption boundary; no production data-loss failure was demonstrated.
