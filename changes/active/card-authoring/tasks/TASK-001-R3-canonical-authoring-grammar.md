---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 4
spec_approval: approved
parent_task: TASK-001
remediates: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-007, AC-008]
depends_on: []
---

# Replace the Rust authoring vocabulary with the approved Card grammar

## Outcome and Value

A Rust user defines the same Service, Components and Verifiers that the YAML
defines, using its field names and values. A Service gets
`add_component(Component::new(...).verified_by(...))`, references remain
strings, and file and Card inputs both use `declare`. The user does not learn
add_verifier, Reference handles or a second declaration entry point.

This is the requested TASK-001 remediation for an **approved behavior
revision**, not a package of new review findings. No FIND IDs are assigned to
the human-directed API change. The original TASK-001 and its review records
remain history; revision 4 now owns the required public behavior. Implement
this task through `$wyrd-implement`, not through the old revision-3 tasks.

The deliverable is the complete common Rust surface and its Rust SDK caller
closure. Python/TypeScript packet examples now show the approved target; their
native bindings and language journeys remain separate projection work after
this shared meaning exists.

## What Changes and What Must Stay True

Replace the public edit/reference/declaration vocabulary with the complete
[approved support tables](../spec.md#proposed-public-support-surface), not just
the finance subset. All authorized named Card kinds use name/version
constructors and YAML-named field methods. Component is a first-class Service
sub-value with alias/ref, verified_by, source, config and credential_refs.
Model verification belongs on its Component; never add Model.verified_by.

Owned Rust setters return Self, mutable add methods return &mut Self, and
correctly typed construction/editing does not perform domain validation,
network IO or source reads. Keep incomplete and malformed domain input until
validate/declaration and use the existing native validators/diagnostic owners.
Language representations and private staging of incomplete values are local
implementation choices; a parallel schema, validator or graph compiler is not.

A full setter replaces its field and its dependency contributions. An add
method does not silently overwrite a duplicate: retain enough information for
validation to refuse attempted duplicates. Later field replacement deliberately
clears the replaced field's additions, including earlier duplicate attempts in
that field. Child capture remains by value, including dependencies still shared
by another current field. Declaration takes a fresh validated snapshot.

Preserve the existing capture/deduplication, reference-slot inventory, loader
order/root/validation, Artifact preparation/upload, registry saga and state
pull/hydrate workflows. Extend their input boundaries as necessary; do not
replace them with another orchestration implementation. Exact registered refs
stay external; Card values at permitted reference slots contribute sibling
definitions; inline native bodies keep their original meaning.

The coordinated API replacement costs call-site/test/rustdoc updates. Existing
native declaration-input and file callers must continue working; do not remove
canonical parser/reference types used by other capabilities merely because
authoring no longer requires public handles. Preserve current version/drift,
permission/tenancy, input-schema, scheduling, failure/cancellation and
post-publication retention semantics.

Simplify the runnable `examples/finance-release/finance_release.rs` as part of
this task, not only the packet sketch. Its smaller story is one report-writer
Component and a Service-level release-check with no-bank-account,
recipient-authorized and follows-policy requirements. Express each declaration
with the field-oriented API; remove redundant reference handles, call wrappers,
mutation scaffolding and fallible per-edit plumbing. Keep helpers that earn
their place, schemas, policy selection, output and error handling for actual IO.
Do not turn this introductory example into the separate two-component acceptance
story, move its Service binding to another subject, or impose a line-count target.

No wire/schema/endpoint, server behavior, migration, dependency/feature, retry
policy, rollback, runtime renaming or Python/TypeScript authoring implementation.
No broad unrelated cleanup or compatibility aliases for replaced authoring
methods. Existing runtime run_for_card is not being removed; packet authoring
examples use the existing run().for_card() form.

The checkout contains caller-owned uncommitted changes, including an authoring
component-binding draft and the real finance-release example. Reconcile them
with revision 4; do not discard or revert them. A current add_component_verifier
draft is superseded by the approved Component.verified_by surface.

## Approach

1. Revalidate the owner map below and inventory affected public exports and
   callers, including the current uncommitted changes.
2. Replace authoring constructors/field/collection inputs and introduce the
   approved Component value while preserving existing current-capture ownership.
3. Route completed snapshots and deferred strings/source descriptors through
   existing canonical parsing/compilation/validation, using YAML-path diagnostics.
4. Adapt the existing declaration owners to accept their approved input forms;
   retain publication-first sequencing, the current saga and shared hydration.
5. Convert all common Rust and SDK callers/tests and the existing finance-release
   Rust example to the approved API; simplify the example's declarations while
   preserving assertions, its smaller story and reader-visible output.
6. Close the native inventory, invalid-input, capture and real-server proof;
   update owning architecture/API rustdoc alongside the code.

## Ordered Implementation Scenarios

### Scenario 1 — Construct every native family with the YAML names and values

**Behavior.** A caller constructs named Cards, Components, slots, bindings,
steps, requirements and every Action/Check variant through the public Rust SDK.
Whole-field setters and incremental construction yield the same canonical
values. References use YAML strings, source slots use their string form,
parameterless operators use strings and parameterized operators their native
objects. Named Prompt Cards use name/version; inline Prompt bodies remain
canonical bodies. Trigger's flattened kind/cron/tz/after and fixed Artifact
card/path selections retain their real grammar.

**RED.** Adapt the existing native primitive and public SDK tests in
[local/public proof](#local-and-public-proof). Assert canonical values and
capture behavior through the approved surface. Confirm a runtime assertion
fails for missing behavior; compilation-only failure is not RED. Add only the
minimum surface needed to make that assertion executable.

**GREEN.** Provide the complete approved constructors and YAML field/add
methods over canonical contracts, including authorized kinds not used by the
story. Reuse native enum/Prompt/schema meaning. Preserve the default required
and independent-nullability proof. No future Python/TypeScript wrapper may
supply a missing native meaning.

**REFACTOR.** Remove duplicate field conversion or public authoring-only
indirection while the canonical comparisons remain green. Preserve native
contract types required elsewhere; no blanket reexports or variant structs
without an approved need.

### Scenario 2 — Validate complete snapshots once at the shared boundary

**Behavior.** Correctly typed constructors and edits accept incomplete values,
invalid names/versions, malformed reference/operator/source strings and forward
references without domain failure. validate() and declaration refuse invalid
completed snapshots with the existing error family and YAML field diagnostics.
A successful validate does not exempt a later edited declaration from validation.
CodeSource.file records a descriptor: construction/editing performs no read,
whereas validate/snapshot preparation reports an unreadable source.

Equivalent invalid file/code inputs exercise the same rule; source file/line
context may differ. Host-language argument type checking is not deferred.

**RED.** Add the planned local tests
`authoring::tests::field_edits_defer_validation_and_preserve_snapshots` and
`authoring::tests::yaml_and_code_share_diagnostics`; exact commands are in
[local/public proof](#local-and-public-proof). Cover invalid identity, unknown
input, invalid operator, required fields, valid forward reference followed by
complete-graph validation, source-read timing and mutation after validate.
The expected failure is current early refusal or unequal validation behavior,
not an unavailable server or compiler error.

**GREEN.** Normalize the completed local input through existing canonical
deserializers, Prompt compiler, loader and domain validators. Extend the
shared normalization/diagnostic owner when necessary so YAML and code converge;
do not implement a new language-specific parser or rule table. Validate before
any Artifact publication or declaration.

**REFACTOR.** Consolidate duplicate normalization/error adaptation at its
existing owner. Local staging may retain incomplete inputs, but no new durable
schema, cached validation result or separate validation engine is permitted.

### Scenario 3 — Replacement and additions preserve current capture semantics

**Behavior.** Exercise full setters and additions for inputs, steps,
requirements, components and bindings. Duplicate map keys, IDs and aliases
added through methods are not lost; validate/declaration refuses them.
Replacing a field resets that field, including prior additions/duplicate
attempts, without clearing another current field.

Replacing Trigger/workspace/component/binding/action values removes obsolete
dependency captures. Shared live dependencies remain. Equal active definitions
deduplicate, conflicting active same-identity definitions refuse, and edits
after child insertion or snapshot creation do not change the parent/snapshot.

**RED.** Extend the existing
`authoring::tests::offline_edits_preserve_snapshots` and
`authoring::tests::replacements_keep_only_current_dependencies` through the
new field/add surface; use [local/public proof](#local-and-public-proof).
Demonstrate duplicate-add preservation and whole-field reset, plus a replaced
local file with an absent old source and another live destination retaining it.

**GREEN.** Keep capture contributions consistent at the existing editing
owners and transfer their current values at parent attachment. Preserve the
existing Graph equal/conflict policy, source ownership and canonical inventory.
Do not silence conflicts, prune by identity alone or add downstream upload guards.

**REFACTOR.** Delete historical/duplicate capture paths exposed by the new
surface without introducing a new traversal, graph store, cache or registration
loop. Rerun scenarios 1–2 and the current-capture regressions.

### Scenario 4 — Declare file, Card and native input through existing owners

**Behavior.** Cards.declare accepts its existing native input plus path/Card;
WyrdState.declare/declare_with accepts path/Card with existing options and
returns the same ready state. Both Card entry points validate snapshots,
publish each local Artifact separately through the existing upload operation,
then submit the graph through the existing saga. Registered references cause
no source access/upload.

Exercise both entry points with local and registered Artifact forms. A server
graph refusal after publication retains the Artifact and exact graph error;
new immutable content under the same version still refuses. Path/native
callers, client selection, bundle handling, preparation/extraction validation
and scratch ownership stay valid. Remove authored-only public entry points.

**RED.** Adapt the existing
`authoring::local_artifacts_reuse_and_keep_versions` and
`authoring::declarations_match_files_and_keep_refusals` to unified names;
use [focused journeys](#focused-journeys). Assert actual source/upload/refusal,
identity/hash and hydration results, not merely a receipt. Use existing
path/native-input consumers to establish those overloads still compile/work.

**GREEN.** Extend argument adaptation on Cards and WyrdState while reusing
the existing publication calls, saga and pull/hydrate tail. Do not reimplement
lower lifecycle stages. Return the graph's
ordinary receipt and existing state/errors.

**REFACTOR.** Delete redundant authored entry points and normalization branches
once callers have moved. Preserve established file helpers/native-input
contracts and existing cancellation/retry behavior; no compatibility shim.

### Scenario 5 — Preserve both component journeys and dependent execution

**Behavior.** Author finance-release through the revised public surface.
Each explicit Component receives only its own verified_by binding, with its
existing two-step Verifier. Preserve every revision-4 case-matrix outcome:
allowed, identifier, recipient, destination, missing evidence, provider
uncertainty, malformed supplied input and other-component unbound lookup.
Check exact subject/Verifier attribution, old-version evidence and rerun identity.

The dependency variant still passes both prerequisite outputs to its summary.
Explicit-only and inferred references preserve native plan stages and existing
bounded concurrency. Running remains the existing component-view API.

**RED.** Adapt
`authoring::multi_component_service_verifies_each_subject` and
`authoring::dependent_steps_consume_prior_outputs`; run their exact commands
in [focused journeys](#focused-journeys). Keep all outcome and attribution
assertions. The new surface must prove behavior rather than skip old cases.

**GREEN.** Use the revised common API with existing test-server/Postgres/provider
owners and current planner/executor; no client scheduling or server changes.
Retain the checked-in baseline story and code/file parity. Named versus inline
Prompt construction must compare with the same file-authoring choice when
asserting pre-resolution parity.

**REFACTOR.** Reuse established fixture/provider support and remove obsolete
authoring syntax in callers. Retain real-server proof; do not substitute packet
syntax checks, generated Rust results or another test binary.

## Acceptance Criteria

| Obligations | Required observation / proof owner |
|---|---|
| REQ-001/002, AC-007 | Native primitive/public SDK comparisons cover every approved type, field/variant and both editing styles, beyond the finance subset. No separate operator/reference/Component grammar or required JSON-string workaround. |
| REQ-002/004/008, AC-002 | Edits defer domain refusal; complete validation shares YAML rules/paths, revalidates later mutations, retains attempted duplicates, resets only replaced fields and preserves snapshots/current shared captures. Local negative/regression proof. |
| REQ-003, AC-002/007 | All slots remain constructible; required/optional and accepting/rejecting supplied null remain independent. Existing canonical and input-engine evidence stays mapped. No new required field on samples/source. |
| REQ-005/006/008, AC-001/003/008 | Unified entry points preserve path/native input, graph receipt/state/options, local-first sole submissions, no-upload external refs, immutable drift and retained publication after graph refusal. Focused Artifact/parity journeys. |
| REQ-004/007, AC-004 | Both two-step Components retain the full verdict/error/uncertainty matrix, exact subject/binding and dependent outputs under existing native scheduling. Focused component/dependency journeys and concurrency regression. |
| INV-001–005, AC-008 | No wire/auth/server/lifecycle change or replacement engine. Native exports, Rust callers, real finance example, architecture and changed-item rustdoc use the approved vocabulary; old authoring aliases are gone, established unrelated APIs remain. Static diff/export/docs and focused lint/boundary proof. |
| REQ-001/002/006, AC-008 | The runnable finance example uses compact canonical field declarations and unified declare without Reference/AgentCall plumbing, while retaining its one writer, Service-level three-requirement binding and exact passing/failing output. Existing real-binary journey, below. |

REQ-009/010 and AC-005/006 belong to subsequent Python/TypeScript native
projection plans, not this task. Their full common meaning is a deliverable
here; their new language binding implementations and journeys are not.
The task index maps those remaining owners without claiming a complete
revision-4 language implementation plan already exists.

## Implementation Reference

These inspected owners/callers establish the reuse boundary, not an edit
allowlist or prescribed private representation.

| Existing owner and inspected consumers | Missing behavior and selected extension |
|---|---|
| wyrd-client authoring Definition, CardDefinition::declaration, Graph::add; current local tests and SDK story helpers | Change the public field/edit inputs and completion boundary; retain the existing by-value collection, same-identity dedup/conflict policy and source descriptors. No replacement graph owner. |
| Binding current Trigger capture/into_parts; AgentCall per-workspace capture/into_parts; Agent/Service attachment and Action/Check conversion | Preserve the proven replacement ownership while adding field/setter and first-class Component inputs. Additional mutable fields must follow current-value capture rather than historical append-only capture. |
| wyrd-loader parse::parse_single_envelope -> Spec::from_kind_and_value; declaration_input_from_cards -> validate_tree/order_cards/submission_for; existing LoadError -> WyrdError diagnostic projection | Current code accepts already typed Cards and eagerly parses some domain values. Extend the existing input/diagnostic seams for completed local definitions and shared YAML field failures; no second parser/validator. |
| Canonical Reference/SourceReference/operator serde, VerifierSpec::validate/plan; Skald AuthoredPrompt compilation | Parse strings/canonical objects at the approved validation boundary through those owners, rather than require handles or implement new rule/enum maps. |
| Cards::declare_from_path/declare/declare_authored/upload_artifact; cards::saga::declare -> prepare/submit/upload/complete | Existing native-input and file paths already converge on one saga; existing authored method only composes publication-first calls. Consolidate public input adaptation and remove its extra name while preserving the workflow, not reimplementing leaf stages. |
| ArtifactUpload::prepare and its scratch/source ownership | Keep per-publication maps, extraction/collision/change validation and scratch lifetime. Code-source descriptor IO moves to snapshot preparation through the existing source/canonical boundary, not an infallible setter. |
| WyrdState::declare_with/declare_authored_with -> pull_declared; existing declared-state and authoring journeys | Change input adaptation and reuse that tail, client and options. No new hydration workflow or temporary YAML bridge. |
| Rust SDK lib authoring reexport; integration/authoring.rs; examples/finance-release/finance_release.rs and examples/tests/finance_release.rs::finance_release_passes_and_fails | Close all current Rust callers/exports and simplify the real example, preserving caller-owned edits and the reader journey. examples/Cargo.toml registers the finance_release binary; the existing test executes it against a local-provider server and asserts exact pass/fail output. Packet examples are target syntax, not runtime proof. |
| Python state/registry.rs and TypeScript native/cards.rs callers of common declaration; existing DeclarationInput consumers | Preserve their existing native-input calls/type contracts as necessary for common-owner compatibility. No new Python/TypeScript authoring surface in this task. |
| architecture/wyrd-design.md In-memory authoring and adjoining file/reference/registry sections | Replace the owning subsection's old API/early-validation wording with approved behavior and truthful cancellation/IO timing; no parallel authority document. |

The canonical serde/Prompt/loader paths already own grammar and validation.
They need a completed-input seam; file discovery/path resolution does not need
to be copied into another loader. The existing publication-first composition
already provides the necessary sequencing; a generic new registry, strategy
hierarchy, graph store or workflow engine is unearned.

## Verification and Evidence

Run Cargo-backed checks sequentially. New selectors below are **planned**
inside confirmed existing packages/targets; confirm their actual collection
after adding them, before RED. Existing selectors were confirmed from source.

### Local and public proof

The two planned negative/edit tests:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::field_edits_defer_validation_and_preserve_snapshots)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::yaml_and_code_share_diagnostics)'
```

Existing native/public parity and capture tests, adapted without weakening:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::core_primitives_match_canonical_contracts) | test(=authoring::tests::offline_edits_preserve_snapshots) | test(=authoring::tests::replacements_keep_only_current_dependencies) | test(=authoring::tests::declarations_match_file_authored_tree)'
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'
```

If test layout changes for a justified private reason, record exact replacement
selectors and equal/non-weaker acceptance; do not silently drop a named proof.

### Focused journeys

Build the server first. Use the repository-managed Postgres lifecycle,
migration task and confirmed existing SDK integration target/profile:

```bash
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::multi_component_service_verifies_each_subject) | test(=authoring::local_artifacts_reuse_and_keep_versions) | test(=authoring::declarations_match_files_and_keep_refusals) | test(=authoring::dependent_steps_consume_prior_outputs) | test(=declare_and_pull::declares_ready_verification_state)'"
```

This is the task's focused five-test capability/regression slice, not the
full SDK journey suite. The existing path-state selector checks the shared
tail; the native DeclarationInput calls remain exercised by declaration and
upload consumers.

### Runnable finance example regression

Its declaration code is simplified while runtime behavior stays the same, so
reuse the existing reader journey rather than manufacture a new behavioral
RED or weaken its exact output assertions. After the server build above:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-rust-examples --test finance_release -P journey --run-ignored=all -E 'test(=finance_release_passes_and_fails)'"
```

This target is the existing examples/tests/finance_release.rs; it launches the
registered finance_release binary from a reader-like directory with the real
policy fixture and asserts unchanged passing and failing release-check output.

### Native scheduling and slot regression

No production execution change is authorized. Preserve the mapped native
behavior with these exact existing selectors:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::independent_actions_overlap_within_the_concurrency_bound) | test(=blocks::engine::tests::supplied_null_follows_the_slot_schema_not_requiredness) | test(=blocks::engine::tests::missing_and_optional_evidence_resolve_honestly)'
```

### Static, documentation and owner checks

Rustdoc/architecture/example updates do not require manufactured RED; inspect
every new/changed item including private/test helpers and applicable inherited
panic contracts. Compare the approved tables, exports, consumers and actual
callers for removed vocabulary rather than adding a permanent keyword checker.

```bash
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -- -D warnings
mise exec -- cargo doc --locked -p wyrd-client -p wyrd-loader -p wyrd-ai --no-deps
mise run check:deps
mise exec -- git diff --check
```

Use existing codegen/check tooling if a consumed SDK projection actually
changes; never hand-edit generated output. An unintended durable schema change
is a stop condition, not a reason to regenerate it into acceptance.
No full gate, all-language journey aggregate or new test harness. Broaden only
for a directly affected owner not covered by this write set.

Completion evidence records RED/GREEN/refactor, exact selectors/results, source
reuse, current-caller conversion, artifact/cancellation preservation, static
export/docs closure and immutable candidate. Do not report packet syntax
checks as compiled/native runtime proof.

## Material Stop Conditions

Return to specification authority for any required wire, server, binding-location,
permission/tenancy, durable identity, versioning, upload atomicity, concurrency,
dependency/feature or compatibility decision beyond revision 4. Do not add
Model.verified_by, early domain failure, a second grammar/validator or a new
registration workflow to sidestep input adaptation.

Private input staging, canonical conversion factoring, source-label diagnostics,
capture representation and test support layout are implementation-owned within
the approved outcomes. Replan only if the inspected existing owners genuinely
cannot supply an outcome without new material machinery; do not treat ordinary
borrow/generic/ffi call-site repairs as an approval gate.

## Authority Links

[Approved revision 4](../spec.md);
[original TASK-001](TASK-001-common-rust-authoring.md);
[task/review history](../review/TASK-001-review-02/verdict.md);
[repository rules](../../../../AGENTS.md);
[workflow](../../../spec-driven-development.md);
[implementation execution](../../../implementation-execution.md);
[testing](../../../../TESTING.md);
[Wyrd contracts](../../../../architecture/wyrd-design.md);
[Rust](../../../../agent_docs/rust.md);
[maintainer style](../../../../agent_docs/maintainer-style.md).

## Implementation Evidence

Candidate: commits `4a755aa4a`, `d049bcbd8`, `37b3c2f96` on `docs`.

**Reuse.** The loader already owned envelope decoding
(`parse_single_envelope` → `Spec::from_kind_and_value`) and tree validation;
its only gap was an in-memory entry point, so `wyrd_loader::parse_card_value`
feeds a JSON envelope through the same `parse_single_envelope`. Authoring
keeps the by-value capture and same-identity equal/conflict policy (`Graph`),
`declaration_input_from_cards`, `ArtifactUpload::prepare` via
`Cards::upload_artifact`, `cards::saga::declare`, and `WyrdState::pull_declared`.
No new parser, validator, graph store, or registration workflow.

**RED/GREEN.** The new tests could not exercise the old surface at runtime
(the old API refused early and had no field methods), so RED is recorded as a
mutation: with duplicate-key retention disabled in `Body::insert`,
`authoring::tests::offline_edits_preserve_snapshots` fails (duplicate slot
admitted); restored, all pass.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002, AC-007 every family, both editing styles | `authoring/{mod,kinds,verifier}.rs` `fields!` setters/adds for all 12 kinds, Component, Binding, InputSlot, Step, Requirement, Action, Check; `CardDefinition::new` for Policy/Source | `authoring::tests::core_primitives_match_canonical_contracts`; `authoring::public_paths_author_every_native_family` (56 operators, all slots, bindings, Component) | PASS |
| REQ-002/004/008, AC-002 deferred validation, shared YAML paths, duplicates kept, field reset, snapshots | `Card::decode` → `parse_card_value` + `declaration_input_from_cards`; `Captures.duplicates`; per-field `Body.captures`; `CodeSource::file` read per snapshot | `authoring::tests::field_edits_defer_validation_and_preserve_snapshots`; `authoring::tests::yaml_and_code_share_diagnostics`; `authoring::tests::offline_edits_preserve_snapshots`; `authoring::tests::replacements_keep_only_current_dependencies` | PASS |
| REQ-003, AC-002/007 slots, requiredness independent of nullability | `InputSlot::{json,media,trace,samples,source}` | `public_paths_author_every_native_family`; vala-verify `supplied_null_follows_the_slot_schema_not_requiredness`, `missing_and_optional_evidence_resolve_honestly` | PASS |
| REQ-005/006/008, AC-001/003/008 unified declare, local-first, no-upload refs, drift, retention | `Cards::declare(impl Into<Declarable>)`, `WyrdState::declare(_with)`; `declare_authored*` removed | journeys `local_artifacts_reuse_and_keep_versions`, `declarations_match_files_and_keep_refusals`, `declare_and_pull::declares_ready_verification_state`; `declarations_match_file_authored_tree` | PASS |
| REQ-004/007, AC-004 two Components, full matrix, dependent outputs, concurrency | `Component::new(..).verified_by(..)` | journeys `multi_component_service_verifies_each_subject`, `dependent_steps_consume_prior_outputs`; vala-verify `independent_actions_overlap_within_the_concurrency_bound` | PASS |
| INV-001–005, AC-008 no wire/server change; vocabulary closed | only client/loader/SDK test/example/design edits; `architecture/wyrd-design.md` In-memory authoring rewritten; packet example imports `authoring::CardRef` | `cargo check --workspace --all-targets`; clippy `-p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -D warnings`; `cargo doc`; `check:deps`; `fmt:check`; `git diff --check` | PASS |
| REQ-001/002/006, AC-008 finance example | `examples/finance-release/finance_release.rs`: one `report-writer` Component, Service-level `release-check` (3 requirements), `WyrdState::declare(&service)` | `wyrd-rust-examples --test finance_release finance_release_passes_and_fails` | PASS |

Commands run (all sequential, all passed): the four local-proof selectors and
the SDK unit selector exactly as listed above; `mise run build:server`; the
five-test focused journey slice and the finance example journey under
`scripts/postgres/with-test-postgres.sh`; the three vala-verify selectors;
`mise run fmt:check`; the listed clippy and doc commands; `mise run
check:deps`; `git diff --check`.

Non-goals held: no wire, schema, endpoint, server, migration, dependency or
feature change; no Python/TypeScript authoring; no `Model.verified_by`;
`run_for_card` untouched. Python and TypeScript native `cards.declare(&input)`
callers compile unchanged through `Declarable::Input`.

Limits: duplicate map keys keep the first value and are refused at
validation; diagnostics name the Card as `Kind/name@version` in place of a
file path. `Declarable::Card` boxes its definition (clippy
`large_enum_variant`).
