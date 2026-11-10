---
id: CHANGE-card-authoring-R2
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 6
spec_approval: approved
findings: [FIND-CHANGE-001]
---

# Card-authoring R2 findings and remediation

This is the single implementation packet for every outstanding finding and
required gate correction in CHANGE-review-02. Complete both corrections below
before requesting re-review. The verdict remains FIX_REQUIRED.

Nested Python Cards and references now reach the shared Rust capture owner,
but the new recursive converter crashes the Python interpreter on circular or
excessively nested containers. Keep the successful composition and restore the
ordinary-JSON refusal behavior at that same conversion boundary. The other
four findings and the PTY correction remain closed. A separate test-comparison
cause also leaves the selected TypeScript gate red; its diagnosis, correction
and proof are included below. This packet adds no new authoring feature or
configurable resource policy.

## Authority and reviewed context

- Approved spec: changes/active/card-authoring/spec.md, revision6.
- Original tasks: tasks/TASK-001-common-rust-authoring.md,
  TASK-002-python-authoring.md, TASK-003-typescript-authoring.md and approved
  R3/R5/R6 follow-ons; historical mechanics yield to revision6.
- Original integrated base:5234961d51798581a026834cfffd5efaadc226b4.
- Prior candidate:17916872ad526d3f3b474c86534d6227e2bc922c.
- Reviewed current candidate:8698f99a6cae6925ef2512a830713632dec3e786.
- Original finding and R1:
  review/CHANGE-review-01/integrated-review.md and
  CHANGE-R1-cross-sdk-authoring-parity.md.
- Independent current source diagnosis, acceptance/carry-forward and execution:
  this directory's integrated-review.md, verdict.md and verification.md.

Preserve the stable FIND-CHANGE-001 identity and prior findings/closure evidence.
Route this one packet directly to wyrd-implement under root/Python/TypeScript
AGENTS, Rust/errors/
maintainer guides and the approved in-memory authoring architecture.

## Finding disposition and correction scope

| Finding or blocker | Disposition | Required work |
|---|---|---|
| FIND-CHANGE-001: nested Python values | Partially closed; capture works, safe refusal regressed | Restore bounded conversion and validation errors as diagnosed below. |
| FIND-CHANGE-002: Prompt null fields | Closed | Preserve constructor/setter None handling and canonical null. |
| FIND-CHANGE-003: generic constructor | Closed | Preserve twelve named kinds and removal of the public generic constructor. |
| FIND-CHANGE-004: path keyword | Closed | Preserve path= and existing declaration options. |
| FIND-CHANGE-005: positive media fixture | Closed | Preserve the canonical fixture and all admission assertions. |
| Previous PTY gate blocker | Closed by the full local-development lane | Preserve master-descriptor observation and all signal/echo/secrecy assertions. |
| Timeline-order assertion gate blocker | Open; test comparison is incorrect | Correct precise chronological comparison as diagnosed below. |

Implementation ownership is the Python argument-conversion boundary and its
existing unit tests, plus the TypeScript timeline journey comparison and its
focused proof. No product timeline change is needed. Detailed accepted closure
and complete specification coverage remain in integrated-review.md; current
execution evidence remains in verification.md.

## Diagnosis and remediation: FIND-CHANGE-001

The original requirement was to carry native objects nested in canonical
Python mappings/sequences while preserving ordinary JSON trust-boundary
refusals. REQ-002 permits ordinary host-argument errors; REQ-008/009 and
AC-002/005 retain canonical adaptation and Python error behavior. Invalid
containers must not terminate their entire Python host.

At sdks/wyrd-sdk-python/src/authoring.rs:186-193, field handles mappings and
list/tuple by recursing before its json fallback. object at :202-220 converts
each mapping value through field. binding and canonical! also call object
directly for root canonical mappings; Check.drift now uses field. These paths
do not use Python recursion protection or a nesting budget.

Previously ordinary containers reached json -> observe::json_value ->
json.dumps and serde_json::from_str. Circular inputs were caught by the
serializer and mapped through invalid_argument to WYRD_SPEC_400_VALIDATION;
the parser also retained serde_json's conventional finite nesting bound.
The changed traversal bypasses both controls for containers.

Independent isolated subprocesses against the current installed extension
reproduced signal11 exits for a dict containing itself, a list containing
itself, and 4000 nested plain dicts passed to Action.agent. A current unchanged
JSON-only Prompt.system control safely returns WYRD_SPEC_400_VALIDATION for
the cyclic dict. NaN still refuses through the validation family and an
acyclic child reused at two keys still constructs. This establishes an actual
fix-induced failure of the required error boundary, not speculative graph-cycle
semantics or a new product expectation. The failure precedes network/domain
validation and can kill unrelated work in the same Python application.

## Selected minimum correction

Preserve the new FieldValue/FromIterator composition, and add bounded local
container traversal consistent with the existing ordinary-JSON conversion's
accepted nesting bound. Installed serde_json1.0.151 initializes its normal
deserializer nesting budget to128 and refuses exhaustion; use that established
boundary rather than inventing a public limit, option or configuration.
Calibrate container counting against the existing JSON-only path so previously
accepted ordinary inputs remain accepted and excessive nesting refuses
consistently.

Place the protection at the Python argument-conversion owner, covering both
field recursion and canonical/binding root-object entrypoints. A guard only
on Action.agent or declaration would leave sibling callers exposed or run
after the host had already crashed. Finite nesting refusal also safely stops
direct/indirect cycles; no ancestor identity graph or global visited/cache
state is needed. Keep acyclic shared containers valid.

Translate refusal through existing invalid_argument and its stable
WYRD_SPEC_400_VALIDATION fields. Preserve non-str key and nonfinite-scalar
refusals, deferred semantic validation, and successful nested Card/CardRef/
ArtifactFile/CodeSource captures, flatten paths, duplicates and replacement.
Conversion must still finish outside the receiver borrow before assignment;
user mapping/schema callbacks must remain reentrant as currently tested.
No invalid conversion may partially replace an existing receiver field.

PyO3 0.29.3 has no safe enter-recursive-call wrapper; its only corresponding
API is unsafe FFI. Workspace unsafe_code is denied. Do not add direct
Py_EnterRecursiveCall/Py_LeaveRecursiveCall calls, an allow override, a new
dependency, a generic graph/validator framework, duplicate JSON round trips
or duplicate callback evaluation to solve this small boundary failure.
Private helper shape/control flow remains implementation-owned.

The correction is bounded source-side protection at the existing traversal.
Deleting composition would restore the original nested-native defect; adding
consumer guards in individual Card families would duplicate the cause; a
later Graph/loader check cannot recover an interpreter already terminated.

## Preserved behavior and excluded work

Keep all four other FIND-CHANGE closures and the PTY master observation:
Prompt None replacement, twelve named kinds/erased conversion with no generic
public constructor, path keyword/options, canonical positive media fixture,
all terminal/signal/secrecy assertions and existing publication/state owners.
No wire/schema/dependency/feature, scheduler, registry/upload, security/tenancy/
persistence, migration or compatibility change is authorized.

Do not expand this task into arbitrary user-callback policing, exhaustive new
JSON semantics, all-language depth normalization or unrelated platform/gate
cleanup beyond the diagnosed timeline assertion correction below. Do not reinterpret host-serialization refusal as eager domain
validation. Preserve ordinary malformed-but-representable definitions until
the existing canonical validate/declaration boundary.

## Acceptance and focused proof

Use actual Python-runtime subprocess tests in the existing authoring unit
file, with bounded subprocess completion, so a regression cannot take down
the parent test runner. Confirm a newly added selector before reporting it.
The proof must establish:

1. Direct dict/list cycles and an indirect mapping/sequence cycle safely
   raise WYRD_SPEC_400_VALIDATION, with normal child exit and no signal
   termination or timeout.
2. Excessive finite nesting refuses safely, and supported boundary-depth
   ordinary inputs agree with the existing JSON-only control. Count root
   canonical objects and nested list/tuple containers consistently.
3. The protection reaches direct field calls and canonical/binding mapping
   entrypoints through their shared owner, rather than only the original
   probe's Action.agent call.
4. Acyclic repeated containers still compose by value; nested native capture,
   callback-safe mutation, replacement/live-field/duplicate/file preparation
   and existing error-family tests still pass.
5. Valid but domain-incomplete JSON remains constructible and is refused
   only by existing semantic validation; no user-visible option or unsafe
   exception was added.

Rebuild the changed Python extension through py:setup once. Run the exact new
subprocess-regression selector after collection confirmation and the existing
authoring/export unit files:

    mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/cards/test_authoring.py tests/unit/test_public_exports.py

Run py:format:check, py:lints, py:typecheck, fmt:check, lints and check:deps
for the actual PyO3/test write set; keep Cargo-backed commands sequential.
Retain required Rustdoc on changed helpers. No shared behavior/wire output is
expected to change; regenerate only consumed outputs actually affected,
never hand-edit generated artifacts. Finish with mise exec -- git diff --check.

No new server journey or full user-journey suite is required for this local
conversion correction: existing real-server canonical parity is accepted
evidence, and this failure occurs before publication. Full integrated lanes
belong to change review. Record the exact source correction and subprocess
outcomes under FIND-CHANGE-001, preserving current review and cumulative target
for bounded closure and affected-path review.

## Diagnosis and remediation: timeline-order assertion gate blocker

The full integrated run at8698f99a6cae6925ef2512a830713632dec3e786 failed one
TypeScript assertion: action-timeline.test.ts > service action timeline is one
connected tree, line103. All other selected lanes passed. Correct this test's
comparison without changing the timeline product or weakening its ordering
and connected-tree assertions. This is a separate gate blocker, not another
authoring product finding or a reason to reopen accepted authoring behavior.

### Evidence and existing owners

Read this directory's verification.md and integrated-review.md, root and
TypeScript AGENTS and applicable testing rules. The selected graph exited1;
TypeScript integration reported263 passed,1 failed,3 skipped. Its actual
timeline contained 2026-11-10T01:59:44.450Z before
2026-11-10T01:59:44.450085Z. That is correct chronological order, but the test's
default string sort reverses the pair. A deterministic Node assertion confirms
the failure independently of timing or server load.

The shared Bifrost::trace_timeline in facade.rs sorts chrono DateTime<Utc>
values with trace/span tie-breakers. rows.rs reads precise nanosecond instants;
SpanRecord.start_time retains DateTime<Utc>. N-API serializes this existing
result, whose chrono AutoSi format permits variable0/3/6/9 fractional digits.
The TypeScript facade and test support preserve returned order. Independent
review found no product-order defect or existing precise TS comparator.
The failing test and sort owner are unchanged in the cumulative authoring diff.

### Smallest correction

Own only the timestamp-order comparison in
sdks/wyrd-sdk-ts/wyrd/tests/integration/action-timeline.test.ts and the minimum
focused proof for its normalization. Normalize the existing UTC-Z timestamp
fraction to fixed nine-digit precision for comparison, including timestamps
without a fraction. Keep comparing the returned sequence against its sorted
copy; do not sort away the returned order before asserting it. Preserve all
trace ancestry, root, link and tenant assertions in the journey.

Do not use Date.parse or JavaScript Date as the sole comparison value: both
recorded timestamps collapse to the same millisecond, losing the property
under test. Do not change server serialization, timestamp wire contracts,
product sorting or time policy; add no dependency or general timestamp
framework. Private in-file mechanics remain implementation-owned.

### Acceptance and focused proof

Provide small deterministic comparison assertions covering the recorded
mixed-precision pair, no fraction, and supported3/6/9-digit fractions. Include
sub-millisecond and sub-microsecond differences so reversed precise chronology
cannot pass accidentally. Keep the real journey's original ordering property.

Use existing mise tasks and installed runtime. Run the deterministic proof and
the exact service action timeline selector against repository Postgres through
the existing scripts/postgres/with-test-postgres.sh lifecycle; confirm the
selector collects the intended test. Run ts:format:check, ts:lints and
ts:typecheck for the changed test, plus git diff --check. No native rebuild is
needed solely for this test-only correction when the reviewed binaries remain
current. Record actual commands/outcomes; a retry of the unchanged flaky
assertion is not closure. Full integrated lanes belong to the next change
review.

Complete this correction together with FIND-CHANGE-001 under this single
packet. Record the source correction and focused proof for each cause in the
implementation evidence. No source correction, stage, commit or push was
performed by this review. Preserve all prior accepted closures.

## Implementation evidence

Commits: `0f501c02f` (FIND-CHANGE-001), `bbdd0316b` (timeline comparison).

### FIND-CHANGE-001

Source correction: in `sdks/wyrd-sdk-python/src/authoring.rs`, `field` now
delegates to `nested(field, value, depth)`. Every mapping (in `object`) and
every list or tuple passes through `enter`. `enter` refuses past `MAX_DEPTH`
(127, the deepest nesting serde_json's 128 budget accepts) through
`invalid_argument`, so the error is `WYRD_SPEC_400_VALIDATION`. The
`binding` and `canonical!` root mappings enter at depth 0. Composition, the
conversion order (before the receiver borrow) and the callbacks are unchanged.
No unsafe code, dependency, option or JSON round trip was added.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Direct dict, list and indirect cycles refuse with a normal exit | `enter` in `object` and `nested` | `test_circular_and_deep_containers_refuse_without_crashing`: the subprocess exits 0 within 120 s, with no signal | PASS |
| Excessive nesting refuses; boundary depth agrees with the JSON-only control | `MAX_DEPTH = 127` | Same test, depths 126/127/128/4000 × list/tuple/dict, compared with `Prompt.system` | PASS |
| Protection reaches `field`, the canonical roots and the binding root | Shared `enter` | Same test: `Action.agent`, `Service.add_component`, `Component.verified_by` | PASS |
| Acyclic shared containers compose; a refusal leaves the field unchanged; existing behaviour holds | Unchanged composition | Same test, plus `tests/unit/cards/test_authoring.py` and `tests/unit/test_public_exports.py`: 43 passed | PASS |
| Valid but domain-incomplete JSON stays constructible and is refused at `validate()` | Unchanged | Same test: `add_component({"alias": "judge"})` | PASS |

The selector was confirmed with `pytest --collect-only` and then run exactly.
`py:setup` was rebuilt once.

Passing lanes: `py:format:check`, `py:lints`, `py:typecheck`, `fmt:check`,
`lints` and `check:deps`.

### Timeline-order assertion

Source correction: `instant()` in
`sdks/wyrd-sdk-ts/wyrd/tests/integration/action-timeline.test.ts` pads the
UTC `Z` fraction to nine digits; a missing fraction becomes `.000000000`. The
journey still asserts that the returned sequence equals its sorted copy. The
ancestry, root, link and tenant assertions are unchanged. No product,
serialization or dependency change was made.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Deterministic mixed-precision proof | `it("timestamps compare at nanosecond precision")` | Covers the recorded pair (raw sort reverses it), no fraction, 3/6/9 digits, and sub-ms/sub-µs gaps | PASS |
| The real journey keeps its ordering property | `instant(span.start_time)` | `with-test-postgres.sh` + `vitest run tests/integration/action-timeline.test.ts -t "timestamps compare at nanosecond precision\|service action timeline is one connected tree"`: 2 passed, 2 filtered out | PASS |

Passing checks: `ts:format:check`, `ts:lints`, `ts:typecheck` and
`git diff --check`. The native addons were not rebuilt: this correction
changes only a test.

Non-goals stayed excluded. Only the two source files and this packet changed.
