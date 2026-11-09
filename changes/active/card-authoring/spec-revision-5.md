---
id: SPEC-card-authoring
revision: 5
status: approved
---

# One Card grammar for YAML and code

## Proposal and approval boundary

A developer should learn one Service and Verifier grammar, then express it in
YAML, Rust, Python or TypeScript. Today the code API introduces another
vocabulary: `add_verifier` instead of `verified_by`, reference handles instead
of reference strings, and `declare_authored` instead of `declare`. Even when
the final wire values match, those names and intermediate concepts make
developers learn and translate a second definition language.

Revision 5 retains the revision-4 grammar and removes redundant editing of
constructor arguments. A field supplied as a fixed constructor argument has
no later setter or incremental edit. Constructors return the ordinary Action
or Check; no extra receiver types or conversion calls are needed.

Revision 4 replaced that vocabulary with constructors, field setters and
incremental methods over the existing YAML contract. A Service contains
Components; each Component has `alias`, `ref` and its own `verified_by`.
`Service.verified_by` remains a binding on the Service itself. An in-memory
Card may occupy a reference position, just as a YAML path points at another
Card file. Both forms go through the existing loader, validation,
declaration, upload and hydration owners.

The maintainer explicitly approved this revision and requested remediation work:
“then do it and approve the spec and add it as remediation work.” The approved
change keeps constructors and removes setters duplicating their fixed arguments,
including name/version setters. It removes EachCheck, AgentCheck and AgentAction,
with no replacement API. Remaining field setters and collection additions stay.
The exact earlier approvals are preserved in
[spec-revision-3.md](spec-revision-3.md) and
[spec-revision-4.md](spec-revision-4.md). Earlier tasks and review findings remain
historical evidence; the revision-5 remediation supersedes their contrary
constructor-setter requirements, including FIND-TASK-001-9.

**Confirmed scope decision:** `verified_by` remains on a Service, Service
component and standalone Agent. A Model receives its Service-specific
verification through its Component. No `Model.verified_by`, new wire field
or server behavior is proposed.

### Why this approach and what it costs

Keep the working graph capture/deduplication, canonical conversion, shared
validation, Artifact publication and declaration/hydration workflows. Replace
their public authoring vocabulary and argument adaptation. Reuse TypeScript's
generated [card-types.ts](../../../sdks/wyrd-sdk-ts/wyrd/src/card-types.ts)
rather than writing another set of spec interfaces.

This changes the newly introduced authoring API and its examples, requires
deferred validation of editable values, and expands schema-aligned coverage
across all three SDKs. Constructors must be able to hold incomplete definitions
until validation. That is a local authoring state, not a second durable schema
or validator. Language adapters and generated declarations still need accurate
typing. No compatibility aliases for the replaced authoring vocabulary are
proposed. Existing file declarations and already registered content keep
their behavior.

Builder methods are editing operations on the same grammar, not extra YAML
keys. `verified_by(...)` sets the field; `add_verified_by(...)` appends one
entry. The same distinction applies to every listed collection.

## Requirements

### REQ-001 — One canonical vocabulary and value grammar

The existing Card envelope and authored YAML schemas own names, nesting,
allowed fields, defaults and value forms. Public code construction exposes
those fields without translating them into a different domain vocabulary.

- Rust and Python field methods use the YAML's snake_case names.
- TypeScript method names use camelCase; options and serialized keys keep the
  YAML's snake_case names. Thus `verifiedBy(...)` edits `verified_by`, and
  `addRequirement(...)` edits `requirements`.
- Reserved words use only the language's necessary escape:
  Rust `r#ref`, Python `assert_`; serialized names stay `ref` and `assert`.
- Execution references are strings such as `inputs.report.text`,
  `steps.scan.output.text` and `verifiers.report-policy.inputs.report`.
  No public Reference handle, `at`, `output` or `status` helper is needed
  to author them.
- Parameterless operators use their YAML strings. Parameterized operators use
  their existing YAML object forms, including their canonical `kind` and
  parameters; they are not reduced to strings or a new operator syntax.
- Inline bodies stay canonical bodies. A named Card stays a Card reference
  when composed; it does not silently become an anonymous inline body.

All twelve repository-authorized Card kinds are covered: Data, Model,
Artifact, Experiment, Prompt, Agent, Mcp, Service, Audit, Verifier, Trigger and
Operator. External is not registrable. The full native Action, Check, input,
decision, condition, reference and nested configuration inventories remain
covered, including variants absent from the finance story.

The surface tables below fix public behavior, not private structs, modules,
conversion algorithms or storage. Canonical typed values remain usable;
callers are not required to assemble an outer wire envelope or JSON string
to access a supported field.

### REQ-002 — Constructors, replacement setters and incremental edits

Each named Card constructor takes `(name, version)`; its kind is fixed by
the type. It may initially lack required spec fields. Python constructors
also accept spec fields as keyword arguments; TypeScript constructors accept
a spec options object. These initialize the same fields as their setters.

A sub-value constructor takes the required identifying/content fields shown
below. Canonical object forms remain accepted in Python and TypeScript.
Named Prompt Cards also use `(name, version)`; `provider` and `model`
are spec fields, not a different meaning for those two constructor arguments.
Anonymous inline Prompt bodies use the existing canonical Prompt spec form.

Fixed constructor arguments are constructor-only: there is no second public
method to replace or append to those fields. This includes Card name/version,
Component alias/ref, Step id/action and every fixed argument in the tables below.
To use different constructor arguments, construct another value. No clone-with,
replacement helper, alias or alternate receiver type is introduced.

Fields not taken as fixed constructor arguments retain their same-key setters
and listed collection additions, even when required by the canonical schema.
For example, Prompt provider/model and Verifier inputs remain editable.
Python keyword arguments and TypeScript spec options initialize those same
editable fields; they do not make the fields constructor-only. This rule applies
to all three SDKs. CardRef identity accessors remain read-only accessors.
Action constructors return Action; Check constructors return Check, including
agent and each. Mixed Check lists need no conversion for these constructors.

| Operation | Rust | Python | TypeScript |
|---|---|---|---|
| Field setter | consumes self, returns Self | mutates and returns self | mutates and returns this |
| Incremental add | mutates and returns &mut Self | mutates and returns self | mutates and returns this |
| Complete collection setter | replaces the whole list/map | same | same |
| Add to list | appends one item | same | same |
| Add to map | inserts one named entry | same | same |

Constructors, setters and add methods perform no domain validation, network IO
or file reads. Correctly typed but incomplete or malformed domain values can
be assembled, including forward references. Host-language argument/type
errors remain ordinary boundary errors; this promise does not suppress them.

Appending duplicate IDs, aliases or map keys must not silently replace an
earlier item. Validation refuses those attempted duplicates. In contrast, a
complete field setter deliberately replaces that field, including all earlier
additions to it. Duplicates already erased by a caller's Python dict or
JavaScript object before the SDK receives it cannot be recovered.

Passing a child captures its value at that moment. Later child edits do not
change the parent. Replacing a field replaces its captured dependencies too;
a removed Artifact is not published, and another live field that still uses
that Artifact retains it. Declaration captures a validated snapshot.

### REQ-003 — Canonical slots and optional schema conveniences

Every language accepts all canonical InputSlot forms. JSON, media, trace,
samples and group source slots retain their existing shapes and rules.
A bare JSON Schema may initialize a JSON slot without a mandatory wrapper.
Python additionally accepts supported scalar classes and optional Pydantic
models as schema conveniences, lowered through shared native semantics.

Omission is controlled by `required` on slot types that have that YAML key;
supplied null is controlled independently by the schema. Required defaults
true. Samples and source slots do not gain an invented `required` field.
Missing-required inconclusive, optional omission and supplied-invalid refusal
retain their current behavior. Pydantic is not required for plain schemas.

### REQ-004 — One validation boundary and unchanged execution

`validate()` checks the current complete local definition graph without
registering or uploading it. Declaration always validates its own snapshot
before publication, even if `validate()` was previously called.
This does not cache a mutable definition's validation result.

Use the existing canonical parsing, Prompt compiler, loader reference
inventory, graph validation and kind validators. If an existing domain
validator is not reached for a code-authored value, extend its shared input
boundary rather than implementing its rules again in authoring or bindings.
Malformed names, versions, operators, source text, reference expressions,
duplicates, bindings and missing required fields are reported here.

Equivalent malformed YAML and code values use the same native rule and stable
error family, with the offending YAML field path in diagnostics. File paths
and parser line/column locations may differ or be absent for in-memory values.
Do not promise identical display strings where the existing validator supplies
different context. No Rust field name, Python attribute alias or TypeScript
camelCase method appears as a replacement diagnostic field path.

References and explicit `depends_on` keep current stage planning, inferred
edges, bounded concurrency and requirement barriers. No client scheduler.
A fixed workspace Artifact selection remains distinct from request evidence.

### REQ-005 — Artifact conveniences preserve the existing publication owner

`Artifact.from_path(name, version, path)` records a local source descriptor;
`extract(bool)` chooses the existing preparation option. Neither reads bytes
until validation/preparation actually needs them. `file(path)` selects a
fixed file/directory prefix using the existing selection contracts.
An exact registered Artifact CardRef has `file(path)` too, without a local
source or implicit upload.

Local source descriptors and code-source file capture are explicit
conveniences outside the persisted grammar; they never become new YAML spec
fields. A local code file source is read and captured as canonical inline
code when validating/preparing a snapshot. Preparation/IO failures are
fallible at that boundary, never hidden in an infallible setter.

Keep existing Artifact preparation, extraction validation, scratch ownership,
versioning, completion and error/retry behavior. Publish each local Artifact
first through its existing sole-submission operation, then declare the
containing graph. Each request owns its existing source map. Registered
references skip all local publication. Successfully published Artifacts
remain registered if a later graph call fails or is cancelled; no rollback.

An authored exact name/version reference is resolved and UID-pinned by the
existing server before hashing/persistence. No second client resolution
authority or promise of transmitting a returned upload UID is introduced.
Changed immutable content still requires an explicit new version; code
setters do not silently choose one.

### REQ-006 — One declaration name and the existing lifecycle

Public `Cards.declare` accepts the existing native declaration input and
path/Card forms; `WyrdState.declare` and `declare_with` accept a path or
authored Card while preserving existing options and selected-holder inputs.
Rust uses idiomatic argument adaptation for these forms; no new wire envelope
or parallel declaration engine is authorized.

The path form uses the existing file loader. The Card form snapshots and
lowers through the existing shared validation and publication owners. Both
use the existing declaration saga; state then uses the same pull/hydrate tail.
Cards returns the containing graph's ordinary receipt; state returns ready
state, with current client/bundle/cancellation behavior.

New authoring-facing `declare_authored*` names disappear. Existing callers
of native declaration input must continue to work. Existing path helpers can
remain where required by established client contracts; all new examples and
normal authoring use `declare(path | card)`.

Running and verifying do not change: `state.run()`,
`run.for_card(alias)`, and `run.observe().verify(...)` keep their existing
meaning. Use the latter component-view form in the packet; no new
`run_for_card` alias or runtime removal is part of this proposal.

### REQ-007 — Preserve the multi-component finance journey

Retain the existing `card_authoring` story: finance-release, distinct
report-reviewer and release-controls Agent components, with report-policy and
release-access bindings respectively. Each has its existing two steps; the
dependency variation still consumes both prerequisite outputs. Do not replace
the accepted baseline with a simpler single-Verifier example.

Each SDK independently constructs the same graph with its revised public API
and runs against WyrdTestServer/repository Postgres using shared checked-in
synthetic policy/source evidence and a local provider.

| Case | Required observation |
|---|---|
| Permitted report, authorized recipient, suitable destination | Both components pass; simulated release permitted. |
| Prohibited bank identifier | report-policy fails; access passes. |
| Unauthorized recipient | Recipient/access fail; report-policy and destination pass. |
| Public destination | Destination/access fail; report-policy and recipient pass. |
| Required report omitted | Dependent report requirements/report verdict inconclusive; hold. |
| Provider explicitly uncertain | Policy Agent/report verdict inconclusive; deterministic requirements pass. |
| Supplied report text has invalid JSON type | WYRD_VERIFICATION_400_INPUT_INVALID, not a verdict. |
| Other component's unbound Verifier requested | WYRD_SDK_404_UNKNOWN_VERIFIER, not a verdict. |

Only passed permits simulated release. Exact subject, Verifier, policy
versions, original citations and old-version attribution remain inspectable.
These synthetic cases prove wiring, not external recipient truth or model
quality. No alias-occurrence isolation beyond existing distinct-subject
semantics is added.

### REQ-008 — Canonical parity across construction styles

File form, full-field construction and incremental construction yield
equivalent canonical specs, sibling/external intent, staged publications and
registered identities for equal content and exact versions. Local source
metadata never reaches the wire.

An in-memory Card used at a permitted reference slot is captured and declared
as a sibling dependency, just as a file path resolves to a sibling. An exact
registered CardRef stays external even when another local definition has the
same identity. Inline bodies remain inline where the canonical slot permits
them. Existing heavy predeclaration rules still apply; accepting a Card value
does not authorize automatic new Model/Data/Experiment byte-upload behavior.

Equal reused definitions declare once; genuinely conflicting active
definitions of one identity refuse. Replacement setters cannot retain obsolete
captures or suppress a conflict still owned by another live field.

### REQ-009 — First-class Python projection

Python supports complete constructor keyword arguments and the same fluent
setters/add operations, with snake_case names and by-value child capture.
It accepts canonical values and the REQ-003 schema conveniences. Semantic
validation, normalization and publication stay shared Rust; PyO3 owns only
the language boundary. Public exports, authored stubs, errors and actual Python
object/lifetime tests match the chosen surface. No mandatory Pydantic import.

### REQ-010 — First-class TypeScript projection

TypeScript uses the wire types already generated in
`sdks/wyrd-sdk-ts/wyrd/src/card-types.ts`. Authoring options preserve their
snake_case keys, unions, requiredness and nesting. At canonical reference
positions, the options additionally accept the corresponding Card object;
schema conveniences and explicit bare-Verifier binding shorthand are the
other documented adaptations.

Do not hand-copy ServiceSpec, VerifierSpec, Check or related grammar into new
interfaces. If an authorized kind is absent from generated SDK roots, extend
the existing generator's consumed roots, not the wire schema or a handwritten
substitute. This is an SDK projection change, not a new Card kind.

Constructors, camelCase fluent methods and plain canonical option objects
converge on shared native meaning. N-API declarations, handwritten wrappers and
actual Node lifetime/error tests stay aligned. TypeScript checks structure;
shared Rust checks semantics at validate/declaration. No new runtime schema
library or JavaScript validator.

## Proposed public support surface

Names below are Rust/Python spellings; apply the TypeScript method casing rule.
Options/object keys always keep the spellings shown. A listed spec field has
its same-named setter unless fixed by its constructor; a listed collection add
method edits only a field not fixed by that constructor.
Nested values retain their canonical fields, not an alternate mini-language.

### Cards and Service components

All named Cards: `new(name, version)`, `validate()`, and canonical authored
metadata support: constructor-only `name`/`version`, editable `labels` and
`annotations`.
Server-derived outer UID/hashes/origin, relationships and status are not
authorable. Named fields below refer to `spec`, not envelope metadata.

| Type | Spec field setters | Incremental collection methods |
|---|---|---|
| Service | description, service_type, components, verified_by, tables, entry_point, deployment, runtime, service_config, credential_refs, content_hash, lock_hash, metadata | add_component, add_verified_by, add_table, add_deployment, add_service_config, add_credential_ref, add_metadata |
| Agent | prompt, execution_route, tool_names, run_config, verified_by | add_tool_name, add_verified_by |
| Prompt Card | provider, model, system, messages, settings, response_format, format, body | add_message, add_body |
| Verifier | description, inputs, steps, requirements, decide, context_capture, order_by | add_input, add_step, add_requirement |
| Artifact | artifact_kind, artifact_uris, content_type, size_bytes, integrity, schema_ref, framework_adapter, external_uri, metadata | add_artifact_uri, add_metadata |
| Model | interface, task_type, signature, sample_input, card_refs | add_card_ref |
| Data | interface, schema, card_refs, splits, target_columns, sql, stats | add_card_ref, add_split, add_target_column |
| Experiment | experiment_type, description, target_refs, default_parameters, run_refs, summary_metrics, best_run_ref, card_refs, details | add_target_ref, add_default_parameter, add_run_ref, add_summary_metric, add_card_ref, add_detail |
| Mcp | description, server_name, transport, tool_refs, scopes, details | add_tool_ref, add_scope, add_detail |
| Audit | description, subject_refs, policy_refs, evidence_refs, details | add_subject_ref, add_policy_ref, add_evidence_ref, add_detail |
| Trigger | description; flattened activation keys kind, cron, tz or after | none |
| Operator | description, action, budget | none |

Map additions take `(key, value)`; list additions take one canonical element.
`add_message` accepts the canonical message value, including structured
content; it is not restricted to a role/text tuple. Prompt portable/native
mutual-exclusion rules are validated, not replaced by a third form.
An Artifact has no invented spec-level `description` or `extract` field.
Trigger has no invented spec-level `activation` key: schedule is
`{kind: schedule, cron, tz?}`; group end is `{kind: group_end, after}`.

| Component contract | Proposed support |
|---|---|
| Identity and target | `Component.new(alias, ref)`; alias and ref are constructor-only |
| Component verification | `verified_by(list)`, `add_verified_by(binding)` |
| Other canonical fields | source, config, credential_refs setters; add_config(key, value), add_credential_ref(value) |
| Target value | Appropriate Card or exact registered CardRef; existing YAML path form only through the file-loading context |
| Meaning | A sub-value of Service, not a Card, registration root or independent verifier owner |

Verification placement is exact: Component.verified_by judges that component;
Service.verified_by judges the Service; Agent.verified_by applies to the
standalone Agent as today. A Model component is verified through its Component.
Do not invent Model.verified_by or attach peer Verifier/Trigger/Operator Cards
as Service components.

### Verifier parts

| Value / constructor | Field setters | Incremental methods |
|---|---|---|
| Binding.new(verifier) | runs_on, on_failure, on_verdict_change | add_on_failure, add_on_verdict_change |
| Step.new(id, action) | depends_on, when, requirements, timeout_seconds, retries | add_depends_on(step_id), add_requirement |
| Requirement.new(id, statement, check) | none | none |
| InputSlot.json(schema) | required | none |
| InputSlot.media(formats) | max_bytes, required | none |
| InputSlot.trace() | required | none |
| InputSlot.samples(features) | none | none |
| InputSlot.source(text) | none | none |

A Binding's verifier accepts a Verifier Card or exact CardRef; runs_on accepts
a Trigger Card, exact reference or permitted inline spec; operator lists
likewise accept Operator Cards, references or permitted inline specs.
A bare Verifier in verified_by/add_verified_by is explicit convenience for
the canonical `{verifier: value}` binding. It does not create another
binding kind or default activation.

`when` takes the canonical `{path, operator, expected}` condition value.
`decide` is `all` or the canonical pass_rate object. `order_by` keeps its
canonical string forms. No `set_*`, `equals` or reference-handle grammar
is required to author these values.

### Actions, checks, sources and references

| Canonical variant / convenience constructor | Field setters | Add methods |
|---|---|---|
| Action.extract(from) | none | none |
| Action.agent(agent) | inputs, workspace | add_input, add_workspace |
| Action.code(source, entrypoint) | runtime, inputs | add_input |
| Check.assert(path, operator, expected) | none | none |
| Check.each(each, operator, expected) | none | none |
| Check.trace(on, span_selector, operator, expected) | none | none |
| Check.agent(agent) | inputs, workspace | add_input, add_workspace |
| Check.code(source, entrypoint) | runtime, inputs, response_format | add_input |
| Check.drift(on, spec) | none | none |

Constructors fix the discriminant (kind); field setters do not convert one
variant into another. Python spells assert as assert_. The canonical assert
object still supports exactly one of path/each and shared validation refuses
an invalid combination in raw input.

Agent check output format is owned by its Agent's Prompt; Check.agent does
not gain a non-existent response_format key. Code checks retain their actual
response_format field. The code-source conveniences are
`CodeSource.inline(text)`, `CodeSource.file(path)`, and
`artifact.file(path)`. Native code source is `{inline: text}` or
`{artifact: {card: ref, path?}}`. CodeSource.file records a descriptor and
becomes the inline form at snapshot preparation; it adds no durable file-source
variant. No arbitrary closure is uploaded.

A workspace value is a scope reference string or
`{artifact: {card: ref, path?}}`. Artifact/CardRef.file(path) carries exactly
that fixed selection meaning, adapted to workspace or code-source position;
it does not invent `kind/name/version` fields alongside `path`.

`CardRef.new(kind, name, version)` and per-kind shorthands construct the
same `{kind, name, version}` identity. CardRef.artifact(...).file(path)
requires no local source or upload. Literal canonical references remain
accepted. No public `Linked.registered`/`Linked.inline` discriminator is
needed: registered reference, inline body and Card object have distinct shapes.

### Replaced authoring vocabulary

| Current authoring name/concept | Proposed expression |
|---|---|
| add_verifier / set_* binding methods | verified_by / add_verified_by and same-key setters |
| add_component_verifier | Component.verified_by / Component.add_verified_by |
| add_registered_component | add_component(Component(..., CardRef(...))) |
| add_slot / add_input_with_required | add_input(name, canonical slot), with slot.required where supported |
| Input/step Reference handles | canonical reference strings |
| Public AgentCall/CodeCall/Linked authoring indirection | canonical Action/Check fields and their permitted Card inputs |
| declare_authored / declare_authored_with | declare / declare_with accepting path or Card |
| EachCheck / AgentCheck / AgentAction and duplicate constructor-field edits | ordinary Check / Action constructors; constructor arguments have no editing methods |

This removes authoring-only public indirection, not the canonical native types
or the existing implementation owners that consume them. Existing internal
reference/parser types remain valid implementation tools.

## Invariants, boundaries and non-goals

- **INV-001:** Canonical Rust contracts own the grammar; shared client/loader
  own normalization and publication. Python and TypeScript are projections,
  not independent validators, graph compilers or registration engines.
- **INV-002:** Card kinds, binding locations, wire fields, immutable versions,
  server-derived roots/UIDs, auth, tenancy, permissions/audit and saga semantics
  remain unchanged. No new endpoint, migration, dependency or Cargo feature.
- **INV-003:** Value capture and replacement are local. No duplicate graph
  store, parser, scheduler, cache, transport or lifecycle; no remote closures.
- **INV-004:** Passed/failed/inconclusive and structured errors remain distinct;
  only passed permits simulated release. Original evidence/version lineage is
  preserved; synchronous return adds no query-visibility guarantee.
- **INV-005:** Examples remain synthetic and prove wiring. No compatibility
  shims for the replaced authoring API; update its callers together.

No standalone Model binding, new observation/execution API, broad runtime
renaming, framework/decorator system, schema library, UI, positioning rewrite
or global documentation redesign. Local source helpers and schema conveniences
are the explicit grammar adaptations above, not permission for new keys.

## Existing owners and necessary changes

| Capability | Existing owner / complete workflow | Missing behavior and proposal |
|---|---|---|
| Names and value schemas | wyrd-spec Card/spec/reference contracts; Skald authored Prompt contract; existing schema generator | Project their authored grammar; replace divergent authoring names, not those owners. |
| Local Card capture/deduplication | wyrd-client authoring values -> current dependency collection -> canonical AuthoredCards | Retain proven capture semantics, extend replacement/add behavior to the field-oriented surface without a second graph owner. |
| File/code normalization | wyrd-loader load/resolve -> shared validation/order -> declaration input | Adapt code fields to that boundary; defer malformed/incomplete local values and share diagnostics, rather than validate in each SDK. |
| Local bytes and registration | Cards.upload_artifact -> existing preparation/saga; Cards.declare -> existing saga | Unified public declare input dispatch composes these same workflows; no replacement orchestrator. |
| State hydration | WyrdState declare -> Cards -> shared pull_declared -> load state | Extend existing argument adaptation, retain this tail and options. |
| TypeScript types | scripts/gen_ts_types.py -> card-types.ts, existing N-API tooling | Consume generated nested types, widen only documented authoring positions and complete authorized generated roots where needed. |
| Python | SDK PyO3 boundary and optional schema adapters | Native-backed grammar projection, authored stubs and Python lifetime proof; no new validation owner. |

The architecture's In-memory authoring subsection is the narrow owner to
update during remediation to state the constructor-only rule. Preserve its
existing canonical admission, capture, Artifact roots and publication lifecycle.
Approval authorizes that bounded public-surface update; it does not claim the
revision-5 code is already implemented.

## Acceptance

| ID | Situation and required observation |
|---|---|
| AC-001 | Equivalent YAML, constructor and incremental definitions in all SDKs yield equivalent canonical fields, sibling/external intent, staged publications and registered content/identities. No temporary-file bridge or extra wire envelope. |
| AC-002 | Incomplete constructors and forward references can be edited without domain failure; validate/declaration report canonical field diagnostics. Duplicate additions refuse before publication, replacement clears obsolete captures, shared captures survive and child/registration snapshots stay fixed. |
| AC-003 | Through both declaration owners, local Artifacts publish first using existing preparation/scratch/source-map/version/error rules, registered references upload nothing, and later graph failure retains completed publication. |
| AC-004 | Rust constructs the preserved two-component story and its full case matrix; exact subjects/unbound refusals, native concurrency and prerequisite outputs remain correct. |
| AC-005 | Python independently authors that story using constructor kwargs and builder methods; schema-without-Pydantic and Pydantic paths, real lifetime/return/error behavior and authored exports/stubs agree. |
| AC-006 | TypeScript independently authors that story with generated snake_case option types and camelCase methods; actual Node objects, declarations and errors agree with native semantics. |
| AC-007 | All authorized kinds, every canonical Action/Check/operator/slot/decision/reference form and supported nested field have public construction/parity coverage; cases absent from the finance story cannot be deferred to wrappers. |
| AC-008 | Fixed constructor arguments have no duplicate editing methods; Action/Check constructors return their ordinary types without the three receiver wrappers or associated mixed-list conversions. Existing native declaration-input and file callers still work; new file/Card calls use declare, and no replaced authoring vocabulary remains in current public authoring exports, packet examples or their tests. |

Unit/canonical proof supplements real SDK -> WyrdTestServer/Postgres -> SDK
journeys. Invalid shared examples compare diagnostic rules/paths as well as
successful output, including strings that were formerly parsed at setters.
No duplicate-key loss, early hidden validation or fresh deferred-source
capture may bypass these checks. Plan after approval chooses precise tests
and commands; this specification does not prescribe a new test harness.

## Approval and history

The maintainer approved revision 5, including the constructor-only rule. The
retained package is: the canonical field/collection surface above,
deferred domain validation, by-value capture/replacement semantics, generated
TypeScript contracts, unified declaration name and removal of the replaced
authoring vocabulary. Model bindings stay on Components, as confirmed by the
maintainer. No material decision remains open. Existing field and collection edits outside
fixed constructor arguments remain supported.

The important cost is a coordinated public authoring API replacement plus
cross-language parity proof; this is not a rename-only patch. Approval records
the required behavior; task planning and implementation remain separate work.

- Revisions 1–2: established shared authoring and complete native inventory.
- Revision 3: approved publication-first Artifact composition and preservation
  of existing registration/upload machinery; preserved unchanged in
  spec-revision-3.md with its prior approval record.
- Revision 4 (approved): replace a separate authoring vocabulary with canonical
  field names/values, fluent and incremental editing in all SDKs, first-class
  Component definitions, shared validation timing and unified declaration.
  Preserve Component-based Model verification; no standalone Model field.

- Revision 5 (approved): constructor arguments have no duplicate setters or
  incremental edits. Remove EachCheck, AgentCheck and AgentAction; constructors
  return ordinary Check/Action. Keep remaining builders and all other R4 fixes.
  Approved directly by the maintainer's instruction to do this, approve the
  spec and add remediation work. No replacement API or metadata exception.

## Authority

[AGENTS](../../../AGENTS.md);
[Wyrd doctrine, client, Verifier and registry/file-authoring contracts](../../../architecture/wyrd-design.md);
[security](../../../architecture/wyrd-security-posture.md);
[workflow and comprehension pass](../../spec-driven-development.md);
[Rust](../../../agent_docs/rust.md);
[Python rules](../../../sdks/wyrd-sdk-python/AGENTS.md);
[TypeScript rules](../../../sdks/wyrd-sdk-ts/AGENTS.md).
