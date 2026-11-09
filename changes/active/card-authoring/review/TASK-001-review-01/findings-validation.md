# Independent findings validation — TASK-001

Recommendation: **FIX_REQUIRED**. Three retained findings, `FIND-TASK-001-1` through `FIND-TASK-001-3`. No new blocker was discovered by this validator. No additional discovery/follow-up is needed for these findings.

## Subject, authority and independence

First review of base `5234961d51798581a026834cfffd5efaadc226b4` to candidate `790da3369c1b0107765db234d72c1ae0c4680446`. No prior verdict, findings or acceptance evidence carries forward. Approved spec revision 3 and original TASK-001 govern acceptance, subordinate to root AGENTS.md and architecture under `changes/spec-driven-development.md`. Read those authorities, `architecture/README.md`, applicable reference/Artifact/Verifier/client authoring sections of `wyrd-design.md`, `agent_docs/index.md`, Rust rustdoc rules and maintainer style. No CodeGraph index or narrower AGENTS governs the changed implementation directories.

Read all eight selected discovery reports, `claim-comparison.md`, `verification.md`, subject/navigation/selection record, original task and approved spec. Inspected actual cumulative diff and source rather than taking reviewer agreement as proof. Read complete authoring modules and local tests, complete affected Cards/state methods, SDK construction/assertion helpers, and expanded to canonical slots/operators/planner, loader validation/order, Artifact preparation, and server external identity resolution/binding/writer. Source inspection is focused on the proposals and their acceptance seams; it is not a new comprehensive discovery pass.

HEAD matches candidate. Tracked worktree and index are clean. Packet SHA-256 matches the subject:

- spec: `73d8be31b64a88eb4d9c99517dca0f1f39bd705e60f8732c91cd47b3233d79fc`;
- original task: `b71ccb16efcc4bbdf28e01bad1c3f41167900c5b93f04525a2a15b5019e7ddc1`.

The packet is untracked, so these hashes, rather than Git membership, establish its unchanged content. Only this report is written. No Cargo process, source edit, packet edit, commit or push was performed.

## Specialist applicability and coverage

The selected roles cover the actual affected contracts, including unchanged consumers. There are no omitted required specialist families and no missing selected reports.

| Applicable obligation | Independent coverage assessment |
|---|---|
| Complete task behavior and non-goals | behavior-rev covers all scenarios and provides a full matrix, including explicit primitive/slot proof. Its FAIL rows are reconciled below rather than inherited as votes. |
| Captured shared values, identity and graph lifecycle | invariant-rev traces producers, sibling writers, Graph, loader and both entry points. New recursive capture makes this role necessary. Replacement defect is independently confirmed below. |
| Broad cross-tier contributor gates | repo-rev covers client, loader, spec, SDK, fixtures, architecture, tests/private documentation and exports. Applicable changed languages and ownership boundaries are accounted for; no SQL implementation or foreign-runtime projection is changed. |
| Ownership/abstraction and workflow reuse | maintainer-rev compares complete loader discovery/resolution versus value collection, not merely upload leaf calls. I verified that Graph does not implement ordering/root/composition validation: loader/order delegates those to canonical spec owners. New value capture is warranted; duplicate workflow DRIFT is unsupported. |
| Publication interruption, shared service availability and hydration | system-rev traces upload/saga ownership, scratch lifetime, retained publication and unchanged pull tail; no new server worker/process failure boundary exists. Static cancellation analysis is appropriate for unchanged lower owners; actual graph-refusal recovery has a live journey. |
| Security, tenancy and subject/binding trust | security review follows authenticated Cards/state through existing server permission/tenant resolution, and component verification/workspace consumers. Both entry points have exact RBAC refusal checks and the component journey has exact unbound refusal checks. A new two-tenant or hostile-policy test is not required merely because unchanged owners remain dependencies. |
| Persistent identity, version/content and source ownership | persistence review covers separate publication/source maps, exact identity/hash pinning, retained publication and registered bypass. Its UID caveat is resolved explicitly below. |
| Shared execution dependency/concurrency | concurrency review follows explicit/inferred references through canonical planner, engine stages/barriers, semaphore and settled-output consumers. Fresh bounded-concurrency regression plus authored dependent-output journey supports preservation. |

Follow-up selection is sufficient: there is one common replacement cause, no competing diagnosis or unresolved source path, and proof/rustdoc claims are directly decidable. A unique proposal (the primitive proof gap) does not require a second discovery vote. No specialist needs to be added to validate a new boundary for the bounded corrections below.

## Proposal decisions

| Discovery proposal | Decision | Ledger result |
|---|---|---|
| BEHAVIOR-001, INV-01, M-001: historical dependencies survive replacement | **CONFIRMED**, consolidated | FIND-TASK-001-1; both replacement families belong to one capture invariant, not separate findings per consumer. |
| BEHAVIOR-002: complete native/public primitive and required/nullable proof | **REVISED** | FIND-TASK-001-2; retain the explicit missing proof, not an unproved claim that types cannot be constructed or a requirement for blanket reexports/new wrappers. |
| REPO-001, M-002: changed-item rustdoc | **CONFIRMED**, consolidated | FIND-TASK-001-3; retain private/test docs, applicable errors/panics, cancellation and inaccurate scheduling prose as one mandatory gate correction. |
| UID wording as a new persistence/wire defect | **REJECTED** as a material implementation finding | No ID; durable UID pinning remains server-owned. Literal limitation and authority reconciliation below. |
| New Graph/loader/Cards adapter is forbidden duplicate machinery | **REJECTED** | No ID; existing file resolution cannot capture in-memory child/source values, while ordering/validation and registration/upload remain existing owners. |

Rejected proposals create no optional remediation work.

## Retained ledger

### FIND-TASK-001-1 — Replacement edits retain dependencies that no current slot owns

**Status:** CONFIRMED. **Classification:** INCORRECT. **Discovery:** BEHAVIOR-001, INV-01, M-001.

**Obligation:** REQ-002 editable receiver/value capture, REQ-008 equivalent constructor/incremental specs and staged sequences, AC-002 and Scenario 1. This is normal public incremental editing, not a hypothetical malformed wire input.

**Exact producer locations:** `crates/shared/wyrd-client/src/authoring/mod.rs:450` (`Binding::set_runs_on`) and `authoring/verifier.rs:537,545` (`AgentCall::add_workspace_input`, `add_workspace_file`). Shared append-only collection: `mod.rs:72`; consumer: `Graph::add` at `:253`; IO sink: `cards/handle.rs:354–367`.

**Independent trace:** `set_runs_on` overwrites `binding.runs_on`, then absorbs all new dependencies into the historical bag. Neither old Trigger capture nor its nested captures are removed. `add_workspace_file` overwrites one map entry and appends its local Artifact; `add_workspace_input` overwrites the entry with Scope and leaves the old Artifact capture untouched. Linked sibling construction captures whole definitions; ArtifactFile captures local Artifacts only. Action/Check transfer AgentCall dependencies, Step/Requirement and Verifier absorb them, and Agent/Service binding/component insertion transfers them into the declaration root. Graph unconditionally visits every historical child/source and compares same-identity definitions; it does not derive liveness from current canonical slots. Cards uploads every collected local Artifact before graph declaration, and state delegates to that same Cards method.

Read the full bodies of all these producers, converters and sinks, including `Binding::{new,registered,add_on_failure,add_on_verdict_change}`, `AgentCall::{new,add_input}`, `ArtifactFile::{selection,dependencies}`, `CodeCall::{artifact,with_source}`, Action/Check conversions, Step/Verifier add methods, Agent/Service attachment and compose. Additive operator/binding/step/requirement methods legitimately accumulate dependencies. CodeCall has no replaceable Artifact source setter. Scalar edits do not own captured dependencies. The failing writes are the replaceable Trigger slot and replaceable workspace destinations, not every use of `absorb`.

**Reachable failure and consequence:** Set a sibling Trigger T with spec A and then replace it with same-identity T/spec B before attachment. Canonical binding only references B, but Graph visits A and B and returns `SpecDuplicateSubmission`; direct B-only construction succeeds. Independently, mount a local policy then replace that destination with Scope input: final call has no Artifact selection, but declaration still publishes the old source or fails reading an obsolete missing path. These side effects and false conflicts violate equivalent final-definition semantics and make ordinary edits unreliable.

**Decision-complete correction boundary:** At Binding and AgentCall, make captured dependencies belong to the current replaceable reference position. Overwriting that position must replace its contribution before the value is transferred into parent dependencies. Preserve other live positions, including the Agent definition's own dependencies and the same Artifact still mounted elsewhere; preserve nested child captures, equal-definition deduplication and genuine active same-identity conflict refusal. Keep the existing public setter/map overwrite behavior and canonical values. The exact private representation is implementation-owned.

**Burden / smaller alternatives:** Bounded source bookkeeping in these existing value owners is sufficient. Deleting setters loses required edits. Refusing replacements changes their supported semantics. Removing conflicts or choosing the latest identity weakens genuine active conflicts and leaves stale distinct identities. Upload guards fix neither false local conflicts nor obsolete sibling submissions and do not own slot liveness. No global reference cache, second traversal/validator, new upload owner or rollback is justified.

**Focused closure proof:** Extend the owning local authoring tests to compare directly built and replacement-edited declarations: same-identity Trigger A→B, local-file workspace→Scope and local-file→different local-file. Assert only current submissions/publication descriptors remain; include two workspace destinations sharing an Artifact, replace one, and prove the other still retains it. Include genuine current conflicting captures so correction cannot simply suppress conflict checks. An absent old source with zero obsolete Artifact descriptors proves the unwanted-IO cause is removed because the unchanged Cards loop consumes exactly those descriptors. No new live journey or request harness is needed for this correction.

### FIND-TASK-001-2 — Required complete-native and slot-adapter proof is not complete

**Status:** REVISED. **Classification:** MISSING. **Discovery:** BEHAVIOR-002.

**Obligation:** REQ-001/AC-007 explicitly require full native family/variant parity and public Rust coverage before wrapper handoff. TASK-001's inventory and Scenario 1 specifically name all ComparisonOperator values, source inputs/references, Trigger/Operator binding fields, other registrable canonical Specs, and required/schema metadata. REQ-007 separately requires required/nullable variants tested at native/input-adapter boundary. This is explicit bounded acceptance evidence, not a demand for exhaustive arbitrary inputs.

**Locations:** `crates/shared/wyrd-client/src/authoring/tests.rs:43,182,453,744`, and public SDK construction coverage in `sdks/wyrd-sdk-rust/tests/integration/authoring.rs:21` onward.

**Independent evidence:** Read all three local tests and all construction helpers. Every operator use is Equals. Canonical InputSlot has five variants, while tests cover Json/Media/Trace/Samples but no Source or source-reference construction/parity. No test invokes `set_runs_on`, `add_on_failure` or `add_on_verdict_change`; those required binding fields receive no native parity proof. No construction table covers the other registrable Spec kinds. Existing tests cover an optional unconstrained slot and the story's optional string notes omission, but never the independent required/optional × accepting/rejecting-null schema cases. Public SDK journeys import/construct the story subset; private client tests use direct `wyrd_spec`/`skald_spec` imports. That does not prove complete SDK-facing construction for absent families.

**Producer/consumer scope:** Verifier `add_input_with_required` and `add_slot` preserve canonical schema and required fields separately; no conversion bug is established. Existing execution admission `vala-verify/src/blocks/engine.rs::conform` validates supplied JSON against schema independently of requiredness; existing Scope handles omitted evidence. Operator/action/check constructors preserve canonical enum values and canonical planner remains owner. SDK already exports `Spec`, Card identities and authoring, so a missing direct alias is not proof of absent construction. Complete named coverage must exercise usable public paths rather than require a particular export layout.

**Decision-complete correction:** Complete compact canonical parity tables in existing local proof and add/extend public SDK construction proof for the inventory families absent from the story. Include all native ComparisonOperator variants, Source slot/source-reference forms, all three Trigger/Operator binding fields, typed Spec construction for the remaining registrable kinds, and requiredness independent of null-admitting/null-rejecting schemas. For slots, prove authored canonical fields and connect them to existing native input validation/omission semantics, including explicit null acceptance/refusal; reuse the established input boundary/tests rather than add a validator. Preserve default required=true and omitted optional semantics. Only add a concrete missing public projection/helper if these public constructions show it is necessary; do not invent wrappers or duplicate canonical types.

**Burden / alternatives:** A bounded table extension and SDK-facing compile/conversion coverage suffice. Existing engine tests may supply unchanged input-execution evidence when explicitly mapped to authored slot parity; their existence alone is not the missing authored-adapter proof. No per-operator server journey, broad API redesign, new dependencies or schema engine is warranted. Green finance journeys cannot replace this named handoff requirement.

**Focused closure proof:** Run the extended named local primitive/snapshot tests and the narrowly selected public SDK construction test(s). Explicitly map every inventory row to construction and canonical comparison; map required/nullable cases to authored output and existing input-boundary evidence. Do not broaden this to arbitrary field-value exhaustiveness or all SDK journeys.

### FIND-TASK-001-3 — Mandatory changed-item rustdoc remains incomplete or inaccurate

**Status:** CONFIRMED. **Classification:** VIOLATION. **Discovery:** REPO-001, M-002.

**Authority:** root AGENTS.md requires every new/changed Rust item to have rustdoc and blocks merge for missing rustdoc. `agent_docs/rust.md:235–241` expressly includes private fields/helpers/tests and errors, panics, cancellation, partial progress, idempotency and retry. The gate is mandatory even when public `missing_docs`/compiler checks pass.

**Independent locations/evidence:**

- `authoring/tests.rs:12,19,36,172,176,182,453,544,744`: wholly undocumented helpers/tests. `yaml` returns Result and can panic; helpers accepting construction inputs and using expect require accurate Panics docs. The documented `every_primitive`, `policy_judge`, `fact_step`, `release_inputs`, `finance_release` likewise omit applicable Panics descriptions.
- New `From::from` bodies at `authoring/mod.rs:292–323,371–410,477–481` and Prompt's tuple field at `:495` lack item/field rustdoc under the explicit all-item rule.
- SDK Story fields at `integration/authoring.rs:93–94` have no docs; Story methods and panicking construction/read/assertion helpers omit applicable Panics. `verify_both` at `:445` returns fallible results and calls expect, but lacks both Errors/Panics semantics. The four journeys already document Panics and must retain them.
- `cards/handle.rs:324–369` documents graph refusal after publication but omits cancellation/retry behavior of the sequence. Full body directly awaits existing `upload_artifact`, whose preceding docs describe background preparation, uncertain partial publication and fresh-call retry. Earlier successfully published Artifacts survive cancellation. State authored entry points and `pull_declared` at `state.rs:661–750` need accurate applicable contracts too; linking only to path declaration cancellation omits the publication stage.
- `authoring/verifier.rs:142–143,242` incorrectly says explicit dependencies alone order steps. Canonical `VerifierSpec::plan/dependencies` in `validate.rs:100–132` unions explicit edges and action/when references; new local tests themselves assert inferred stages.

**Consequence:** Explicit merge gate fails. Incorrect dependency guidance and incomplete durable partial-progress guidance also mislead SDK readers. This finding does not allege that runtime concurrency or cancellation ownership is broken.

**Decision-complete correction / burden:** One prose pass over new/materially changed Rust items in authoring modules/tests, Cards authored method, state authored/pull helper and SDK authoring tests. Add meaningful missing item/field docs and applicable Errors/Panics; correct inferred scheduling; project/link existing cancellation and retry semantics without promising rollback or inventing behavior. Preserve adjacent source and assertions. No checker, test harness, replacement helper or style-only review round: batch with the behavioral/proof remediation.

**Focused closure proof:** Independently inspect the cumulative changed-item docs including private/test items and complete affected method bodies. Compare planner prose with canonical planner and cancellation prose with unchanged upload/saga/pull ownership. Use relevant existing format/doc lanes; no synthetic behavior test for prose and no assumption that compiler missing-docs establishes private coverage.

## UID reconciliation

Persistence accurately identifies a literal limitation: `upload_artifact` returns a UID-bearing root, but Cards authored sequencing discards that value; authored external selections carry kind/name/version without UID. `Artifact::registered` also keeps identity without the input UID. Do not describe this as sending the returned UID.

This does not establish a new persistence defect. Stronger architecture doctrine 19 and the pre-existing Reference forms/Path resolution contract assign rewrite to UID-bearing durable references to the server before hashing, relationships and persistence. Card identity is tenant/kind/name/version; UID is optional in the authoring reference. Spec INV-002 and REQ-006 require preserving this existing boundary and file/object parity. Loader currently refuses authored UIDs; source confirms the established constraint, but source alone is not the authority for weakening the spec.

REQ-005's “including the resolved UID” is satisfied for the registered containing graph by that server binding; it must not be promoted into a requirement for a second SDK-owned pinning/resolution mechanism. The original task's literal “substituting the returned exact CardRef” is a prescribed dataflow spelling subordinate to these existing reference semantics, not permission to bypass loader validation. The new architecture paragraph and implementation Evidence decision are not themselves evidence of human approval to change a guarantee.

Independent server trace: tenant-bound `select_card_uids_by_ref_batch` resolves exact kind/name/version; `resolve.rs::bind_ref` replaces any external UID with that resolved UID and requires it; `DeclarationWriter::write` rechecks preflight exact Active UIDs under transaction locks before persistence. Parity checks resolved identities/hashes and immutable-version checks pass. There is no supported failing scenario showing different durable attribution from dropping the upload return's UID. Therefore no UID finding, new wire contract or spec-revision blocker is retained. This conclusion does not certify client-to-server transmission of the upload UID, which does not occur.

## Reconciled acceptance and verification

| Task acceptance / constraint | Reconciled outcome and evidence |
|---|---|
| REQ-001, AC-007: all native families/public parity | **FAIL**, FIND-TASK-001-2. Story subset works; explicitly complete proof remains unfinished. |
| REQ-002/008, AC-002: edits/snapshots/equivalence | **FAIL**, FIND-TASK-001-1. Existing child snapshots, duplicate ID/alias/input refusal and real active-definition conflicts otherwise pass local/source evidence. |
| REQ-003/007, AC-002: required independent of nullable | **FAIL for required proof**, FIND-TASK-001-2. No schema conversion defect established; omitted optional story case passes. |
| REQ-004: references/planning/concurrency/dependent outputs | **PASS** runtime/source boundary: delegated planner, explicit-only plus inferred local assertions, fresh concurrency regression and dependent-output journey. Two incorrect rustdoc sentences remain FIND-TASK-001-3. |
| REQ-005/006, AC-003: staged publication, exact references, source isolation, bypass, retained Artifact/version/error preservation | **PASS** for current dependencies via existing owners and fresh Artifact journey. Obsolete captures are the single FIND-TASK-001-1 exception, not a separate saga defect. UID interpretation above applies. |
| REQ-006/008, AC-001: Cards receipt/state hydration and file parity | **PASS** existing adaptation for current dependencies, native tree parity, registered identity/hash parity, exact permission/drift refusals and existing ready-state journey. |
| REQ-007, AC-004: common two-component story and full outcome matrix | **PASS**, fresh component journey plus independently inspected specialist evidence; only both passed permits simulated release. Synthetic provider proves wiring, not model quality. |
| INV-001–005 and non-goals | **PASS** source/specialist coverage: no durable schema, endpoint, dependency/feature, upload/registration/scheduler/transport replacement, auth/tenant or evidence-semantics change. Python/TypeScript projections remain later task work. |
| Mandatory contributor documentation gate | **FAIL**, FIND-TASK-001-3. Other scoped standards coverage passes. |

`verification.md` records freshly passing three local authoring tests, exact existing bounded-concurrency regression, four authored real-server journeys, existing ready-state journey, server build and range whitespace check. Original task records broader format/lint/dependency/docs checks passing; those broader runs were not repeated in this review. Passing checks do not exercise replacement edits or fill the explicit missing inventory/nullability proof, and public lint success cannot waive private/test rustdoc.

Recommend one bounded remediation task containing all three findings. Reuse accepted baseline evidence for unchanged domains in the remediation review; inspect the fix diff, full affected capture owners/converters and their tests, public proof additions, and documentation closure. Do not restart the eight-role team or rerun all journeys unless the fix actually changes their underlying boundary. No specification or expensive persistent/security/concurrency decision is needed for these corrections.

No Cargo, cross-platform or fault-injection runs were performed by this validator. Residual implementation risk is the confirmed stale-capture behavior until corrected; publication/scheduler/security owners remain the existing ones within the reviewed scope.
