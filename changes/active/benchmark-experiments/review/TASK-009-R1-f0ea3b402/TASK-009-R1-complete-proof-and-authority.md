---
id: TASK-009-R1
kind: remediation
status: proposed
spec: SPEC-benchmark-experiments
spec_revision: 24
parent_task: TASK-009
remediates: [FIND-TASK-009-1, FIND-TASK-009-2, FIND-TASK-009-3, FIND-TASK-009-4]
---

# Complete the public ACK proof and reconcile its authority

The shared admission implementation is supported by the review, but its required SDK journey bypasses public Run/Window authoring, its owner documents still contain the retired ordering, and one new lifecycle method lacks mandatory rustdoc. Final-source integrated proof also trails the last two executable changes. Correct these bounded gaps without redesigning admission.

## Acceptance context

- Approved spec: `changes/active/benchmark-experiments/spec.md`, revision 24, REQ-035–039 and AC-015–019.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-009-unified-queue-and-closeout.md`, revision 2.
- Reviewed base: `6f38db16c524417301ffc4a5dc0b478daf997ed1`.
- Prior candidate: `f0ea3b4023e544d0f95e9b5e38d398105dbe2e33`.
- Review authority: sibling `verdict.md` and `findings-validation.md`; stable findings must retain their IDs.
- Execute with `wyrd-implement`; subsequent task review covers closure, the prior-candidate-to-new-candidate diff and affected callers, with original cumulative context retained.

## Diagnosis and selected correction

### FIND-TASK-009-2: V18 skips the promised public authoring path

`crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:7632`, `queued_origins_share_ack_gated_admission`, sends raw `request_json` bodies to enqueue/execute, records observations with `Bifrost::write_batch` and uses private schedule fixture controls. It proves the server's own-input gate and readback but never executes the specifically required Run/Observe submission and Window observation authoring in that held-ACK journey.

Actual `Observe::submit` resolves the bound Verifier, projects declarations, and chooses Window `drift_value` observation recording or Run `enqueue_run`/`Target::enqueue` with normal target correlation and idempotency. That supported path includes the newly borrowed bound spec in `wyrd-client/src/state.rs:888`. The current V18 would pass if this authoring path failed before reaching the correct server admission owner. Existing normal SDK stories do not supply the new combined held-ACK proof. This is a required proof gap, not a demonstrated runtime defect.

Extend this existing journey to use public hydrated-state/Run/Observe authoring for grouped Run sources and scheduled Window observations, and retain the ordinary returned execution handles for status/readback. Reuse card/state patterns from `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs::grouped_conversations_coexist_with_drift`, the existing ingest barrier and schedule-timing controls. Keep the held-input assertions, direct completion while archival is paused, all-origin completion and fixed grouped-source readback. Harness controls may set deterministic timing; they must not replace the public operations being proved.

Replacing bypasses in one existing journey is sufficient. Do not add a parallel test harness, new SDK APIs, private SDK access, raw SQL in stories, or another language-wide ACK matrix. No production admission change is justified by this finding.

### FIND-TASK-009-3: the owner text still directs the old race

`architecture/wyrd-design.md:1638`–1659 describes group selection enqueueing/deleting pending state inside its preparation transaction, then staging the selector after commit. `architecture/bifrost-design.md:1827`–1830 describes scheduled/group selectors published after PostgreSQL commit and early claims retrying missing evidence. These runnable-candidate clauses contradict the revised ACK-first paragraphs at Wyrd `:1751` and Bifrost `:387`, approved REQ-038 and actual source. A maintainer following the owner could restore exactly the premature admission/consumption this task removes.

Correct these existing paragraphs to describe preparation and transaction release, own-input publication/ACK, then the shared tenant admission transaction inserting/replaying execution and conditionally moving its expected cursor or selected group generation. Reconcile concurrent-group activity wording with generation-scoped consumption so newer activity stays pending. Preserve cutoff/order/null rules, source visibility/retention, unchanged-content and permanent-refusal decisions, orphan prequeue input limits, and defensive missing-evidence retries.

The existing runtime owners already implement that order: scheduler/group preparation, Scribe's private acknowledged transition, `SubmissionSink`, and SQL accept methods. Correct their normative text; do not introduce another owner, protocol, guard, persistent record or stronger guarantee. Historical migration comments are not a reason to edit applied migrations.

### FIND-TASK-009-4: a required lifecycle item is undocumented

The new `impl Drop for InFlight::drop` at `crates/wyrd/wyrd-server/src/verification/scheduler.rs:148` has no method rustdoc. It removes the source from shared in-flight suppression on final Arc ownership release and recovers mutex poison. The type's documentation does not satisfy root AGENTS' explicit every-changed-item rule; missing rustdoc blocks merge.

Add concise method rustdoc covering its lifecycle role, removal side effect and poison recovery. Preserve the implementation and claim lifetime. This is one documentation block, with no new runtime test, lint exception, helper or source-tracking redesign.

### FIND-TASK-009-1: integrated proof identifies an older executable source

The genuine green V13 log identifies `e9e3d95cb57249a37238235259437dd8a64fab98`. Later `da366eefe` changes production bound-Verifier resolution used by Observe, and `f0ea3b402` changes the paced upstream used by the Gateway driver. The original task's final-source V13 requirement therefore remains unsatisfied at the reviewed candidate. Its one passing outbox regression cannot substitute for that aggregate.

Update task evidence to identify those later changes and the remediation candidate accurately. Preserve the older green result and its SHA/log provenance; do not relabel it as a run on new source. Link narrow remediation proof and mark original final-source V13 pending unless genuine aggregate evidence for the settled executable source exists. At original integrated change-review/closeout, fulfill and record V13 once for the settled source: SHA, exact invocation, overall exit, lane results and actual log path. This is the original completion obligation, not an extra gate per finding. Do not add a broad aggregate to this remediation's focused checks or rerun already-covered components as a checklist.

The user accepts the older five L1 records as valid; do not reopen driver-budget validity. Their `19f4190a8` source attribution stays intact. Original V14–V16 final-source measurements, separate history/report publication and main-based PR remain downstream closeout. This remediation neither performs nor newly authorizes those operations; it records the pending status honestly. The current review request prohibits benchmarks, commits, pushes and publication.

## Preserved behavior and non-goals

Keep the one ACK-gated pipeline, existing Bifrost input resolver/engine, frozen metadata, per-tenant batching, expected-cursor/generation protection, RLS/authority, renewable leases/stale-result fence and direct costs. Preserve the original exact work/query counts, 50 ms linger, source-retention/orphan/prequeue and asynchronous result-loss limitations. No source selection changes, new prequeue store, stronger durability, public receipt, compatibility path, benchmark retuning, L2, merge or release.

## Focused acceptance and proof

| Finding | Required closure observation |
|---|---|
| FIND-TASK-009-2 | The existing V18 journey executes public Run submission and Window observation authoring under its held-input ACK assertions, retains direct return and immutable grouped readback, and passes its exact selector with nonzero test selection. |
| FIND-TASK-009-3 | Both complete owner sections consistently describe ACK-before-admission and generation-scoped source consumption, with preserved refusal/source-retention limits. |
| FIND-TASK-009-4 | `InFlight::drop` carries meaningful method rustdoc; behavior is unchanged. |
| FIND-TASK-009-1 | Task evidence distinguishes earlier and final source, links focused proof, and records original V13 as pending until genuine settled-source integrated evidence closes it. Whole-task PASS cannot be claimed while that obligation is pending. |

Run only the existing focused V18 journey for the behavioral correction, through the repository's mise/Postgres lifecycle:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run build:server && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all --test-threads=1 -E "test(=verification_runtime::queued_origins_share_ack_gated_admission)"'
```

Use static paragraph/source comparison for authority and destructor rustdoc. Run `mise run fmt`, the owning lints for the Rust write set, and `mise exec -- git diff --check`. Confirm owning tasks in `mise.toml`; keep Cargo work sequential. Run another exact regression only if a changed dependency or actual failure justifies it. Do not require complete journey suites, full benchmarks or a broad aggregate inside this task remediation. Original integrated final-source proof and delivery run in their established closeout phases.

## Implementation Evidence

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-009-2 | `verification_runtime.rs::queued_origins_share_ack_gated_admission`: `support` and `observed-service` are pulled and hydrated as `WyrdState`s; the Window sample is `Observe::submit("observed-drift")` (`Submission::Recorded`), the held turn is `Observe::group_id("held").submit("turn-check")` (`Submission::Queued`), direct is `Observe::verify`. Raw enqueue/execute bodies, `Bifrost::write_batch`, `custom_drift_verifier` and `bind_schedule` removed; `activate` and `make_binding_due` remain as timing controls. Held-ACK, direct, first-attempt and grouped readback assertions unchanged | the exact selector above: 1 run, 1 passed (33.5 s) | PASS |
| FIND-TASK-009-3 | `architecture/wyrd-design.md` group paragraph: preparation writes nothing and rolls back, the selector is staged, and on its ACK the batched writer inserts the execution and deletes the pending row only at the selected generation; unchanged digests settle in preparation; newer activity keeps a fresh generation. Scheduled text: "prepares" the occurrence. `architecture/bifrost-design.md`: selector staged before admission, its ACK admits in the cursor/generation transaction, defensive missing-evidence retry kept | static comparison with `accept_grouped`, `prepare_group`, `hand_off_groups`, migration `20261028000000`; `mise run docs:check` exit 0 | PASS |
| FIND-TASK-009-4 | `scheduler.rs` `InFlight::drop` rustdoc: release on last holder, removal from the shared set, poison recovery | `cargo clippy -p wyrd-server --lib -D warnings` clean; no behavior change | PASS |
| FIND-TASK-009-1 | TASK-009 "Later executable source and closeout status" names `da366eefe`, `f0ea3b402`, this remediation, and V13–V16 as pending | static | PASS |

Checks: `mise run fmt`, `cargo clippy -p wyrd-testing --all-targets -D warnings`,
`git diff --check` clean. Non-goals held: no production admission change, no
new harness, SDK API, raw SQL in stories, benchmark, aggregate, or publication.
