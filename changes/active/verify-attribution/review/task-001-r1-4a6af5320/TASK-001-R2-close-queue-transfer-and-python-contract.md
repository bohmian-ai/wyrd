---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-verify-attribution
spec_revision: 3
requirements: [REQ-006, REQ-008, AC-001, AC-005]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-5]
---

# Close queue ownership transfer and Python timeline typing

Execution skill: `$wyrd-implement`. This is the single R2 remediation task for
TASK-001. It closes two independently validated defects within approved spec
revision 3; there is no migration or new product/architecture decision.

## Authority and immutable inputs

- Approved spec: `changes/active/verify-attribution/spec.md`, revision 3.
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`.
- Prior verdict, ledger, and R1 task:
  `changes/active/verify-attribution/review/task-001-827ec9f15/`.
- R1 re-review verdict and validated ledger: this directory's `verdict.md` and
  `findings-validation.md`.
- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`.
- R1 reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`.
- Current reviewed candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`,
  tree `036d647413d0acfd1afdebff2b7057b45ac00609`.

Apply AGENTS.md and the applicable queue, Python, testing, and Bifrost
authority. Preserve all behavior already closed by R1.

## FIND-TASK-001-1: transfer the existing producer-context charge at seal

### Diagnosis

`Producer::enqueue_prepared` now correctly charges the row arrays and producer
context before admission. While those row guards still own that context charge,
`RecordQueue::seal` reserves `max_message_bytes + trace_bytes` for the frame
and selected contexts. The budget deliberately keeps back only
`max_message_bytes` for sealing. A valid traced staging set that fills the
admission limit therefore leaves enough room for the frame but not the second
copy of its context charge; sealing restores the same rows and repeats the same
backpressure forever. The current retry test leaves spare admission capacity,
so it does not exercise this state.

### Selected correction

Keep `ClientByteBudget` as the sole owner. At the existing staged-to-sealed
transition, carry the selected first-32 contexts' already-admitted charged
ownership into `SealedBatch`; reserve sealing headroom only for the IPC frame.
Each retained context allocation has exactly one live charge from admission
until ACK, terminal loss, or shutdown. Contexts not selected for the batch
release with their rows.

Do not enlarge the byte limit or permanent headroom, add another limiter or
counter, weaken context accounting, change the first-32 rule, change batch or
retry identity, or add a second context representation on the wire. Reuse the
existing budget, staged context ownership, `SealedBatch`, and settlement path.

### Acceptance and focused proof

- A non-empty sampled context can fill the documented admission ceiling and
  still seal and publish using the reserved frame headroom.
- Selected contexts remain charged through an ambiguous retry and every charge
  returns to zero after terminal settlement.
- Unselected contexts and failed admission release their charge normally.

Extend the existing saturation test rather than adding a harness, and retain
the R1 retry test:

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::admitted_rows_cannot_take_the_sealing_headroom) | test(=producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry)'
```

## FIND-TASK-001-5: make Python describe the canonical timeline output

### Diagnosis

The Python native boundary serializes the canonical Rust `SpanRecord` directly
to dictionaries. Rust always emits fields that have `serde(default)` but no
`skip_serializing_if`, yet the authored Python `TypedDict` graph marks those
fields optional. Its broad `SpanStatus` also permits `description` for `ok` and
`unset`, shapes the canonical Rust producer and generated TypeScript sibling
cannot emit. Runtime journeys pass, but the public Python type is weaker than
the shared output contract.

### Selected correction

Keep the current dictionaries, native JSON conversion, public methods,
exports, and inline authored types. In the existing `TypedDict` definitions,
make every always-serialized `Resource`, `InstrumentationScope`, `SpanEvent`,
`SpanLink`, and `SpanRecord` field required. Leave only fields that serde can
actually omit as `NotRequired`. Express `SpanStatus` as the canonical three
tagged variants: `unset` and `ok` contain only `code`; `error` may additionally
contain `description`.

Do not restore the deleted Python generator or generated classes, add a
runtime converter/validator, introduce aliases or compatibility behavior, or
change Rust/TypeScript/runtime data.

### Acceptance and focused proof

Extend `sdks/wyrd-sdk-python/tests/typecheck/bifrost_contracts.py` so a complete
canonical record type-checks, omission of an always-emitted key is rejected,
and an `ok` or `unset` status with `description` is rejected.

```bash
mise run py:typecheck
mise run py:lints
mise run py:format
```

No additional runtime journey is required for this declaration-only change;
the Rust, Python, and TypeScript action-timeline journeys all passed on the
reviewed candidate.

## Preserved behavior and non-goals

- No migration, new task split, public endpoint, query, wire field,
  configuration, compatibility surface, or second resource owner.
- No change to sampling, linked-ingest eligibility, tenant routing, catalog
  projection, Scribe ownership, timeline query depth/order/deduplication, or
  Rust and TypeScript SDK contracts.
- Do not forward producer `tracestate` or reserved flag bits to Gate links.
- Keep the user-approved localized `SpanData` clone.
- Keep the R1 role-separated Scribe outage proof and its production paths
  unchanged.

## Final scoped verification

After the focused proofs, run `mise run test:shared`, `mise run fmt`,
`mise run lints`, and `git diff --check`. Do not run `mise run gate`, broad SDK
journey suites, code generation, schema checks, or migration checks unless the
implementation expands beyond the prescribed queue and authored-Python-type
write set.

Record exact commands and selected counts against both finding IDs. A later
`$wyrd-task-review` reviews the complete cumulative candidate against the
original task, prior verdicts, and this remediation.

## Implementation evidence

Commits on `split/verify-attribution` over reviewed candidate `4a6af5320`:
`a23b3dbbe` and its formatting follow-up `766341c87` (FIND-1),
`4073020e2` (FIND-5).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1: a non-empty sampled context can fill the admission ceiling and still seal and publish from the frame headroom | `wyrd-queue/src/queue.rs` `ProducerTrace` owns one context and its one `ClientByteBudget` charge; rows and the `SealedBatch` share it by `Arc`. `seal` reserves only `max_message_bytes` and resizes to the frame. `producer.rs` `enqueue_prepared` charges rows and context at admission. `SealedBatch::producer_traces` is `Vec<Arc<ProducerTrace>>`; the gRPC sink reads `trace.context()`. No new limiter, counter, option, wire field, or headroom change. | `producer::tests::admitted_rows_cannot_take_the_sealing_headroom` (extended with a traced record filling admission) FAIL before (`the frame comes from the sealing headroom: Backpressure`), PASS after | PASS |
| FIND-TASK-001-1: selected contexts stay charged through ambiguous retry and return to zero on terminal settlement | Batch holds the shared charged context until ACK, terminal loss, or shutdown | `producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry` PASS | PASS |
| FIND-TASK-001-1: unselected contexts and failed admission release normally | Contexts not selected drop with their rows; a refused admission drops its row and context guards | `queue::tests::seal_keeps_the_first_distinct_producer_contexts_in_order` now asserts only the 32 selected contexts stay charged after the rows drop; the retry test's refused second enqueue ends at `(0,0,0)` | PASS |
| FIND-TASK-001-5: Python types describe the canonical timeline output | `bifrost/_client.py`: every always-serialized `Resource`, `InstrumentationScope`, `SpanEvent`, `SpanLink`, `SpanRecord` field is required; only `parent_span_id`, `scope.version`, and `resource.service_{namespace,version,instance_id}` are omittable. `SpanStatus` is the union of `unset`, `ok`, and `error` variants; only `error` has `description`. Exports, methods, and runtime data unchanged. | `tests/typecheck/bifrost_contracts.py`: a complete record type-checks; a record missing `links` and `ok`/`unset` statuses with `description` are rejected. Against the previous types all three `ty: ignore` directives report `unused-ignore-comment` | PASS |

Non-goals held: no migration, task split, endpoint, query, wire field,
configuration, compatibility surface, or second resource owner; no change to
sampling, linked ingest, tenant routing, catalog projection, Scribe ownership,
timeline query, the first-32 rule, batch or retry identity, or Rust/TypeScript
SDK contracts; the `SpanData` clone and R1 outage proof are unchanged. The
Python generator was not restored and no runtime converter was added.

### Verification commands

```bash
mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=producer::tests::admitted_rows_cannot_take_the_sealing_headroom) | test(=producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry)'  # 2 run, 2 passed
mise exec -- cargo nextest run --locked -p wyrd-queue   # 59 run, 59 passed
mise run py:typecheck
mise run py:lints
mise run py:format
mise run test:shared                                    # 796 run, 796 passed
mise run fmt
mise run lints
git diff --check
```

| Command | Result |
|---|---|
| focused queue tests | PASS, 2 selected |
| `wyrd-queue` crate | PASS, 59 |
| `mise run py:typecheck` | PASS |
| `mise run py:lints` | PASS |
| `mise run py:format` | PASS |
| `mise run test:shared` | PASS, 796 |
| `mise run fmt` | PASS |
| `mise run lints` | PASS |
| `git diff --check` | PASS |

Status: `IMPLEMENTED`, routed to `$wyrd-task-review`.
