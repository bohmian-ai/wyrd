---
id: TASK-004-R2
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 27
parent_task: TASK-004-R1
remediates: [FIND-TASK-004-7, FIND-TASK-004-8, FIND-TASK-004-9]
---

# Finish the recovery proof and scope histogram selections

Execute through `$wyrd-implement`. Correct three validated residual/affected-path findings; F1–F6 are closed and must not be reimplemented.

## Acceptance context and immutable subjects

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Original base: `9692bfd06c6bdf2585e88c791884a90669a5dc26`.
Prior candidate: `ed82f63bf35ca124c0a2980c160f0d64535eebd1`.
Reviewed current candidate / next review's prior candidate: `0c2bfe70d5186dff4dc224c7e402b57c24f354d9`.
Approved spec: `changes/active/wyrd-ui-main-integration/spec.md`, revision 27 with already authorized context reconciliation and continuing caller approval of potential spec deviations.
Original task paths under `changes/active/wyrd-ui-main-integration/tasks/`: `TASK-004-observe-components.md`, historical/superseded `TASK-022-metrics-page.md`, and `TASK-004-revision/TASK-04-bifrost-bloom-pruning.md`; preserve recorded TASK-02/TASK-03 closure transfers.
Prior remediation: `changes/active/wyrd-ui-main-integration/review/range-9692bfd06-ed82f63bf-20261104/TASK-004-R1-metrics-correctness-and-index-coverage.md`, with its verdict/ledger and evidence. Current acceptance: this directory's `verdict.md`, `findings-validation.md`, selected reports and focused follow-up. Repository and owning architecture authority apply.

## Intended outcome and correction ownership

A selected histogram remains populated when the reader changes grouping or every-series mode, with independent latency/rate visibility scoped to its metric view. Recovery tests establish their claimed post-sync unanswered boundary and actual old-owner/IO termination before replacement, retaining current nonvacuous row/retry assertions. The remaining Rust constant receives its mandatory purpose documentation.

Reuse Observe's current per-view selection map and Chart shown/callback seam; reuse Scribe's existing post-sync hold, abort owner and WAL-lane lifecycle. Ordinary private names, fixture layout and local control flow remain implementation-owned. No new product, API, persistence or recovery decision is needed.

## Issue diagnoses and selected corrections

The following independently validated ledger is reproduced so an implementer need not rediscover the defects or choose the correction boundary.


### FIND-TASK-004-7 — Required durable interruption boundary is not observed

**REVISED; MISSING required proof.** Sources: prior SYS-01, current DATA-R1-01 and focused follow-up. Obligation: spec revision 27 AC-038 and TASK-004-R1's explicit durable-series/pre-points plus durable-unanswered-points retained-volume recovery proof.

Location: `redux/src/scribe/tests/scribe_persistence_path.rs:1539–1568`, callers at `1623–1632` and `1688–1697`.

Independent trace:

1. The helper only decodes `replay_wal_directory` and detects nonempty commits/matching append metadata. Its select checks for a premature answer only while this condition is false. Neither the condition nor successful replay observes live sync completion.
2. `shards.rs:3792–3836` submits/awaits COMMIT `WritePrepared`, collects touched segments, and then separately awaits `sync_group`; `4484–4504` submits/awaits `SyncWal`. `execution_lanes.rs:2304–2316` performs append and sync as distinct operations. `wal.rs:1733–1746` writes readable bytes; `2551–2558` fsyncs segments separately.
3. Consequently the scan can see the terminal COMMIT while the shard is awaiting the write notification, before SyncWal submission/completion. The post-sync fault at `shards.rs:3287–3296` is armed but not observed. That branch only retains `post_commit.into_state()` after sync; observing its retained state would establish the intended durable/unanswered boundary.
4. `execution_lanes.rs:2198–2228` submits detached Rayon work, whose filesystem operation can finish after its async waiter drops. `shards.rs:1096–1111` retains ordinary Tokio handles; its owner loop at `1397–1465` may finish the current group before channel closure is noticed. No Scribe/ShardRuntime Drop aborts these owners. `drop((scribe, wal))` therefore is not a stop barrier before epoch 2 opens the directory.

Both tests can pass their recovered counts on that schedule without demonstrating their declared interruption point. This matters because the narrow interruption test is an explicit acceptance obligation, not because any production loss has been observed. The eight recovered points are useful nonvacuous evidence and must remain.

**Decision-complete correction:** keep both existing cases, current retained directory, next epoch, ordinary replay and existing row/retry assertions. Reuse the existing post-sync failure hold and Scribe abort lifecycle. Replace readable-COMMIT polling as the trigger with positive non-consuming test-tier observation of the targeted admission *after its group has entered retained post-sync/pre-insertion ownership*, while its response is still unanswered. A fault-arm flag or its earlier consuming swap is insufficient. Explicitly abort old owners without graceful flush, observe their termination, and await any already-submitted WAL IO before constructing epoch 2. Existing `abort_shutdown`, retained task/abort handles and WAL-lane `drain` provide the owners/mechanisms; add only the narrow test-support access/observation needed to use them. This does not change production lifecycle, ordering, admission or recovery semantics.

**Burden and rejected alternatives:** a bounded test observation/stop correction is sufficient. Sleeping, repeating green runs, observing sync submission or reading WAL again cannot establish completion. A new subprocess harness, cross-table transaction, durable storage tier, second restart or broad crash matrix is unnecessary. The hold consumer owns the exact post-sync invariant, so publishing its test observation there is appropriate rather than adding unrelated downstream guards.

**Focused closure proof:** independently select and execute the two existing exact interruption tests after the correction. Establish target held-state reached, response unanswered, old owners/IO complete before next writer, then ordinary replay with 1 series/0 points plus successful 4-point retry, and 1 series/8 recovered points including the unanswered batch. Preserve every recovered point's context-index membership assertion and refusal/retry sibling behavior. Run only changed-owner Rust format/lint and focused tests; no broad runtime suite.

### FIND-TASK-004-8 — New nested Rust const lacks mandatory documentation

**CONFIRMED; VIOLATION.** Source REPO-R1-1, continuing prior REPO-1. Root AGENTS requires rustdoc on every new/changed Rust item and says missing rustdoc blocks merge; `agent_docs/rust.md:234–240` applies regardless of visibility, including test items.

Location: `redux/src/scribe/tests/scribe_persistence_path.rs:1301`, `const CONTEXT: [&str; 4]` inside `series_contexts`. The complete helper body has purpose/panic docs, but the new constant item has no rustdoc. This leaves the explicit gate open after all originally listed omissions were repaired. No production behavior or panic is alleged.

**Minimum correction and proof:** document why these four representative accepted-context fields are used by the fixture. One local rustdoc comment closes it; no helper, runtime test, API or lint suppression. Inspect the item and any Rust items added by F7 together and use already-required owning format/lint checks. Batch with behavioral/proof corrections; do not create a style-only round. Retain F8 rather than assigning a new ID for this same cause.

### FIND-TASK-004-9 — Histogram line choices hide new views after ordinary navigation

**CONFIRMED; REGRESSION.** Source B-R1-01. Obligations: REQ-050 histogram identity/selection, REQ-053/054 and AC-032 grouped-to-every-series investigation. Location: `ui/src/routes/t/[tenantKey]/observe/+layout.svelte:344–367`; shared consumer `packages/ui/src/lib/components/Chart.svelte:112–140`; identity producer `observe.ts::histogramWindows` and `loadMetric`.

Independent trace: latency/rate props now carry typed group/bounds/measure identity. Clicking a checkbox or Show all invokes `choose`; without `onseries`, Chart stores a local `chosen` set. A subsequent grouping or apart link updates this same SvelteKit layout and retains the histogram branch/Chart instances; there is no keyed remount. Default identity cells and apart identity cells have different tuple shape/content. Chart's `shown ?? chosen ?? default` retains old keys and `drawn = lines.filter(visible.has)` therefore selects no current line for disjoint tuples. Populated data and legend remain but plotted values disappear until Show all. Rate has the same owner/cause; it is not a second finding.

The existing numeric sibling at layout `:163–164,377–378` already scopes host choices by serialized metric view. The new histogram calls supply neither `shown` nor `onseries`. The browser fixture changes mode before any visibility selection and the component fixture unmounts after selection, so their successful executions do not cover this transition.

**Materiality:** selecting lines then changing grouping/apart is ordinary explicitly supported investigation. A populated plot becoming blank is a material bounded interaction regression, even though Show all recovers it and stored data remains intact. This is a new host-selection cause; original F1 typed identity and F2 missing ranking/control remain closed.

**Decision-complete correction:** extend the existing Observe per-view `picks` ownership to histogram latency and rate, with separate selection identities for each plot under the current metric view. Pass each plot's current `shown` and callback through the existing Chart seam. New views use that plot's default keys; same-view choices remain available; changing latency choices cannot hide rate lines. Preserve generic authored Charts and numeric charts. No new state store, option, global reset effect, schema or replacement chart is warranted.

**Burden and focused proof:** a small host binding extension solves the producer/consumer mismatch. Deleting selection or the required apart control loses approved behavior; changing generic Chart reset policy affects unrelated callers. Extend existing host/component coverage and the filtered Metrics journey: choose/hide a histogram line, change grouping and click every series without page reload, then verify populated current latency and rate lines remain drawn and choices are independent. Assert same-view choices remain scoped as designed. Reuse existing fixtures; no new harness or broad journey collection.


## Constraints, preserved behavior and non-goals

Preserve all F1–F6 closures: correct reset-before-window SQL, exact integer summaries, typed histogram identity without numeric folding, missing-sum availability, lower-bound qualifiers, busiest top-20/totals and every-series index labels, stable stored series IDs and complete accepted-context suppression. Keep latency/rate selection independent, existing same-view choices, authored generic Charts and numeric host behavior.

Keep production ordered series-before-points durability, ordinary replay/fences/WAL format, bounded tenant-scoped cache, native/OTLP/accepted dimension semantics, permissions and query ceilings. Test-tier observation belongs at the existing post-sync held-state owner; it must not change when production acknowledges data, add graceful flush to the simulated stop, or allow old IO to race replacement.

No new subprocess harness, recovery layer, persistent storage tier, second-restart obligation, broad crash matrix, global Chart reset, schema/config option, dependency or new state store. No changes to accepted unchanged Bloom/fork/fingerprint/dev/sort behavior. Do not edit prior review reports to remove the nonblocking historical EOF whitespace warning. Latency and human visual acceptance remain disclosed; this task does not invent performance optimizations or claim visual signoff.

## Scenarios and acceptance mapping

1. **F9:** select/hide histogram lines, change grouping, then switch to every-series without reloading; each populated latency/rate view must draw its current data, with no sibling/view choice leakage and same-view choice behavior preserved. Exercise both plots and the actual host navigation, not only a fresh Chart mount.
2. **F7:** each existing interruption case positively observes its targeted group already retained after completed sync, with response unanswered; old owner tasks and submitted WAL IO have completed termination before the next writer starts. Ordinary same-directory recovery preserves the current series-only/zero-point then four-point-retry assertion and the eight-point recovery including unanswered points, all indexed by context. No sleep/readability poll substitutes for that event.
3. **F8:** document the local representative-context constant and all Rust items actually added or changed by the F7 correction, with accurate errors/panics where applicable. Static inspection closes documentation; do not manufacture a runtime test for a comment.

Perform RED/GREEN/refactor per executable scenario using the smallest regression proving the exact gap. Record actual selected commands and results; planned new test names remain implementation-owned and require exact focused selectors when added. A successful run that never reaches the asserted boundary is not proof.

## Focused verification

Use existing unit/host and filtered Metrics journey fixtures, and the same retained-volume Scribe cases. Run only the correction's affected lanes; do not require full user-journey collections or aggregates.

Existing exact interruption tests plus refusal sibling:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::scribe_persistence_path::a_stop_after_durable_series_rows_recovers_them_and_admits_the_retry) | test(=scribe::scribe_persistence_path::a_stop_after_durable_unanswered_points_recovers_every_point_indexed) | test(=scribe::scribe_persistence_path::a_failed_series_admission_fails_the_points_admission)'
```

Record exact selectors for any new lifecycle/held-observation test; confirm with source/nextest listing. Update current evidence to describe the event actually observed and why old IO cannot complete after replacement starts.

Affected UI unit/host files and single Metrics journey:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts src/lib/ui-library.test.ts
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated metrics investigation$"'
```

Reuse the existing build prerequisites from `changes/active/wyrd-ui-main-integration/references/verification.md` only as needed for changed source: `mise run ts:build`, `mise run ts:build:testing`, `mise run build:server`, and owning UI build. Preserve the current journey's real Service-release scoped rail assertions while adding the missing visibility-before-navigation sequence. A skipped or zero-selected result does not pass.

Finish with write-set checks:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --locked -p vala-bifrost-redux --all-features --all-targets -- -D warnings
```

No codegen input change is expected; if an actual correction changes a generated input, regenerate via its existing owner and run `mise run codegen:check`. Do not rerun unaffected metric-math/Bloom/fork suites merely because another review round began. The existing math integration passed independently in this review and its source/caller closure carries forward unless this correction touches it. No `gate`, synthetic host load or extra crash matrix.

## Next review scope

Retain the original task, both prior candidates/verdicts/ledgers, R1 and this R2. Review `0c2bfe70d5186dff4dc224c7e402b57c24f354d9..<new-candidate>` for F7/F8/F9 closure and directly affected state/lifecycle/caller paths. Carry F1–F6 and unchanged first-pass acceptance forward unless concrete changed dependencies or new material evidence invalidate it. This is the first unsuccessful remediation review; the two-round convergence threshold has not been reached. Do not commit/push/merge/deploy unless separately asked.

## Implementation evidence

Candidate `0c2bfe70d..1faffddef` (`5476a76fc` F9, `1faffddef` F7/F8).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| F9: histogram latency and rate choices are scoped per metric view and plot; regrouping and every series draw the new view's lines; same-view choice kept | `observe/+layout.svelte` passes `shown`/`onseries` keyed `latency <view>` and `rate <view>` through the existing `picks` map | Metrics journey, histogram step: hide a latency line (rate unchanged), group by `service_name`, every series, then back/forward. RED without the layout change: `expected +0 to be 4` at the regroup step; GREEN with it | PASS |
| F7: each interruption case observes its target group retained after completed sync, response unanswered, then stops old owners and WAL IO before epoch 2 | Hold branch in `shards.rs` publishes the held slices to `WalFaults::post_sync_held` (a `watch`, non-consuming); `WalWriter::post_sync_held_for_test`; `ScribeImpl::abort_and_settle_for_test` takes the shard join handles, runs `abort_shutdown`, awaits each owner, then `wal_io.drain()`; `crash_once_held` replaces readable-COMMIT polling | The two exact interruption tests plus the refusal sibling: series case 1 series / 0 points then a 4-point retry; points case 1 series / 8 points; every point indexed by context | PASS |
| F8: the `CONTEXT` const and every Rust item added for F7 are documented | Rustdoc on `CONTEXT`, `post_sync_held`, `post_sync_held_for_test`, `hold_post_sync_for_test`, `take_owners_for_test`, `abort_and_settle_for_test`, `crash_once_held` (with `# Panics`) | Static inspection; clippy `-D warnings` | PASS |

The held group is published only after the shard owner holds `retained_commit_ambiguity`, which is after `SyncWal` completed. The test waits on that publication and never on WAL readability. Old IO cannot race epoch 2, for two reasons:
- Every shard owner is aborted and joined, so no new WAL job can be submitted.
- `ScribeWalIoPool::drain` returns only once every submitted Rayon job has finished.

The new hooks are `#[cfg(test)]`, except the publication on the existing test-support fault branch. Production ordering, acknowledgement and recovery are unchanged.

Commands:

- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E '<the three exact selectors> | test(post_sync) | test(=scribe::wal_closeout::failure_after_fsync_before_ack_reuses_stable_batch_once)'` passed (5 tests).
- `pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts src/lib/ui-library.test.ts` passed (22 tests).
- The filtered journey `-t "^authenticated metrics investigation$"` passed (1 passed, 5 skipped).
- UI `check` (0 errors) and `build` passed.
- `cargo fmt --all -- --check` passed.
- `cargo clippy -p vala-bifrost-redux --all-features --all-targets -D warnings` passed, and so did the `--features test-support --lib` variant.
- `git diff --check` was clean.

Non-goals stayed excluded:
- No change to production lifecycle, WAL format, Chart reset policy, schema or dependencies.
- No second restart and no subprocess harness.
- Prior review reports are unedited.

Limit: human visual acceptance is still not claimed.
