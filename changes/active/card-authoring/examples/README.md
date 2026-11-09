# Approved one-grammar authoring examples

These sketches express [approved specification revision 5](../spec.md).
They are reviewable interface examples, not evidence that the SDK exports are
implemented. No fake SDK or registration shim is included.

| Example | What it demonstrates |
|---|---|
| [Python multi-component](python/multi_component.py) | Two Components with their own verified_by bindings; constructors and incremental edits. |
| [Python constructors](python/constructor_style.py) | Canonical constructor fields and a Service-level Binding. |
| [Python incremental edits](python/incremental_style.py) | add_input, add_step, add_component and add_verified_by over those same fields. |
| [Python registration](python/registration_flow.py) | Unified declare and existing verification/results. |
| [Python policy versions](python/policy_versions.py) | Exact registered reference reuse and explicit immutable version updates. |
| [Python dependencies](python/dependencies.py) | YAML reference strings and depends_on IDs; existing server scheduling. |
| [TypeScript](typescript/multi_component.ts) | Generated snake_case options, camelCase editing methods and canonical inline objects. |
| [Rust](rust/multi_component.rs) | Infallible owned setters, mutable additions, explicit Components and string references. |

The primary story remains finance-release, report-reviewer/report-policy and
release-controls/release-access, with two steps per component's Verifier.
The constructor/incremental examples retain a separate Service-level binding
illustration; the dependency sketch is an optional extension, not a change to
the primary graph.

Python demonstrates optional Pydantic schema conveniences; Rust and TypeScript
use JSON schemas. All languages use the checked-in policy under
fixtures/cards/card_authoring and the same packet Python check source. Named
Prompt Cards and inline Prompt bodies are both canonical supported forms;
the sketches illustrate both and do not claim byte-identical pre-resolution
graphs for those different authoring choices. Implementation parity tests use
the same reference/inline choice on both sides.

## Semantics

- Fixed constructor arguments have no later setters or additions. Other field
  methods and collection additions remain available. Action/Check constructors
  return their ordinary types without extra receiver conversions.

- A field method replaces the field. An add method appends/inserts, retaining
  attempted duplicates for validate/declaration to refuse.
- Children are captured by value. Replacements remove obsolete dependencies
  while retaining shared live ones.
- References are strings or canonical CardRefs; Card values occupy permitted
  reference positions. No Reference, AgentCall, CodeCall or Linked editing layer.
- CodeSource.file and Artifact.from_path record sources; setters do no file IO.
- validate and declaration share native rules. Declaration validates its own
  snapshot before publishing anything.
- Each local Artifact publishes first through the existing owner, as a sole
  submission. Registered CardRefs upload nothing. A later graph failure leaves
  completed publication registered; no rollback is promised.
- Model verification belongs on its Service Component, not on Model.
- Running is unchanged: run, component view, observe, verify. Python/TypeScript
  keep their existing observe property; Rust keeps observe().

## Checks and implementation boundary

The Python checker parses the sketches without importing future authoring
exports and executes only real local evidence/check logic. Rust formatting
checks syntax; TypeScript syntax transpilation does not establish types or
native behavior. SDK compilation, type checking and real-server journeys
belong to the implementation tasks.

Revision-3 task/review evidence remains preserved. See the
[current task index](../tasks/README.md) for the revision-5 handoff.
