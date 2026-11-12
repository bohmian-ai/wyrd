---
id: TASK-005-R2
kind: remediation
status: ready
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 7
parent_task: TASK-005
remediates: [FIND-005-1, FIND-005-3]
requirements: [REQ-001, REQ-003, REQ-022, AC-018, INV-004]
depends_on: []
---

# Resolve the remaining measurement reference and Gateway qualification

The Verification calibration now passes. The engine reference still rejects a
native c64 write case because the server reports one more inserted row than
the independent reference predicts. Gateway observation cost also remains
unqualified. Close these two remaining parts of FIND-005-1 using the existing
measurement owners; do not change the reference simply to make the run pass.

## Authority and subject

- Approved spec: `changes/active/benchmark-latency-diagnosis/spec.md`, revision 7,
  retaining the original TASK-005 revision-6 obligations.
- Original task: `changes/active/benchmark-latency-diagnosis/tasks/TASK-005-full-path-benchmark-parity-and-optimization.md`.
- Prior candidate: `180879328ac9dde94d7504855e821980e5db0f18`.
- Reviewed candidate: `2ce7caec7d5a7bd51fb0ba1512f552b0f425ec0f`.
- Prior R1 task and evidence: `../TASK-005-006-review-001-180879328/TASK-005-R1-qualified-measurements-and-details-proof.md`.
- Validated diagnosis and correction: [findings-validation.md](findings-validation.md).

## Diagnosis and selected correction

REQ-022/AC-018 require qualified observations before crediting optimizations.
R1 records two `native-write-c64` failures with server rows R+1, as well as
unresolved Gateway observer qualification. The current Verification calibration
and final VALID Agent records do not close those different proof populations.
This is an acceptance-proof failure, not evidence that production lost or
duplicated payload rows.

`capacity/tests/pg_tests.rs::rows_match_the_server` derives audit rows from
acknowledged logical requests. Native client `GrpcSink::send_owned_bytes` can
retry the same batch identity, while Gate authorization stages a separate audit
decision before Scribe admits the payload. That makes the retry explanation
plausible; the retained records do not establish it as the measured cause.
The Gate terminal request count is also not automatically the authorized
decision count: early failures can occur before authorization.

Reuse the existing Gate/audit producer, sole benchmark recorder, snapshot and
independent-reference test. First diagnose why the attempted Gate series read
as absent/zero: preserve the failing snapshots and trace actual build,
registration, initialization, export and selection through the existing metric
APIs. Absence must remain distinguishable from an observed zero. Correct any
established measurement defect at its owner.

Reconcile deduplicated payload rows against logical batches and audit rows
against the actual staged decisions in the same settled window, including
retries and late calls. If the existing observations cannot distinguish these
populations, add only the necessary bounded standard metric at the existing
decision producer and expose it through the existing recorder. This is an
extension of the approved observation path, not another benchmark mechanism.
Prove its meaning under a controlled retry before using it to correct the row
equation. If it does not explain the extra row, trace the remaining source and
leave the proof failing until diagnosed. A fixed tolerance, inserted-row count
as its own reference, or counting every refused Gate call as authorized is
not an independent proof.

Resolve Gateway observer cost from retained comparable evidence first. If it
cannot establish the original envelope, prepare the smallest current-code
observer control using the existing experiment/window/report owners and obtain
the original workload authorization before execution. Retain all standard
repetitions, method/build identity and original validity rules. Report costs,
spread and unresolved attribution honestly. Do not rerun historical baselines
or relabel confounded/INVALID records. No new performance target is imposed.

The burden is one bounded extension of existing observation/proof owners, if
needed, and the outstanding qualification evidence. Deleting or weakening the
independent check would remove the approved guarantee. A new service, recorder,
retry policy, public API or persistent-data decision is unnecessary.

## Preserved behavior and limits

Preserve authorization and one audit decision per permission check, Scribe
deduplication and acknowledgement, cancellation, settled-window populations,
host guards and canonical historical records. Do not redesign native retries,
repair Azurite, generate host load, or rebuild old binaries. Passing calibration,
details and existing reduction proofs carry forward unless a change invalidates
them. The remaining documentation subset is split below without duplicate edits.

## FIND-005-3: RequestJson documentation subset

The original list is closed. Batch these newly changed Gateway items with this
substantive correction, preserving the mandatory rustdoc gate:

| Candidate location in `crates/wyrd/wyrd-gateway/src/request.rs` | Required contract |
|---|---|
| `:239`, write_members | Panics/invariant for its private valid member ranges used in indexing/subtraction; no redundant runtime guards. |
| `:668`, ranges and `:744`, with_model_rewrites_kept_text_without_decoding | Move the misplaced model-rewrite test intent/Panics onto that test; retain the range helper's actual purpose. |
| `:677–682`, Flattened fixture fields | Brief serialization/duplicate-member roles. |
| `:689`, written_text_scan_matches_the_canonical_check | Assertion/expect panic contract. |

No runtime defect is alleged by missing prose. Add accurate concise contracts;
deleted-by-functional-fix items need no replacement boilerplate. Source inspection
and affected formatting prove this subset. TASK-006-R2 owns the other rows of
the shared finding; no checker, dependency or documentation-only round is needed.

## Ordered proof and acceptance

1. Preserve the failing evidence, distinguish absent from zero, and reproduce
   the necessary observation gap in a focused existing-owner test. If executable
   behavior changes, record RED, the smallest correction, and GREEN; do not
   manufacture a failing test for unchanged behavior.
2. If a decision observation is required, prove that a retried/deduplicated
   payload contributes payload once while each real permission decision remains
   independently observed. Early refusal without authorization must not add an
   authorized-decision row. Use existing Gate/recorder fixtures.
3. Complete the existing engine reference under its existing host guards and
   separately authorized release-server workload. Every original case and
   repetition must reconcile; preserve the previous failures as history.
4. Resolve the Gateway observer envelope with comparable canonical evidence;
   distinguish observer cost from optimization gains. Do not repeat the passing
   Verification calibration solely to collect another pass.

The existing named engine command is:

```sh
RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows)'
```

This command runs an opt-in workload; this review does not authorize executing
it. Any added unit-test selector is planned: confirm its owning target and exact
name after adding it, record its exact focused command, then run RED/GREEN.
Run `mise run fmt` and the narrow clippy/test lanes for the actual executable
write set; use repository Postgres setup for any PG proof. No full journey suite,
aggregate gate, new profile run or unrelated benchmark is required by this task.

Completion requires both remaining proof obligations and this bounded docs
subset, not merely an explanation of the likely extra row. Route directly to
`$wyrd-implement`.

## Maintainer override

FIND-005-1 (native-write reference and Gateway observer qualification) is
overridden by the maintainer and removed from this task. Only the FIND-005-3
RequestJson documentation subset was implemented.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-005-1 | Overridden by maintainer | none | NOT REMEDIATED |
| `write_members` Panics contract | `crates/wyrd/wyrd-gateway/src/request.rs` `write_members` | source inspection; `mise run lints` | PASS |
| Misplaced test doc moved; `ranges` keeps its purpose | test docs moved to `with_model_rewrites_kept_text_without_decoding`; its duplicate range closure replaced by `ranges` | `cargo nextest run --locked -p wyrd-gateway --lib` (in the 328-test run) | PASS |
| `Flattened` field roles | `request.rs` tests `Flattened` | source inspection | PASS |
| `written_text_scan_matches_the_canonical_check` Panics | `request.rs` tests | source inspection | PASS |

Commands: `mise run fmt`; `mise run lints` (exit 0);
`mise exec -- cargo nextest run --locked -p wyrd-auth-verify -p wyrd-gateway -p wyrd-skald-spec -p wyrd-skald-providers -p vala-verify --lib` (328 passed);
`git diff --check`. No runtime behavior changed.
