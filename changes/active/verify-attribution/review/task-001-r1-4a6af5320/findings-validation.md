# TASK-001 R1 findings validation

## Immutable subject

- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Prior reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Approved specification: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Prior ledger and verdict: `changes/active/verify-attribution/review/task-001-827ec9f15/{findings-validation.md,verdict.md}`
- Remediation task: `changes/active/verify-attribution/review/task-001-827ec9f15/TASK-001-R1-close-trace-continuity-gaps.md`

The candidate tree was unchanged when this validation completed. I read every
discovery report and the focused follow-up, then checked the cited producers,
owners, sibling paths, public projections, and focused tests against current
source. Agreement between discovery reports was not treated as proof.

## Proposal validation

| Proposed issue | Validation | Reason |
| --- | --- | --- |
| `BR-R1-001`, `INV-R1-001`, `STD-R1-001`, `CD-001`: sealing charges selected producer contexts twice | **REVISED; prior `FIND-TASK-001-1` reopened** | Admission owns the context bytes while `seal` asks the fixed frame headroom to reserve the same bytes again. The defect is reachable at the documented admission ceiling and is the incomplete staged-to-sealed ownership transition of the prior finding, not a new independent finding. |
| `PD-R1-001`, `SDK-001`, `FOLLOWUP-R1-001`: Python's authored timeline type is broader than the canonical output | **CONFIRMED as new `FIND-TASK-001-5`** | The native boundary mechanically serializes canonical Rust `SpanRecord`s, but the public Python `TypedDict` makes always-emitted keys optional and collapses the tagged status enum into shapes Rust cannot emit. |
| Prior `FIND-TASK-001-2`: nonadjacent duplicate span identities survive timeline sorting | **CLOSED** | The shared Rust facade now retains the first `(TraceId, SpanId)` with one local `HashSet` after the required total-order sort, and the fixture separates differing-time copies with an intervening span. |
| Prior `FIND-TASK-001-3`: no same-outbox outage/recovery proof | **CLOSED** | The role-separated integration test holds back the sole Scribe, drives the retained Oracle outbox to the 2,048-copy cap, exercises sibling HTTP/gRPC/MCP and verifier-result work, starts Scribe, drains that same outbox, and proves later admission. |
| Prior `FIND-TASK-001-4`: `stored` lost its rustdoc | **CLOSED** | `fixed_width` and `stored` now each have their correct adjacent purpose and `# Errors` documentation. |
| Stale task/remediation frontmatter, function-local test imports, and commit trailers | **REJECTED as task findings** | They do not alter an accepted behavior, security boundary, tenancy rule, durability property, public contract, or reachable implementation path. Rewriting the immutable candidate or creating a style-only remediation is not justified. |
| Accepted linked-route `SpanData` clone | **REJECTED as a finding** | It remains one localized, non-retained snapshot used by the existing processor boundary, adds no second mechanism, and the owner explicitly accepted this bounded simple cost. |

## Validated finding ledger

### FIND-TASK-001-1 — REVISED (reopened) — INCORRECT / REGRESSION: a fully admitted traced staging set cannot seal

- **Discovery sources:** `BR-R1-001`, `INV-R1-001`, `STD-R1-001`, and
  `CD-001`.
- **Violated obligation:** the prior remediation requires the existing
  `ClientByteBudget` to own producer-context memory continuously from admission
  through terminal settlement without weakening the existing guarantee that
  one reserved `max_message_bytes` of sealing headroom can frame every admitted
  row set. `QueueConfig` and `ClientByteBudget::for_config` reserve exactly that
  frame headroom.
- **Exact locations:**
  - `crates/shared/wyrd-queue/src/producer.rs:84-95,545-568`
  - `crates/shared/wyrd-queue/src/queue.rs:33-48,546-603,672-696`
  - incomplete proof at
    `crates/shared/wyrd-queue/src/producer.rs:1336-1418,2037-2068`
- **Producer-to-consumer trace:** `Producer::enqueue_prepared` reserves the row
  arrays plus `context_bytes` and stores that one live guard on every row of
  the logical record. `RecordQueue::seal` drains those rows but keeps their
  guards live, clones the selected first-32 contexts, and calls
  `reserve_sealing(max_message_bytes + trace_bytes)`. At
  `admission_limit()`, only `max_message_bytes` remains, so any non-empty
  selected context makes the reservation fail. The queue restores the same
  guarded rows, leaving the next flush with identical arithmetic and no owner
  able to release space. Every traced JSON `insert`, `record`, and `observe`
  producer reaches this shared path; context-free rows and the separate
  caller-owned `Entry::Batch` path are unaffected.
- **Observable consequence:** valid traced work can be accepted under the
  documented budget and then remain permanently unpublishable. `flush` and
  `shutdown` return backpressure while the accepted rows and their context
  stay resident. The fresh green queue test does not close this path because
  it deliberately leaves enough admission slack for the second context
  reservation.
- **Decision-complete smallest correction:** keep the one
  `ClientByteBudget`, current public configuration, first-32 selection, wire
  representation, batch identity, and retry settlement. At the existing
  staged-to-sealed transition, retain or transfer the selected contexts'
  already-admitted charged ownership into the sealed batch and use the fixed
  sealing headroom only for the IPC frame; do not hold both the staged context
  charge and a second sealed-context charge at once. One charge must follow
  each retained context allocation until ACK, terminal loss, or shutdown.
  Do not enlarge the budget/headroom, add a limiter or option, or weaken
  context accounting.
- **Focused closure proof:** extend the existing
  `producer::tests::admitted_rows_cannot_take_the_sealing_headroom` case with a
  non-empty sampled producer context, fill admission to its documented ceiling,
  and prove one flush publishes. Retain and run
  `producer::tests::producer_context_bytes_stay_bounded_through_ambiguous_retry`
  to prove the selected context stays charged across retry and the budget
  returns to zero after terminal settlement. Both are unit tests in the
  existing queue harness; no new fixture or lane is warranted.

### FIND-TASK-001-5 — CONFIRMED — REGRESSION: Python does not declare the canonical timeline output precisely

- **Discovery sources:** `PD-R1-001`, `SDK-001`, and
  `FOLLOWUP-R1-001`.
- **Violated obligation:** specification REQ-008 and task Scenario 12 require
  Python and TypeScript to project the same canonical `SpanRecord`; AGENTS.md
  and `python-api-and-stubs.md` require precise authored public types.
- **Exact locations:**
  - `sdks/wyrd-sdk-python/python/wyrd/bifrost/_client.py:1319-1403`
  - producer at `sdks/wyrd-sdk-python/src/bifrost/mod.rs:789-794`
  - canonical types at
    `crates/wyrd-spec/src/vala/trace/span.rs:22-70,193-206` and the adjacent
    `resource.rs`, `instrumentation_scope.rs`, `span_event.rs`, and
    `span_link.rs` definitions
  - focused type fixture at
    `sdks/wyrd-sdk-python/tests/typecheck/bifrost_contracts.py`
- **Producer-to-consumer trace:** the shared Rust facade returns canonical
  `Vec<SpanRecord>`. The PyO3 method serializes that value directly with
  `serde_json::to_value`, and both public Python clients return the resulting
  dictionaries under `list[SpanRecord]`. Serde always emits fields marked
  `default` without `skip_serializing_if`, including attributes, flags,
  trace-state, events, links, and all dropped counts. Python instead places
  those fields in `total=False` bases for `Resource`,
  `InstrumentationScope`, `SpanEvent`, `SpanLink`, and `SpanRecord`.
  `SpanStatus` also permits `description` on `ok` and `unset`, while the Rust
  producer and TypeScript sibling use a three-variant tagged union in which
  only `error` may contain it. No runtime converter normalizes either mismatch.
- **Observable consequence:** every sync and async Python timeline caller gets
  complete canonical dictionaries but is told that guaranteed fields may be
  absent and that impossible status records are valid. Python therefore
  exposes a weaker public contract than Rust and TypeScript for the same task
  API. The fresh green Python journey proves runtime transport, not the static
  key and variant contract.
- **Decision-complete smallest correction:** keep the current plain
  dictionaries, native JSON conversion, method signatures, exports, and
  inline authored definitions. Move every always-serialized field into its
  required `TypedDict` body; keep only fields serde conditionally omits as
  `NotRequired`/an optional-key base with their emitted value type. Replace
  the single broad `SpanStatus` dictionary with a union of the three existing
  wire variants: `unset` and `ok` contain only `code`, while `error` has an
  optional `description` key. Do not restore the deleted generator or add
  dataclasses, conversion helpers, aliases, validation, or compatibility
  behavior.
- **Focused closure proof:** extend
  `sdks/wyrd-sdk-python/tests/typecheck/bifrost_contracts.py` so one complete
  canonical record type-checks, omission of an always-emitted key is rejected,
  and `ok`/`unset` statuses carrying `description` are rejected. Run
  `mise run py:typecheck` and `mise run py:lints`. The current-candidate Python
  action-timeline journey already passes and no additional runtime harness is
  needed for this declaration-only correction.

## Prior-finding closure and verification evidence

| Finding | Source closure | Current-candidate proof | Result |
| --- | --- | --- | --- |
| `FIND-TASK-001-1` | Admission accounting exists, but sealing overlaps the same context charge with a second reservation | `producer_context_bytes_stay_bounded_through_ambiguous_retry` passed 1/1 but does not saturate admission before sealing | **REOPENED / REVISED** |
| `FIND-TASK-001-2` | Sort followed by one local identity set; strengthened nonadjacent fixture | focused Postgres timeline test passed 1/1 | **CLOSED** |
| `FIND-TASK-001-3` | Existing cluster seam exercises one real outbox through outage and recovery | focused server integration test passed 1/1 | **CLOSED** |
| `FIND-TASK-001-4` | Correct rustdoc restored on both helpers | current format/lint evidence is green | **CLOSED** |
| `FIND-TASK-001-5` | Authored Python declarations remain broader than the canonical producer | Python action-timeline passed 1/1; this does not test static required keys or union discrimination | **OPEN** |

Fresh candidate evidence also reports the Rust, Python, and TypeScript
action-timeline journeys passing 1/1 each. That closes the earlier post-merge
runtime uncertainty. It does not exercise the full-admission traced sealing
edge or Python static type precision, which are the two retained findings.

## Validated empty claims and rejected contradictions

The security/tenancy report's empty ledger is supported: tenant and principal
still derive only from verified credentials, producer contexts cannot select
either, catalog projection remains allowlisted, and timeline SQL stays on the
authenticated tenant-qualified Oracle path. The system report's empty ledger
is supported outside the queue sealing defect: Scribe outage, retry, recovery,
shutdown, and span-permit release preserve the approved non-blocking boundary.

The maintainer report's claim that `FIND-TASK-001-1` closed is rejected because
it considered admission and retry accounting separately but did not reconcile
their simultaneous reservations at a full admission ceiling. The standards
report's Python pass is narrowed by the two SDK-domain reports and resolved by
the focused follow-up: exports and runtime conversion are coherent, but the
public authored output declaration is not canonical.

## Specification impact

Neither retained finding needs a specification revision. Both corrections are
bounded inside already-approved revision 3: complete the existing queue
ownership transfer without new machinery, and make the existing authored
Python type accurately describe the existing canonical output.
