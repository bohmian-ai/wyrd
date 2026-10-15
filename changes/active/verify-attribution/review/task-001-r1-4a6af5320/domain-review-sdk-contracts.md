# SDK contract domain review

## Subject

- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Prior review/remediation: `changes/active/verify-attribution/review/task-001-827ec9f15/{findings-validation.md,verdict.md,TASK-001-R1-close-trace-continuity-gaps.md}`

This pass is limited to the first-class Rust, Python, and TypeScript timeline
contracts after both main merges. It independently traced the public return
values, native conversions, exports, test helpers, and action-timeline
journeys. It did not treat the prior completion summary as proof.

## Boundary and authority coverage

| Surface | Authority and source inspected | Result |
| --- | --- | --- |
| Shared Rust timeline owner | Spec REQ-008; `crates/shared/wyrd-client/src/bifrost/{facade,blocking}.rs`; canonical `wyrd_spec::vala::trace::SpanRecord`; strengthened timeline fixture | PASS: async and blocking clients return the canonical Rust type, preserve the authenticated query owner, deterministically sort, and retain one `(TraceId, SpanId)` identity. |
| PyO3 boundary | AGENTS.md §§7-8; `sdks/wyrd-sdk-python/src/bifrost/mod.rs`; `to_python_json` | PASS: the wrapper parses `TraceId` before IO, calls the shared client, and mechanically converts the canonical serde value into plain Python data without duplicating query behavior. |
| Python public runtime and exports | AGENTS.md §8; `python/wyrd/bifrost/{_client.py,__init__.py,__init__.pyi}`; public imports in integration support and the action journey | FAIL: runtime values and imports are coherent dictionaries/TypedDicts, but the authored output type does not precisely describe keys Rust always serializes (SDK-001). The move from unshipped generated dataclasses to plain dictionaries needs no compatibility shim or migration. |
| TypeScript native and public projection | TypeScript guide; `native/src/lib.rs`; generated `wyrd/index.d.ts`; `wyrd/src/{index.ts,card-types.ts}` | PASS: native code validates locally and serializes the shared Rust result; `traceTimeline` returns the generated canonical `SpanRecord[]`; required and omitted fields agree with serde output. |
| Python and TypeScript journey consumers | Testing workflow; Python `tests/integration/{support.py,test_action_timeline.py}`; TypeScript `tests/support/server.ts` and `tests/integration/verify-in-real-time.test.ts` | SOURCE PASS / EXECUTION UNPROVED: both journeys consume the current shapes and still assert ordering, ancestry, producer/consumer links, gateway attempts, and verification ancestry. Neither language journey was rerun on this candidate after the merges. |

## Public value trace

1. `wyrd_client::Bifrost::trace_timeline` returns
   `Vec<wyrd_spec::vala::trace::SpanRecord>` after the shared query, ordering,
   and identity retention.
2. Python's `NativeBifrost::trace_timeline` serializes exactly that vector and
   `to_python_json` constructs ordinary Python dictionaries. Sync and async
   `Bifrost.trace_timeline` return those values directly under the public
   `wyrd.bifrost.SpanRecord` TypedDict annotation.
3. TypeScript's native `traceTimeline` serializes the same vector into one
   `NativeLifecycleResult`; the public wrapper parses it as the generated
   `SpanRecord[]` exported from `wyrd-ai`.

No second query, tenant bypass, SDK-local deduplication, or language-specific
timeline model was introduced.

## Material proposed finding

### SDK-001 — INCORRECT: Python marks always-emitted canonical span fields as optional

- Violated obligation: spec REQ-008 and task Scenario 12 require Python to
  project the canonical `SpanRecord`; AGENTS.md §8 requires precise public
  Python typing. The Python and TypeScript projections of the same returned
  value must agree on which keys exist.
- Location:
  - `sdks/wyrd-sdk-python/python/wyrd/bifrost/_client.py:1319-1403`
  - Canonical producer: `crates/wyrd-spec/src/vala/trace/span.rs:15-63`,
    `span_link.rs:33-58`, `resource.rs:24-43`, and
    `instrumentation_scope.rs:23-35`
  - Correct sibling projection:
    `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:6010-6135`
- Evidence: Python places `attributes` on `Resource` and
  `InstrumentationScope`, the defaulted event/link fields, and
  `SpanRecord.attributes`, counters, `events`, `flags`, `links`, and
  `trace_state` in `total=False` bases. The canonical Rust fields use
  `#[serde(default)]` without `skip_serializing_if`, so serialization always
  includes those keys. The generated TypeScript contract correctly marks the
  same keys required. Only `parent_span_id`, optional resource identities,
  scope `version`, and error-status `description` are conditionally omitted.
- Reachability and consequence: every Python sync and async timeline caller
  receives this public TypedDict. Runtime data is currently complete, but the
  public Python contract says callers must handle absence for keys guaranteed
  by Rust, so Python no longer projects the same canonical output contract as
  Rust and TypeScript and static consumers cannot rely on REQ-008's named
  fields.
- Smallest testable correction: keep the dictionary/TypedDict representation
  and all runtime conversion unchanged. Declare the always-serialized keys as
  required on `Resource`, `InstrumentationScope`, `SpanEvent`, `SpanLink`, and
  `SpanRecord`; retain only serde-omitted keys as `NotRequired`/the existing
  `total=False` base. Add one focused public-type assertion for the required
  key sets (or extend the nearest existing public typing test) and run
  `py:lints`, `py:typecheck`, and the single Python action-timeline journey.
  Do not restore the deleted generator, generated classes, or a compatibility
  wrapper.

## Verification limits

- Reported green on candidate `4a6af5320`: format, lints, Python/TypeScript
  typechecks, `codegen:check`, and docs checks.
- The focused remediation tests and owner journeys were green before the two
  main merges.
- Missing on the immutable candidate: the Python action-timeline journey, the
  TypeScript action-timeline journey, and the Rust timeline/remediation focused
  tests. Because the merges changed both language timeline projections and
  their consumers, the earlier runtime results do not prove the candidate.
- This reviewer ran no Cargo or mise command, as required while the delegated
  review was parallel. Source inspection is not a substitute for those two
  language journeys.

## Overall result

**FAIL** — one bounded public typing mismatch (`SDK-001`) remains. Independently
of that source finding, current Python and TypeScript runtime journey evidence
is still required for the final candidate.
