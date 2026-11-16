# One Card grammar for YAML and code

- Change: `card-authoring`
- Specification: `SPEC-card-authoring`, approved revision 6; draft revision 7 superseded
- Completed: 2026-11-11
- Reviewed target: `78ca329b4` (merge of pull request 234), approved by the human owner in place of a final change review
- Delivery reference: pull request 234

Cards authored in Rust, Python, or TypeScript use the same canonical field
names, values, validation, and declaration path as YAML, so a definition built
in code registers exactly what the equivalent file registers.

## Shipped behavior

- One canonical vocabulary in all three SDKs: constructors take fixed
  arguments, canonical fields are set or appended incrementally, and Service
  Components are first-class definitions. Fixed constructor arguments have no
  duplicate setters; Action and Check constructors return ordinary types.
- Domain validation is deferred to `validate` and declaration, which report
  canonical field diagnostics; incomplete values and forward references can
  be edited. Values are captured by value; replacement clears obsolete
  captures.
- Local Artifacts publish first through the existing preparation and upload
  rules; registered references upload nothing; a later graph failure keeps
  completed publication.
- Card paths and authored Cards declare through one `declare` in each SDK.
- Slot-level `required` and omission-to-null are removed: missing evidence
  makes dependent checks inconclusive while independent checks run.
  `Verifier.add_input` declares slots; `Action`/`Check.bind_input` binds them.
- TypeScript uses generated snake_case option types with camelCase methods;
  Python projects the same grammar with authored stubs.

## Invariants and constraints

- Canonical Rust contracts own the grammar; SDKs add no validator, graph, or
  lifecycle. Card kinds, binding locations, immutable versions, and wire
  fields (except removed slot-level `required`) are unchanged.
- Passed, failed, inconclusive, and structured errors stay distinct; no
  missing trace establishes absence. No compatibility shims.

## Decisions and deviations

- Revisions 3 to 6 progressively fixed publication-first Artifacts, canonical
  field naming, single-place constructor arguments, and the input-binding
  split; Model bindings stay on Components.
- Draft revision 7 (reuse the original Prompt, `verify` imports, optional
  constructor versions) was not approved here; that work was respecified and
  shipped as `sdk-authoring-ergonomics`. Its TASK-004-R1 was not implemented
  under this packet.
- The final change review was waived by the maintainer.

## Acceptance closure

AC-001 through AC-009 closed through task reviews and remediations for the
shared Rust, Python, and TypeScript projections, the finance two-component
journeys in each SDK, cross-SDK parity tests, and engine plus real-server
journeys for inconclusive missing evidence.

## Current owners

- [Wyrd design](../../../architecture/wyrd-design.md).
- [Shared authoring](../../../crates/shared/wyrd-client/src/authoring/mod.rs)
  and [declaration state](../../../crates/shared/wyrd-client/src/state.rs).
- Successor record: [sdk-authoring-ergonomics](sdk-authoring-ergonomics.md).
- Journeys: [Rust authoring](../../../sdks/wyrd-sdk-rust/tests/integration/authoring.rs),
  [Python authoring](../../../sdks/wyrd-sdk-python/tests/integration/cards/test_authoring_journey.py),
  [TypeScript authoring](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/authoring.test.ts).
