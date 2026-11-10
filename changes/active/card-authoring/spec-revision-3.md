---
id: SPEC-card-authoring
revision: 3
status: approved
---

# Author Cards in code through the existing registration interfaces

## Problem, change, and value

Developers can declare complete Wyrd graphs from files, but the public SDKs
cannot construct the equivalent Service, components, Verifiers, and local
Artifact sources entirely in memory. Python supports selected native Card
holders; it does not expose the complete authoring surface. Add a thin shared
Rust authoring interface and first-class Python/PyO3 and TypeScript/N-API
projections. Keep all existing registration, upload, versioning, validation,
pull, hydration, and execution owners.

A developer constructs `Verifier(...)`, adds typed steps and requirements,
attaches it to a component or Service, then passes that Service to `Cards`
declaration or `WyrdState` declaration. Local Artifact sources are prepared by
the existing upload owner. Python input schemas may come from Pydantic; the
equivalent Rust and TypeScript definitions produce the same canonical Wyrd
submissions. No separate builder/build phase or new registration lifecycle is
introduced.

This change is solely authoring and ergonomics: typed construction/editing,
reference/schema conversion, serialization to existing Wyrd contracts, and thin
argument adapters for existing Cards/WyrdState operations. There is no new
registration implementation, upload path, transport, protocol or lifecycle.
Every registration/upload operation delegates to existing machinery. The
Artifact sequencing below composes existing calls; it does not authorize
reimplementing any of their behavior.

The primary acceptance story is a finance-release Service with two distinct
Agent components. Each component has its own multi-step Verifier. The report
reviewer checks a synthetic report against a reusable company-policy PDF;
release controls check recipient authorization and destination controls. Each
component executes only its own bindings and returns exact subject/Verifier
attribution. This is a proposed, unrun journey.

### Rationale and costs

The value is ordinary language-native composition with one implementation of
authoring conversion, rather than maintaining three graph compilers. The
cost is public type/binding work, Python stubs, generated N-API declarations,
and real-server journeys in all three languages. Constructors and local edits
are synchronous and network-free; declaration performs the existing IO.
Pydantic is an optional schema adapter, not the implementation of Wyrd's
Service or Verifier. TypeScript uses actual runtime schemas, not erased type
annotations. No new schema-library dependency is required.

## Relationship and approval

This change owns the Python authoring exploration moved out of
`SPEC-positioning`. That packet still owns the README/docsite revamp and its
sensitive-release quickstart; it need not wait for this API to use existing
file authoring. This change owns no positioning, competitor analysis, docsite
redesign, new server endpoint, or registration-engine replacement.

The maintainer explicitly requested a new specification followed by three
tasks and approved revision 3. The tasks remain proposed with their recorded
dependencies; specification approval does not record implementation or review
completion.

## Requirements

### REQ-001 — Shared typed authoring

Expose authoring through `wyrd-client` and first-class SDK reexports. Reuse the
existing `wyrd-spec` contracts as backing data. Provide ergonomic Service,
Verifier, Step, Requirement, Agent/Prompt, and Artifact construction plus
canonical reference/check/action values. Other registrable kinds must be
expressible through the existing typed Card/spec representation without raw
wire-envelope assembly or a second spec hierarchy. External is not registrable.
Python uses `wyrd.authoring`, TypeScript uses `wyrd-ai/authoring`, and Rust uses
the public SDK's `authoring` reexports.

Task 1 owns the complete shared primitive surface, not just the subset used by
the finance journey. That includes all existing action and check variants,
input/evidence slot variants, references, composition/binding values, decisions,
conditions, code sources/runtime, Agent/Prompt configuration, Artifact selections
and their authoring helpers. Reuse canonical structs/enums or provide thin
inherent helper facades; do not duplicate existing contract types merely to
give each variant a new struct name. In particular AgentCheck and AssertCheck
and their shared conversion/validation are Rust-owned, along with CodeCheck,
TraceCheck, DriftCheck, AgentAction, CodeAction and ExtractAction equivalents.

Tasks 2 and 3 depend on this complete Rust surface. They expose those types,
constructors, helpers and behavior through PyO3/N-API and language-native schema
adapters. They must not supply missing core primitives, enums, serialization,
reference semantics or checks as Python-only or TypeScript-only implementations.
The native coverage and public reexports must be complete before Task 1 can
hand off to the wrapper tasks; the example journey alone is insufficient.

### REQ-002 — Direct constructors and incremental edits

`Verifier(...)` and `Service(...)` create editable definitions. Support
`add_input`, `add_step`, `add_requirement`, `add_component`, and `add_verifier`
on their applicable owners, with idiomatic language spelling. No public
`builder()`/`build()` phase is required. Python/TypeScript edits mutate the
receiver and return it; Rust uses ordinary fallible inherent methods. Duplicate
IDs/aliases are errors, not replacement. Added children are captured by value;
later edits to the original child do not silently modify an already composed
parent. Declaration takes a validated snapshot; later edits cannot retarget a
previous registration. Full graph checks may wait until declaration so forward
references can be authored.

### REQ-003 — Schemas without mandatory slot wrappers

Python accepts Pydantic models, canonical JSON Schemas and the scalar classes
str/int/float/bool directly in the input mapping. TypeScript accepts canonical
JSON Schema values; Rust accepts
existing schema/slot representations. These lower to the same native input
slots. Public `JsonInput` wrappers are not required. An explicit `required`
option on `add_input` handles omission; nullability is independent. Required
defaults true. Preserve supplied-invalid refusal, missing-required inconclusive
dependencies, and missing-optional null semantics. Pydantic remains optional;
plain supported Python types and canonical schemas work without it. Neither
schema extraction nor model construction contacts a server.

### REQ-004 — References and concurrent steps

Expose lightweight input references and `step.output(path)` references, plus
common check conveniences such as equality, lowering to existing reference
and check contracts. Support explicit `depends_on` without adding Python/Node
scheduling. Independent steps are eligible for bounded concurrent execution;
current server stages/barriers and requirement ordering remain unchanged.
Cycles, unknown references and invalid bindings retain shared validation/error
semantics. Service components carry their own `verified_by` bindings; a
Verifier remains a peer Card, never a component.

### REQ-005 — Local Artifacts use existing upload preparation

An Artifact definition may name a local file/directory and request extraction.
`artifact.file(path)` selects a fixed versioned file. An existing Artifact
CardRef is used directly without preparation, extraction or upload.

For each local Artifact source, the common SDK uses existing
`ArtifactUpload::prepare` and declaration/upload machinery to publish that
Artifact first, retaining extraction scratch until transfer completes. Every
request with a non-empty artifacts manifest contains that Artifact as its sole
submission. After publication, the containing graph references its exact
registered version, including the resolved UID where provided by the existing
upload result. Do not include the local Artifact's heavy submission in the
containing graph request. Multiple local Artifacts are published through their
own sole-submission requests, not one mixed heavy composite.

Preserve existing source/extraction validation, manifests, versioning,
completion, activation, retry and error behavior. Source maps stay scoped to
their existing Artifact publication requests; do not merge them into a global
map or silently overwrite colliding paths within a request. Source paths and
scratch remain client-side. No duplicate upload engine or registration
machinery. If publication succeeds but containing-graph declaration fails,
retain the published Artifact; do not delete it or promise rollback.
New policy bytes require new Artifact/Verifier/subject versions; old references
and results retain attribution.

### REQ-006 — Existing registration entry points

Accept authored definitions at `Cards` declaration and `WyrdState` declaration,
while preserving existing path and selected-holder inputs. Use normal Rust
signatures where overloading is unavailable. Cards returns its existing
declaration result; state declaration uses existing declare → pull → hydrate
and returns ready state. Both entry points perform local Artifact publication
followed by containing-graph declaration within the SDK call; callers do not
manually register dependencies. The thin Rust argument adapter composes the
existing calls and passes authored values to existing publication/registration
owners; it supplies no alternative registration or upload implementation.
Cards returns the containing graph's
existing receipt, not a new aggregate multi-request receipt.

Preserve flat `CardSubmission[]` within each request and the server's heavy
Artifact sole-submission rule. References to just-published Artifacts become
external exact registered references; sibling intent remains for definitions
in the containing graph's request. Existing external CardRefs never become
uploads or implicit siblings. Preserve server-derived roots, exact versions,
idempotence, and normal failure/cancellation behavior. Do not add a wrapper wire contract or
temporary YAML/JSON-file bridge. WyrdState remains a client projection.

### REQ-007 — One reusable multi-component journey

Define one scenario called `card_authoring` for all three SDKs: Service
`finance-release`, distinct Agent components `report-reviewer` and
`release-controls`, and two component-scoped Verifiers. `report-policy` has a
report scan and policy-context preparation step, with deterministic identifier
and usable-report requirements plus a native policy Agent judgment. The policy
PDF is uploaded once and selected by exact Artifact version. `release-access`
has independent recipient and destination preparation steps and requirements.

Each SDK constructs these Cards using its own authoring surface and performs
real SDK → WyrdTestServer/repository Postgres → SDK verification. Reuse the
same checked-in policy, source files, meaningful identities, baseline graph
and expected cases; do not replace Python/TypeScript construction with loading
Rust-generated output. Their runtime inputs may be language-native typed values.

Prove permitted, prohibited-identifier, unauthorized-recipient and inadequate-
destination cases, uncertainty/error handling, exact subject attribution,
rejection of another component's unbound Verifier, and unchanged rerun
idempotence. Do not imply occurrence-level isolation when two aliases point
to the same CardRef: this story deliberately uses distinct components.

The synthetic policy permits external-audit reports with no prohibited bank
identifier. Recipient authorization and destination controls are judged by
release-access rather than duplicated in this sample policy. Shared cases are:

| Case | Required observation |
|---|---|
| Permitted report, authorized auditor, suitable destination | Both component verdicts passed; simulate release. |
| Add BANK-ACCOUNT-DEMO-001 to report text | Report policy failed, identifier requirement failed, access passed; hold. The policy Agent may also report that policy violation. |
| Recipient authorized=false | Report policy passed, recipient requirement/access failed, destination passed; hold. |
| Destination public_sharing=true | Report policy passed, destination requirement/access failed, recipient passed; hold. |
| Omit required report input from report-policy invocation | Report-dependent requirements and overall report verdict inconclusive; no fabricated policy failure; hold. |
| Valid request, scripted provider returns a native inconclusive verdict with a reason and no unsupported pass | Native sharing-policy-satisfied requirement and report verdict inconclusive; deterministic report requirements passed; hold. |
| Supply raw report JSON with text=123 | Refuse with WYRD_VERIFICATION_400_INPUT_INVALID, not a verdict; hold. Use raw JSON to reach the server rather than failing local Pydantic construction. |
| Call the other component's unbound Verifier | WYRD_SDK_404_UNKNOWN_VERIFIER, not a verdict; hold. |

The optional notes slot is omitted in the permitted case and resolves to null
under existing optional-slot semantics. Explicitly supplied null still has to
satisfy its schema. Required/nullable variants are tested separately at the
native/input-adapter boundary.

### REQ-008 — Canonical parity and local lifecycle

Equivalent constructor-style, incremental and file-authored definitions must
produce equivalent native specs, staged request sequences and registered
identities under the same tenant/name/version/content. Compare each standalone
Artifact publication and the subsequent containing graph, including sibling/
external intent before server resolution and resolved content afterward.
Local sources are not wire
fields. Reusing an equal definition with the same durable identity emits one
submission; conflicting definitions for that identity refuse rather than
choosing one. Prove network-free construction, child snapshot semantics, final graph
validation, and no source fallback. Reject conflicting authoring with existing
structured errors; do not create language-only error codes.

### REQ-009 — Complete Python projection

New PyO3 code lives in `wyrd-sdk-python`, wrapping shared client authoring and
existing methods. Register native types, public exports and `__all__`; update
authored public/native `.pyi` files and typing examples. Preserve GIL/runtime
rules and Pydantic evidence serialization. Raw JSON schema inputs work without
Pydantic installed. Test Python-owned object lifetimes, in-place method returns
and exceptions in Python, not emulated Rust tests.

### REQ-010 — Complete TypeScript projection

Expose typed construction and edits through the public package with thin N-API
conversion to shared Rust. Update native exports and wrappers; generate N-API
declarations through existing tooling and keep handwritten declarations where
their owner requires them. Preserve field names, error codes, `unknown` trust
boundaries and Node lifetime rules. No mandatory Zod or other schema framework.
Test actual Node objects and lifetimes in TypeScript.

## Invariants and non-goals

- **INV-001:** `wyrd-spec` stays IO/async/PyO3-free; shared client owns behavior;
  Python and TypeScript are adapters, not registration or execution engines.
- **INV-002:** Existing auth, tenant isolation, Card kinds, permission/audit,
  public wire fields, durable identity, immutable versions, and saga semantics
  are unchanged. No migration, new server endpoint, dependency or Cargo feature.
- **INV-003:** No arbitrary local closures run remotely. Code actions capture
  source through existing contracts. No client-side scheduler, graph store,
  parser, cache, transport or lifecycle duplicates an existing owner.
- **INV-004:** Preserve native passed/failed/inconclusive and separate errors;
  only passed permits the sample application's simulated release. Original
  evidence and exact policy/subject/Verifier versions remain inspectable;
  synchronous return does not promise immediately query-visible publication.
- **INV-005:** Synthetic snapshots do not establish external recipient or
  destination truth. Local provider fixtures prove wiring, not model quality.
  Samples are proposed APIs until implemented; no compatibility shims or fake
  SDK implementation is committed as proof.

No UI, new inference framework, blanket typed-result redesign, source-code
decorator/capture framework, schema-library dependency, or global docs rewrite
is included. Keep precise existing result projections for these journeys.

## Acceptance and task ownership

| Acceptance | Observation | Owner |
|---|---|---|
| AC-001 | Equivalent local definitions and file inputs retain canonical fields, sibling/external intent, versions and registered identities through both registration entry points. No temporary file bridge or extra wire contract. | Task 1; REQ-001, REQ-006, REQ-008; INV-001–002 |
| AC-002 | Direct construction/edits have no network IO; duplicate IDs/aliases and invalid references fail with shared errors. Child/registration snapshots stay unchanged after later source edits. Nullable and optional slots remain distinct. Native stage planning honors explicit-only dependencies; an authored dependent step executes using prerequisite outputs. | Tasks 1–3; REQ-002–004, REQ-008; INV-001–003 |
| AC-003 | Through both Cards and state authored-Service entry points, a local path publishes its Artifact first as a sole submission, then the graph uses its exact registered reference. Existing CardRefs cause no upload. Reuse/version/extraction validation and scratch lifetime remain intact; graph failure after successful publication leaves the Artifact available. No local path becomes durable data and no rollback is promised. | Task 1, projected in Tasks 2–3; REQ-005–006; INV-002, INV-004 |
| AC-004 | Rust constructs the two-component graph with two multi-step bindings, verifies each via its own run subject, exercises pass/fail/uncertainty, and rejects cross-component unbound lookup with WYRD_SDK_404_UNKNOWN_VERIFIER. Independent work overlaps through existing scheduling. | Task 1; REQ-004, REQ-007; INV-003–005 |
| AC-005 | Python independently authors and registers the same logical graph, passes the shared cases, accepts Pydantic evidence, works with plain schemas without a Pydantic import, and has accurate registered exports and authored stubs. | Task 2; REQ-003, REQ-007–009; INV-001–005 |
| AC-006 | TypeScript independently authors/registers the same graph, passes the shared cases, uses the native implementation and precise runtime schemas, and has generated declarations/handwritten wrappers aligned. | Task 3; REQ-003, REQ-007–008, REQ-010; INV-001–005 |
| AC-007 | Every core primitive family in Task 1's inventory is constructible through the common Rust public surface and lowers to its canonical contract, including variants absent from the finance example. Python/TypeScript expose that full surface with matching native behavior and complete declarations; no missing primitive is implemented only in a wrapper. | Task 1 owns definitions/helpers/parity; Tasks 2–3 own projections; REQ-001, REQ-009–010; INV-001, INV-003 |

Use real-server journeys with repository-managed Postgres and local provider
fixtures. Unit/serialization tests supplement those journeys. Each task lists
exact selectors and setup; broader affected SDK lanes and boundary/codegen
checks run once on the integrated candidate. Examples under `examples/` in
this packet are reviewable syntax proposals, not completed implementation.

## Decisions and approval

No material decision is intentionally deferred. The public API sketches in
[examples](examples/README.md) illustrate the required ergonomics. Idiomatic
signatures and private representation may change while preserving these
semantics. New work is limited to authoring and argument adaptation over
existing loader/client owners; registration/upload machinery is reused as-is.
If those owners cannot support the required workflow without changing a wire,
security or registration contract, stop for a spec revision.
The prior architectural blocker around local Artifact composition is withdrawn:
the publication-first SDK sequencing above preserves the existing server
contract and requires no server-contract change.

The maintainer approved revision 3 covering the thin cross-language interface, value
capture/edit semantics, existing entry-point integration, optional schemas,
Artifact reuse, the common two-component journey, and evidence together.

## Revision history

- Revision 1 — Draft common Card authoring and three SDK implementation tasks;
  move/revise Python examples and add TypeScript/Rust sketches.
- Revision 2 — Make Task 1 explicitly responsible for every core primitive,
  helper, native conversion and public reexport; require complete native coverage
  before Python/TypeScript projection handoff and add full-surface acceptance.
- Revision 3 — Clarify existing-CardRef reuse and SDK-local Artifact publication
  before containing-graph declaration, preserving sole-submission requests,
  extraction lifetime, retry/error behavior and retained publication on graph
  failure. Withdraw the prior Artifact architectural blocker. Explicitly
  approved by the maintainer.
- Approval clarification — Revision 3 remains approved. The maintainer
  reiterated that this is solely ergonomics/authoring and must reuse existing
  registration/upload machinery; no additional execution path is authorized.

## Authority links

- [Repository rules](../../../AGENTS.md), [Rust](../../../agent_docs/rust.md), [errors](../../../agent_docs/errors.md)
- [Wyrd contracts and registry lifecycle](../../../architecture/wyrd-design.md), [security](../../../architecture/wyrd-security-posture.md)
- [Python rules](../../../sdks/wyrd-sdk-python/AGENTS.md), [TypeScript rules](../../../sdks/wyrd-sdk-ts/AGENTS.md)
- [Workflow](../../spec-driven-development.md), [execution](../../implementation-execution.md), [testing](../../../TESTING.md)
- [Positioning change](../positioning/spec.md)
