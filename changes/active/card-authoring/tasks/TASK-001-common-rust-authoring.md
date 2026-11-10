---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-card-authoring
spec_revision: 3
spec_approval: approved
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-007]
depends_on: []
---

# Author and register a multi-component Service through the common Rust client

## Outcome and Value

A Rust SDK user creates a Service and its components, adds multi-step Verifiers
to both components, and registers that graph without writing temporary YAML.
Cards declaration returns the existing receipt; state declaration returns the
existing hydrated state. The SDK verifies each component under its exact
subject and never resolves the other component's unbound Verifier.

This supplies the shared implementation Python and TypeScript will wrap, not
a new registration engine. Begin only after spec revision 3 is approved.

## What Changes and What Must Stay True

Expose the thin authoring representation and methods through `wyrd-client`
and the Rust SDK. Back values with canonical spec/reference/action/check types.
The new implementation is limited to authoring primitives/helpers,
serialization/conversion and argument adapters. Existing registration and upload
machinery is called, not replaced or reimplemented. No new registration path,
upload path, transport, protocol, lifecycle, retry policy or durable state.
Constructor and incremental styles converge on the same native submissions.
Local child insertion captures a value, mutations affect their receiver, and
registration captures a validated snapshot. Duplicate IDs/aliases fail rather
than overwrite. Keep later graph validation compatible with forward references.

### Complete native primitive surface

This task owns all shared authoring primitives, structs/enums, constructors,
inherent helpers, canonical conversions, validation delegation and public Rust
reexports. It is not limited to Service/Verifier or the example's AgentCheck
and AssertCheck. The inventory below is required coverage, not a mandate to
create duplicate structs for existing enum variants or to implement a new
execution engine.

| Primitive family | Required native surface |
|---|---|
| Card definitions | Service, Verifier, Agent/Prompt and Artifact authoring; existing typed Card/spec access for every other registrable kind; canonical metadata, identity and kind values. |
| Composition and bindings | ServiceComponent, VerificationBinding, subject/component attachment, existing Trigger/Operator references and binding fields; no invented component kinds. |
| Inputs and evidence declarations | Canonical JSON, media, trace, samples and source input forms, required/schema metadata, media limits/formats and existing source-reference values. No mandatory public JsonInput wrapper. |
| Actions | ExtractAction, AgentAction and CodeAction equivalents over the complete canonical Action enum, including AgentCall and CodeCall parameters. |
| Checks | AssertCheck, AgentCheck, CodeCheck, TraceCheck and DriftCheck equivalents over the complete canonical Check enum. Include path/each, all native ComparisonOperator values, Agent workspace/input bindings, code response formats, trace selectors and existing drift configuration. |
| Steps and decisions | Step, Requirement, Condition/when, depends_on, retries/timeouts and Decide variants including all/pass_rate, preserving existing invariants. |
| References | CardRef, sibling versus external declaration intent, inline/named references, Reference/source-reference values, input helpers and step output/path helpers. |
| Artifacts and source selections | Local file/directory source descriptors, extraction option, fixed ArtifactSelection/ArtifactFile helpers, WorkspaceSource variants and artifact.file(path). Reuse upload preparation rather than define upload behavior anew. |
| Code and Agent configuration | CodeSource inline/exact Artifact variants, CodeRuntime, source-file capture helper, native Agent/Prompt specs, route/tools/run configuration and supported response-format types. |
| Common operations | Direct constructors and add methods, equality/common assertion conveniences with access to every native operator, canonical serialization/normalization, shared errors and client/SDK public exports. |

Existing pure contract types remain in wyrd-spec; authoring behavior and
preparation stay in their existing client/loader owners. Reexport or extend
them through wyrd-client where appropriate. If a named primitive is best
represented by an existing enum variant plus an inherent constructor, that
satisfies the requirement; a standalone duplicate struct is not required.

Tasks 2/3 may add only binding adapters and language-specific conveniences.
They may not define the missing Rust meaning of AgentCheck, AssertCheck or any
other family above. Task 1 is incomplete until this inventory has public Rust
coverage and canonical parity proof, even if the multi-component journey passes.

Reconcile in-memory definitions with existing loader reference inventory,
Artifact preparation, `Cards::declare` and state declare/pull/hydrate. No
temporary file export, new wire envelope, transport, graph store, scheduler,
feature or dependency. No duplicate upload engine or registration machinery.
Existing Artifact CardRefs are used directly without uploading. A local path
uses existing ArtifactUpload::prepare and declaration/upload machinery to
publish that Artifact first as a sole submission. Then the containing graph
uses its exact registered reference. Both Cards and state entry points perform
this sequencing inside the common SDK call; Python/TypeScript only forward.
Preserve preparation validation, scratch lifetime, versioning and existing
retry/error behavior. If Artifact publication succeeds but graph declaration
fails, retain the Artifact; there is no rollback guarantee. Source maps remain
scoped to each existing Artifact request, not merged across publications.
Sibling intent remains for Cards within the final graph request, distinct from
the external reference to a just-published or pre-existing Artifact.

The shared `card_authoring` journey uses Service `finance-release`, distinct
Agent components `report-reviewer` and `release-controls`, and their respective
`report-policy` and `release-access` bindings. Each Verifier has two steps, as
defined in the spec. Reuse one checked-in story's source/policy/input evidence
across all SDK tasks. Tests use real WyrdTestServer/Postgres with a local provider
and original-evidence citations. New fixture YAML belongs in the established
fixture tree and is the file-authored comparison, not a replacement for building
the graph through the new interface.

Sample recipient/destination records are synthetic, not network inspection.
Only passed permits simulated release. Errors and inconclusive stay distinct;
no immediate-persistence guarantee is added.

## Approach

1. Trace native submissions, reference rewriting, local upload preparation,
   `Cards` declaration and state hydration using the inspected owners below.
   Identify how authored values become their existing inputs; do not introduce
   another registration/upload path or replay their private workflow.
2. Map and cover every family in the native primitive inventory, then implement
   its minimal constructors/helpers/conversions over existing contracts. Do not
   stop at the finance journey's subset or defer primitives to wrapper tasks.
3. Add thin authored-value argument adapters to existing Cards/state operations,
   using idiomatic Rust signatures and public client/SDK reexports. All actual
   registration and upload work delegates to existing machinery.
4. Implement the shared checked-in story and real component-scoped journey,
   including exact binding/subject identity and original policy evidence.
5. Close Artifact reuse/version/error paths and file/constructor parity, then
   update owning architecture/API docs for the newly accepted client input.

## Ordered Implementation Scenarios

### Scenario 1 — Construct and edit canonical definitions locally

**Behavior.** Construct Service/Verifier/Step/Requirement values without a
server. Construct and lower every family/variant in the complete primitive
inventory, including unused Trace/Drift checks and Extract/Agent actions, to
its canonical type without changing supported execution semantics. Edits
preserve native field/reference semantics; duplicate IDs/aliases
refuse. Adding a child captures its value and later source edits do not alter
the parent. Required versus nullable slots stay distinct. Unknown references
and cycles fail at the existing shared validation boundary when the graph is
complete. Assert native stage ordering for explicit-only dependencies and
inferred output references, not just cycle rejection. Repeated serialization
does not emit local paths or derived status.

**RED.** Add planned inline native authoring test
`authoring::tests::offline_edits_preserve_snapshots` and
`authoring::tests::core_primitives_match_canonical_contracts` in the owning client
library. The latter uses small variant tables to cover the whole inventory,
not only checks appearing in the real-server story. Confirm actual selectors
before [local proof](#local-proof).
Start with a missing-behavior assertion; compilation alone is not RED. If
new symbols are needed, introduce only enough skeleton to run that assertion.

**GREEN.** Reuse existing contract values and validation to provide direct
construction, edits, reference/check/action/Artifact helpers, optional-slot
metadata and canonical conversion for the entire native inventory. Use normal
structs/inherent methods, not a parallel validator or
generic lifecycle abstraction. Constructors/add methods perform no network IO.

**REFACTOR.** Remove duplicate field/reference conversions, keeping local
source metadata outside durable specs. Deduplicate equal reused definitions,
and refuse conflicting definitions of one identity. Preserve earlier snapshot/error
assertions. Private module layout and final selector remain implementation-owned.

### Scenario 2 — Verify both authored components through their own bindings

**Behavior.** Build and register the specified two-component graph through
public Rust authoring, obtain ready state, and open each component's Run.
Both have two executed steps with expected requirement evidence and exact
subject/Verifier refs. Exercise permitted, prohibited identifier, unauthorized
recipient and unsuitable destination cases. Uncertainty holds the simulated
release. Use the spec's REQ-007 case matrix for missing evidence, provider
uncertainty and malformed supplied JSON, including exact input-error code.
Asking either component for the other's binding returns exactly
`WYRD_SDK_404_UNKNOWN_VERIFIER`. An unchanged rerun keeps identities.

**RED.** Add planned integration test
`authoring::multi_component_service_verifies_each_subject` to the existing Rust
SDK integration target. Use real repository Postgres and test-server/provider
fixtures. Run [component journey](#component-journey); assert missing graph,
binding or outcome behavior rather than accepting an arbitrary response.

**GREEN.** Connect native authoring to existing declaration and state owners,
publish client/SDK reexports, and create the reusable fixture story. Preserve
existing component binding semantics and default all decision policy. Reuse
the existing server planner, whose concurrent stage behavior has regression
proof below. Rerun Scenario 1 and this journey.

**REFACTOR.** Keep fixture/provider setup at established test owners. Avoid
language-specific serialized expected graphs; the common fixture describes
intent and every SDK must build its own definitions.

### Scenario 3 — Publish local policy first or reuse its existing CardRef

**Behavior.** Exercise the authored Service workflow through both Cards and
state entry points with both Artifact source forms:

| Source | Required observation through each entry point |
|---|---|
| Local file/directory path | Existing preparation/extraction and publication complete first. Each Artifact-bearing request is a sole submission; subsequent graph declaration contains the exact registered Artifact reference, not a mixed heavy submission. |
| Existing exact Artifact CardRef | Graph uses that reference directly; no Artifact preparation, extraction or upload is performed. |

Multiple executions reuse the pinned policy. New policy/Verifier/subject
versions leave old state attributed to the old policy. Preserve preparation
validation for each request, including missing/unreadable sources, changed
bytes and extraction-output collisions. Distinct Artifact requests do not
share a source map or silently overwrite files. Keep prepared extraction
scratch alive through transfer.

If local Artifact publication succeeds and graph declaration is then refused,
the published Artifact remains readable and reusable. A subsequent authored
Service using its existing CardRef declares without reuploading. Force a real
post-publication graph failure, not a local validation failure that prevents
publication from beginning. Preserve the original graph error and normal retry
rules; no cleanup/rollback or synthetic aggregate success is allowed.

**RED.** Add planned integration test
`authoring::local_artifacts_reuse_and_keep_versions`; run
[Artifact journey](#artifact-journey). Prove file reads/selection and exact
identities, both entry-point/source combinations, sole-submission compliance,
no upload for an existing CardRef, and retained publication after graph failure.
Run against unchanged server validation; use existing request capture where
needed rather than introducing production observability for the proof.

**GREEN.** Reuse ArtifactUpload::prepare and existing publication machinery
before declaring the containing graph, substituting the returned exact CardRef.
Keep each Artifact-bearing request a sole submission. Existing CardRefs bypass
the publication path. Retain scratch ownership through transfer and leave
published Artifacts intact if later graph registration fails. Fixed selections
cannot be substituted by request evidence. Rerun earlier proof.

**REFACTOR.** Remove redundant local upload/parse code and reuse existing owner
errors. No cache, mutable latest-policy reference, duplicate upload engine or
registration machinery. Sequence existing owners within the common SDK call.

### Scenario 4 — Keep both entry points equivalent to file authoring

**Behavior.** The canonical baseline authored in code and from checked-in files
has equivalent staged Artifact publications and containing-graph specs, with
the published Artifacts represented as external exact references. Sibling intent
is compared among definitions in the containing-graph request. Registration yields
the same resolved content/identities under unchanged versions. Cards returns
its normal receipt; state declaration returns ready state. Existing path and
selected-holder flows remain valid. Changed immutable content and insufficient
authority return their existing exact error codes.

**RED.** Add planned integration test
`authoring::declarations_match_files_and_keep_refusals`; run
[parity journey](#parity-journey). Compare pre-resolution references through
the owning native conversion proof and registered content through public SDK
reads, not by loading a predeclared Rust result as the test input.

**GREEN.** Adapt authored inputs to existing Cards and state methods without
creating a new registration route. Delegate to their existing machinery,
preserving flags, errors and cancellation. Keep the existing
flat submission contract and root derivation. Rerun all scenarios.

**REFACTOR.** Share path/object normalization where the existing owner supports
it. Delete accidental duplicated traversals rather than adding a new parser,
graph compiler, registration loop or temporary on-disk intermediate.

### Scenario 5 — Execute an authored dependent step with prerequisite outputs

**Behavior.** In a small extension of the common access Verifier, add a
dependent summary action that consumes the recipient/destination outputs.
Reference helpers lower to the existing expressions; explicit depends_on
metadata is retained. The summary sees both completed outputs and its
requirement produces the expected judgment. The baseline common graph remains
two steps per component; this is a dependency variant, not a third binding.

**RED.** Add planned integration test
`authoring::dependent_steps_consume_prior_outputs`; run
[dependency journey](#dependency-journey). Assert the actual combined output
and judgment. Scenario 1 separately asserts native ordering for explicit-only
edges so inferred references cannot mask lost depends_on metadata.

**GREEN.** Preserve canonical references/dependency fields in shared conversion
and let the existing planner/executor stage work. Rerun prior scenarios; add no
client scheduler, timing assertion or server planning endpoint.

**REFACTOR.** Reuse native dependency/reference types and checked-in source
fixtures. Delete redundant author-side stage computation if introduced.

## Acceptance Criteria

| Obligations | Expected evidence |
|---|---|
| REQ-001–004, REQ-008; AC-002; INV-001–003 | Scenario 1 proves direct typed edits, snapshots and slot/reference semantics without network access; canonical types remain the owner. |
| REQ-004, REQ-007; AC-004; INV-003–005 | Scenario 2 proves both multi-step components and correct attribution, pass/fail/uncertainty, unbound rejection and reuse. Existing bounded-concurrency test plus the authored independent graph proves scheduling is preserved. |
| REQ-005–006; AC-003; INV-002, INV-004 | Scenario 3 proves local-path publication before graph declaration and existing-CardRef reuse through both entry points, sole-submission requests, scratch/validation/version/error preservation and retained Artifact after graph failure. |
| REQ-001, REQ-006, REQ-008; AC-001; INV-001–002 | Scenario 4 proves canonical parity, existing Cards/state outcomes and unchanged authority/immutable-version refusals. |
| REQ-004, REQ-008; AC-002; INV-003 | Scenario 5 proves prerequisite output consumption through authored references; Scenario 1 proves explicit-only dependency ordering. |
| REQ-001; AC-007; INV-001, INV-003 | Scenario 1's full primitive test and public SDK compile coverage prove all inventory families/variants and helpers exist in Rust and match canonical contracts. The finance journey alone cannot satisfy this row. |

## Implementation Reference

- **Types:** `ServiceSpec`, `ServiceComponent`, `VerifierSpec`, `Step`,
  `Requirement`, `Action`, `Check` and `Reference` in `wyrd-spec` already own
  durable shapes. `VerifierSpec::plan` infers stages and validates dependencies.
  Missing: ergonomic in-memory construction/reexports and argument adaptation
  that composes existing publication/declaration calls, not new contracts or
  registration/upload machinery.
- **Normalization and sources:** `wyrd_loader::DeclarationInput` owns
  submissions/source maps; `load` and `build_declaration_input` in
  `crates/shared/wyrd-loader/src/lib.rs` already compare/rewrite complete
  declaration graphs. `ArtifactUpload::prepare` in `src/upload.rs` retains
  extracted scratch. `Cards::upload_artifact` prepares one Artifact and calls
  ordinary declaration with that sole submission; its returned exact CardRef
  is the containing graph's external dependency. Keep source validation and
  maps scoped to each such call. Do not compare only upload leaf methods or
  combine heavy submissions with the containing graph.
- **Lifecycle:** `Cards::{declare,declare_from_path,upload_artifact}` in
  `crates/shared/wyrd-client/src/cards/handle.rs` and `cards/saga/mod.rs` own
  registration, transfer, completion and outcomes. `WyrdState::declare_with`
  in `src/state.rs` composes declare/pull/hydrate; `bound_verifier` and
  `run_for_card` select component subjects. Missing: object input, not a second
  registration/state owner or execution path. Public reexports belong in client/SDK modules,
  not private downstream crate imports or the internal feature.
- **Consumers/tests:** Rust SDK `tests/integration/main.rs` owns one target;
  add the planned `authoring` story there. `support::Deployment` supplies the
  server/client; `verifier_building_blocks.rs` supplies local provider and real
  uploaded-document patterns. `verify_in_real_time.rs` uses component Runs and
  verifies exact subjects. `fixtures/cards/verify_in_real_time/assistant.yaml`
  shows component binding structure but does not cover this two-multi-step
  authoring story. New story/fixture support is required; no new test binary.

## Verification and Evidence

All new selectors below are planned. Confirm actual collection/selector after
adding tests and before RED; an owner-module move is a private correction, not
permission to weaken proof. Run Cargo commands sequentially. Build the server
with `mise run build:server` before integration commands; their Postgres wrapper
and migrations match `test:rust-sdk:journey`. Local provider fixtures, not live
credentials, supply judge responses.

### Local proof

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::offline_edits_preserve_snapshots) | test(=authoring::tests::core_primitives_match_canonical_contracts)'
```

### Component journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::multi_component_service_verifies_each_subject)'"
```

### Artifact journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::local_artifacts_reuse_and_keep_versions)'"
```

### Parity journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::declarations_match_files_and_keep_refusals)'"
```

### Dependency journey

Planned test in the existing integration target:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::dependent_steps_consume_prior_outputs)'"
```

### Existing concurrency regression

Existing engine test, not a new timing benchmark:

```bash
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::independent_actions_overlap_within_the_concurrency_bound)'
```

Run `mise run fmt`, `mise run lints`, `mise run check:deps` for the shared owner
boundary and `mise exec -- git diff --check`. Update required rustdoc and
owning client-authoring architecture prose. No durable schema change is
authorized; if generated consumed contracts actually change, use their existing
generator and check rather than editing artifacts. Broad SDK journeys run once
on the integrated candidate after all three tasks.

## Material Stop Conditions

Stop if correctness requires changing existing registration semantics or
replacing registration/transport/graph-storage machinery,
server endpoint, schema, security/tenancy semantics, dependency/feature, changed
saga atomicity, or replacement execution scheduling. Do not promise alias
occurrence isolation beyond the distinct-subject story. Private normalization
factoring, fixture shape, wrappers and equivalent commands remain local choices.
Publication-first sequencing through the existing owners is explicitly in
scope and is not the withdrawn architectural blocker. Do not change the server's
sole-submission rule to avoid that sequence.
Do not hand off a missing primitive to Task 2/3 or treat wrappers as the owner
of native definitions. Missing inventory coverage is unfinished Task 1 work.

## Authority Links

- [Approved spec revision 3](../spec.md), [language sketches](../examples/README.md)
- [Repository rules](../../../../AGENTS.md), [Rust](../../../../agent_docs/rust.md)
- [Wyrd design](../../../../architecture/wyrd-design.md), [security](../../../../architecture/wyrd-security-posture.md)
- [Workflow](../../../spec-driven-development.md), [execution](../../../implementation-execution.md), [testing](../../../../TESTING.md)

## Evidence

### Reuse map

| Need | Existing owner reused | Gap closed | Why sufficient |
|---|---|---|---|
| Durable shapes | `wyrd-spec` `VerifierSpec`, `Step`, `Action`, `Check`, `ServiceSpec`, `AgentSpec`, `InlineableRef`, `ArtifactSelection` | `wyrd-client/src/authoring/` holds canonical values plus captured dependencies; `Reference::{input, step_output, at}` build references by parsing | No parallel contract; every wrapper exposes its canonical value. |
| Graph validation and order | `wyrd_loader::validate::validate_tree`, `order::order_cards`, `order::submission_for` | `wyrd_loader::declaration_input_from_cards` runs them over in-memory `AuthoredCard`s | Same rules and order as a file tree; no temporary files. |
| Artifact publication | `Cards::upload_artifact` → `ArtifactUpload::prepare` → `Cards::declare` | `Cards::declare_authored` calls it once per local Artifact, then `Cards::declare` | No new upload or registration path; each Artifact request is its own sole submission. |
| State | `WyrdState::declare_with` pull/hydrate tail | Factored into private `pull_declared`, shared by `declare_authored_with` | One hydrate path. |

Decision: authored Artifact references are external exact `{kind, name, version}` with no UID. Loader validation refuses authored UIDs and the registry pins them, so the code and file forms resolve identically.

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001–004, REQ-008; AC-002; INV-001–003: offline typed edits, snapshots, duplicate refusal, slots, ordering | `crates/shared/wyrd-client/src/authoring/{mod.rs,verifier.rs}` | `authoring::tests::offline_edits_preserve_snapshots` | PASS |
| REQ-001; AC-007: every inventory family/variant matches canonical contracts | same; `Check::{assert,each,equals,trace,agent,code,code_with_format,drift}`, `Action::{extract,agent,code}`, `InputSlot` json/media/trace/samples, `Decide`, `Condition`, retries/timeouts, `Linked` inline/sibling/registered, `Binding` trigger/operators, `Prompt`, Agent route/tools/run config | `authoring::tests::core_primitives_match_canonical_contracts` (decodes canonical YAML and compares; explicit-only and inferred stage ordering; unknown/self dependency refusals) | PASS |
| REQ-008; AC-001: code-authored graph equals the file-authored tree before resolution | `CardDefinition::declaration`, `fixtures/cards/card_authoring/` | `authoring::tests::declarations_match_file_authored_tree` (perturbing one YAML byte fails it) | PASS |
| REQ-004, REQ-007; AC-004; INV-003–005: two components, own subjects/bindings, REQ-007 matrix, unbound refusal, idempotent rerun | `WyrdState::declare_authored_with`, `Cards::declare_authored` | `authoring::multi_component_service_verifies_each_subject`; `blocks::engine::tests::independent_actions_overlap_within_the_concurrency_bound` | PASS |
| REQ-005–006; AC-003; INV-002, INV-004: local policy publishes first, CardRef reuse without upload, versions, refusals, retained Artifact after a server-refused graph | `Cards::declare_authored` | `authoring::local_artifacts_reuse_and_keep_versions` (`WYRD_REGISTRY_409_SPEC_DRIFT`, `WYRD_LOADER_400_INVALID_ENVELOPE`, `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY`) | PASS |
| REQ-001, REQ-006, REQ-008; AC-001: same registered identities/hashes as files; drift and permission refusals through both entry points | same | `authoring::declarations_match_files_and_keep_refusals` (`WYRD_REGISTRY_409_SPEC_DRIFT`, `WYRD_PERMISSION_403_DENIED_RBAC`) | PASS |
| REQ-004, REQ-008; AC-002: dependent step consumes prior outputs, keeps `depends_on` | `Step::output`, `Step::add_depends_on` | `authoring::dependent_steps_consume_prior_outputs` | PASS |

Commands: `mise run build:server`; the task's local-proof, journey, and concurrency commands (the journeys also ran together under `-E 'test(/^authoring::/)'`, 4 passed); `mise run fmt`; `mise run lints`; `mise run check:deps`; `mise run check:docs`; `mise run docs:check`; `mise exec -- git diff --check`. All passed.

Non-goals held: no new registration, upload, transport, endpoint, schema, feature, or dependency; no rollback; no client scheduler. The third unit test, `declarations_match_file_authored_tree`, sits in the owning crate because the Rust SDK has no loader dependency and adding one was out of scope.
