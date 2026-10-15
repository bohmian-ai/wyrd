# Focused follow-up: Python timeline contract

## Subject and uncertainty

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Approved specification: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`

This pass resolves only the disagreement about whether the post-merge Python
`trace_timeline` type is a coherent conservative projection or a material
public-contract regression. It separately resolves the `SpanStatus` tagged-
union claim. It does not reassess the queue charge finding.

## Source path inspected

1. `crates/shared/wyrd-client::Bifrost::trace_timeline` returns
   `Vec<wyrd_spec::vala::trace::SpanRecord>`.
2. `sdks/wyrd-sdk-python/src/bifrost/mod.rs:789-794` serializes that exact
   value with `serde_json::to_value` and converts it mechanically to Python
   dictionaries through `to_python_json`; no Python-side normalization or
   omission occurs.
3. The canonical Rust definitions were inspected in
   `crates/wyrd-spec/src/vala/trace/{span,resource,instrumentation_scope,span_event,span_link}.rs`.
   Fields carrying `#[serde(default)]` but no `skip_serializing_if` are always
   emitted. Only `parent_span_id`, the optional resource service fields,
   instrumentation-scope `version`, and error-status `description` can be
   omitted.
4. Both public Python methods return the native dictionaries directly and are
   annotated as `list[SpanRecord]`. The public runtime and stub exports in
   `python/wyrd/bifrost/{__init__.py,__init__.pyi}` re-export the single inline
   definition from `_client.py`.
5. `sdks/wyrd-sdk-python/python/wyrd/bifrost/_client.py:1319-1403` puts
   canonical always-emitted keys in `total=False` bases for `Resource`,
   `InstrumentationScope`, `SpanEvent`, `SpanLink`, and `SpanRecord`.
6. The same file models status as one dictionary whose `code` is
   `Literal["unset", "ok", "error"]` and whose optional `description` is
   legal for every code.
7. The generated TypeScript sibling at
   `sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:6057-6164` marks the always-emitted
   fields required and models status as the canonical `unset | ok | error`
   tagged union.
8. `architecture/references/languages/python-api-and-stubs.md` makes the
   inline authored declaration authoritative and requires precise public
   types. Task Scenario 12 requires Python and TypeScript to project the same
   canonical type with the same fields; no generator or Python value class is
   required by the current authority.

## Resolution

### Required-key claim: material and reachable

The optional-key declarations are not a harmless conservative input type.
`SpanRecord` and its children describe the output of the public sync and async
timeline methods, and their docstrings say so; Rust is the sole producer and
always emits these keys. Marking them optional therefore weakens the public
output contract below the canonical Rust and TypeScript contracts and fails
the task's same-type projection requirement. The defect is reachable by every
Python timeline caller even though successful runtime values currently contain
the keys.

The pre-merge generated dataclasses do not make the mismatch acceptable. They
allowed default-valued fields at construction, but timeline callers received
objects with those fields present and directly accessible. The current
repository deliberately replaced that unshipped representation with plain
dictionaries, so neither compatibility nor restoration of the generator is
needed; the authored `TypedDict` must accurately describe the dictionaries the
native boundary returns.

### Status-union claim: material and part of the same contract defect

Canonical `SpanStatus` is a serde-tagged enum: `unset` and `ok` contain only
`code`, while only `error` may contain `description`. The current Python
declaration admits impossible values such as
`{"code": "ok", "description": "failed"}` and cannot discriminate the
error variant. This is not merely a looser spelling of the canonical type; it
removes the tagged-union invariant retained by Rust, the prior generated
projection, and current TypeScript. It belongs in the same finding because it
has the same producer, public consumer, correction boundary, and proof.

The current-candidate Python and TypeScript action-timeline journeys both pass
(`1/1` each). That closes the post-merge runtime-shape uncertainty, but those
journeys do not type-check required-key sets or reject impossible status
variants, so they do not refute this static public-contract finding.

## Proposed finding

### FOLLOWUP-R1-001 — REGRESSION: the authored Python timeline type is broader than the canonical output

- **Violated obligation:** specification REQ-008, task Scenario 12's same
  canonical `SpanRecord` projection, and the Python authority's requirement
  for precise public declarations.
- **Location:**
  `sdks/wyrd-sdk-python/python/wyrd/bifrost/_client.py:1319-1403`, produced by
  `sdks/wyrd-sdk-python/src/bifrost/mod.rs:789-794`; canonical authority at
  `crates/wyrd-spec/src/vala/trace/span.rs:22-70,193-206` and the nested trace
  record modules cited above.
- **Evidence:** serde always emits defaulted non-optional fields, while the
  Python `total=False` bases mark them omissible; the single Python
  `SpanStatus` also admits descriptions for `ok` and `unset`. The generated
  TypeScript projection preserves both requiredness and the tagged union.
- **Observable consequence:** Python's public type accepts records the API
  cannot produce and does not promise keys that every successful API result
  guarantees, so its `SpanRecord` is not the same canonical SDK contract.
- **Smallest correction:** keep the current dictionaries, native JSON
  conversion, method signatures, exports, and current public names. In the
  existing inline definitions, move every always-serialized key into the
  required `TypedDict` bodies, retain only serde-omitted keys as optional, and
  define `SpanStatus` as a union of three small tagged `TypedDict` variants,
  with optional `description` only on `error`. Do not restore the generator,
  dataclasses, conversion helpers, aliases, or compatibility behavior.
- **Focused proof:** extend the existing
  `sdks/wyrd-sdk-python/tests/typecheck/bifrost_contracts.py` fixture to require
  the canonical record keys and to reject an omitted always-emitted key and an
  `ok`/`unset` status carrying `description`; run `mise run py:typecheck` and
  `mise run py:lints`. The already-green current-candidate Python action-
  timeline journey is sufficient runtime proof, so no new runtime harness or
  journey is needed for this correction.

## Result

**RESOLVED** — the required-key and tagged-status discrepancies are one
reachable, bounded TASK-001 Python public-contract regression. The minimum
repair is declaration-only plus a focused static contract fixture; it requires
no product, wire, runtime, generator, or compatibility decision.
