# Maintainer review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/verify-attribution`
- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Prior reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Remediated candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Result: **PASS**

The candidate remained immutable during this pass. I reviewed the complete
cumulative task surface and used the remediation commits to inspect the four
corrected owners closely. The two later main merges do not change any of the
seven Rust files in the remediation write set; their Python and TypeScript
changes were reviewed where they project TASK-001's public timeline contract.

## Changed-surface coverage

| Surface | Symbols, callers, and tests inspected | Maintenance assessment |
|---|---|---|
| Queue byte ownership | `ClientByteBudget::{reserve,reserve_sealing}`, `ClientByteGuard`, `Producer::enqueue_prepared`, `Row`, `RecordQueue::seal`, `producer_traces`, `context_bytes`, `SealedBatch::producer_traces`, ambiguous-retry settlement, focused producer and queue tests | The correction extends the existing sole byte owner. One context is charged once while its record rows share it; each sealed context clone is charged on the existing frame guard and remains there through retry. It adds no limiter, configuration, context wrapper, or settlement path. The small `context_bytes` helper names the one repeated sizing rule used at admission and sealing. |
| Timeline identity | `Bifrost::trace_timeline`, blocking/PyO3/N-API callers, the strengthened Postgres fixture, and Rust/Python/TypeScript timeline consumers | A local standard-library `HashSet` after the existing deterministic sort is the smallest complete correction. Deduplication remains in the shared Rust facade and is not repeated by either language SDK. The method documentation now states which duplicate survives. |
| Scribe outage proof | `sustained_scribe_outage_caps_tenant_spans`, its metric, published-row, MCP, and gRPC probes, the role-separated cluster's delayed-node/restart seams, the retained process outbox, and existing test-support APIs | Production is unchanged. The test uses the existing cluster lifecycle and the same outbox instance before and after the Scribe node joins. Its local helpers each isolate one assertion boundary and avoid adding a production fault switch, generic fault framework, second outbox, or test-only dependency. Mirroring the fixed 2,048 contract in the test is clearer and smaller than exposing a production-private permit constant solely to tests. |
| Verifier SQL documentation | `fixed_width`, `stored`, and the identifier decoders that call them | Each adjacent helper again has one accurate purpose and one accurate `# Errors` section. The correction changes no runtime behavior and restores the required rustdoc without a documentation-only abstraction or test. |
| Python timeline projection after main merge | native `Bifrost::trace_timeline`, pure-Python sync/async methods, `SpanRecord` and nested `TypedDict`s, public `wyrd.bifrost` runtime/stub exports, shared `timeline` helper, and `test_action_timeline` | Returning the native JSON mappings directly and describing them once as public inline `TypedDict`s matches the repository's new authored-stub policy and deletes the removed generator's conversion layer. Sync and async use the same native owner, field names stay canonical, and the journey consumes the public mapping shape. No second Python model or validation path was introduced. |
| TypeScript timeline projection after main merge | generated `card-types.ts::SpanRecord`, `Bifrost.traceTimeline`, native lifecycle projection, shared `timeline`/`ancestry` helpers, and the action-timeline assertions now colocated in `verify-in-real-time.test.ts` | The wrapper remains one typed N-API call over the shared Rust facade. `SpanRecord` comes from the repository's canonical generator and is re-exported once; timeline and ancestry helpers remove repeated journey code. The merge adds no task-specific DTO, adapter, or second read path. |
| Original trace-continuity owners | telemetry sampler/router/export, `serve_request`, Gate producer links, `ServerSpanSink`, first-class SDK propagation, and action-timeline consumers | Source still has one parent-or-sampled-link eligibility rule, one request-serving entry point, one Scribe writer, and one shared timeline facade. The previously accepted localized `SpanData` snapshot remains documented and introduces no parallel route opener or speculative channel. |

## Prior-finding closure

| Prior finding | Source evidence | Result |
|---|---|---|
| `FIND-TASK-001-1` | `Producer::enqueue_prepared` includes context strings in the record reservation; `RecordQueue::seal` includes selected cloned contexts in the frame guard retained by `SealedBatch` | **CLOSED** |
| `FIND-TASK-001-2` | `Bifrost::trace_timeline` sorts once and retains the first `(TraceId, SpanId)` through one local `HashSet`; the fixture separates duplicate copies with another span | **CLOSED** |
| `FIND-TASK-001-3` | The cluster test holds back Scribe, drives the actual Oracle process outbox to its span cap, probes sibling capabilities, joins Scribe, settles that same outbox, and accepts a later copy | **CLOSED** |
| `FIND-TASK-001-4` | `fixed_width` and `stored` each have their own correctly placed rustdoc and error contract | **CLOSED** |

## Material findings

None.

## Non-blocking notes

- `TASK-001` still says `status: proposed` and `spec_revision: 2`, while its
  evidence and the approved authority use revision 3; `TASK-001-R1` still says
  `status: ready` while its evidence says `IMPLEMENTED`. These are stale review
  metadata, not a maintainability defect in a changed implementation symbol,
  and they should be reconciled by the review/completion lifecycle rather than
  by adding code or another task.
- The outage proof is necessarily long because it establishes one real
  cross-process failure/recovery story. Its helpers are local, have single
  callers, and replace no reusable harness capability, so extracting a new
  fixture layer would add more machinery than it removes.

## Verification limits

The recorded focused tests and owner lanes passed before the two main merges.
After those merges, format, lints, typecheck, codegen, and docs checks passed,
but the three focused remediation tests and the Python/TypeScript timeline
journeys were not rerun on `4a6af5320`. This source review did not run Cargo,
mise, Postgres, Python, or TypeScript commands in parallel with the other
reviewers; current runtime closure of the merged SDK projections remains for
the orchestrator's verification assessment.
