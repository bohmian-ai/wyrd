---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 3
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3]
---

# Keep authored dependencies aligned with current values and complete the required proof

Route directly to `$wyrd-implement`. Implement the bounded corrections below under approved revision 3; no new product or architecture decision is required.

Approved spec: `changes/active/card-authoring/spec.md`.
Original task: `changes/active/card-authoring/tasks/TASK-001-common-rust-authoring.md`.
Review base: `5234961d51798581a026834cfffd5efaadc226b4`.
Reviewed candidate / remediation starting point: `790da3369c1b0107765db234d72c1ae0c4680446`.
Validated diagnosis: `findings-validation.md` beside this task. Prior verdict: `verdict.md`.
Packet hashes and complete review subject: `subject.md`.

## Current problem and intended outcome

Replacing a Trigger or workspace value currently changes the canonical field while retaining the old captured dependency. A developer who replaces a local policy with an input can still upload that old policy, or fail because its obsolete local path is missing. The final definition must instead declare and publish exactly the dependencies of its current values. Equivalent direct construction and incremental edits must produce equivalent declarations.

The implementation also lacks the explicitly required full native/public construction and required/nullable proof, and fails the repository's changed-item rustdoc gate. Complete these together with the dependency correction. This task does not redesign registration, upload, validation, execution or the SDK surface.

## Diagnosis and selected correction

### FIND-TASK-001-1 — Current slots retain historical dependency captures

REQ-002/REQ-008, AC-002 and original Scenario 1 require editable receiver values and equivalent constructor/incremental results. `Binding::set_runs_on` at `crates/shared/wyrd-client/src/authoring/mod.rs:450` overwrites the canonical target but appends its dependencies. `AgentCall::add_workspace_input` and `add_workspace_file` at `authoring/verifier.rs:537,545` overwrite a destination while retaining the old local Artifact. The shared `Dependencies::absorb` only appends.

These captures move through Action/Check, Step/Requirement, Verifier, Agent/Service and into `Graph::add` (`mod.rs:253`). Graph consumes all captures, including obsolete ones. Two successive same-identity Trigger definitions with different specs therefore cause a false `SpecDuplicateSubmission`. A removed workspace Artifact remains in `Declaration.artifacts`, which `Cards::declare_authored` (`cards/handle.rs:354–367`) uploads before declaring the graph. WyrdState uses the same path. Existing green tests cover additions and snapshots, not these replacements.

Correct dependency ownership at the existing Binding and AgentCall replacement positions: replacing a canonical slot must replace that slot's captured contribution before it is transferred into a parent. Preserve nested captures, the Agent target's dependencies, other binding fields and workspace destinations, and any Artifact still mounted elsewhere. Retain current replacement semantics. Private representation remains implementation-owned.

This bounded producer correction closes both the false conflict and obsolete-upload paths. Deleting setters or rejecting replacement loses required editing behavior. Silently choosing the last identity weakens real active-conflict detection. A downstream upload guard cannot repair obsolete sibling submissions or local false conflicts. Do not add another graph traversal/validator, cache, upload workflow or rollback mechanism.

### FIND-TASK-001-2 — Explicit primitive/public and required/nullable proof is incomplete

AC-007 and the original inventory require complete native family/variant parity and usable public Rust coverage before Python/TypeScript handoff. The slot obligations additionally require proving requiredness independently of nullability at the authoring/input boundary.

The full local test at `authoring/tests.rs:182` uses only Equals, omits Source/source-reference examples, never exercises the three Trigger/Operator binding fields, and has no typed Spec table for the remaining registrable kinds. The tests do not distinguish required/optional from null-admitting/null-rejecting schema cases. SDK journeys construct only the story subset; local proof imports canonical crates directly. These are named acceptance gaps, rather than proof that a public type is impossible to construct. No missing-export defect was established.

Extend existing compact canonical parity proof and SDK-facing construction proof to cover the missing named inventory: all native ComparisonOperator values; Source input/source-reference forms; `runs_on`, `on_failure`, `on_verdict_change`; remaining registrable kinds through canonical typed Specs; and required/optional × accepting/rejecting explicit-null schemas. Map each inventory row to construction and canonical comparison. Use public SDK paths for the public proof; inference/defaults/existing Spec access are valid where they provide usable construction.

For slots, compare authored canonical output and explicitly connect it to the existing native input validation/omission behavior. Existing engine admission/Scope tests may supply unchanged execution evidence when mapped to authored parity. Preserve default required=true, optional omission, and independent schema treatment of supplied null. Reuse the canonical validator; do not implement another schema engine. Only add a concrete missing projection/helper if this proof establishes it is necessary. No blanket exports, speculative wrappers, per-operator server journeys or new dependencies are warranted.

### FIND-TASK-001-3 — Mandatory changed-item rustdoc is missing or inaccurate

Root AGENTS and `agent_docs/rust.md` require all new/changed Rust items, including private helpers, tests and fields, to have meaningful rustdoc and applicable Errors/Panics and asynchronous partial-progress contracts. Compiler/public missing-doc checks do not establish that gate.

The validated locations include undocumented helpers/tests at `authoring/tests.rs:12,19,36,172,176,182,453,544,744`; applicable panic sections on documented construction helpers; new conversion methods and Prompt's tuple field in `authoring/mod.rs`; and Story fields/methods plus construction/read/assertion helpers in `sdks/wyrd-sdk-rust/tests/integration/authoring.rs`. `verify_both` also lacks its applicable error contract. Consult the validation ledger for the complete cited set.

The authored Cards method and state methods/pull helper need accurate cancellation, partial-publication and retry guidance using the existing upload/saga/pull owners. Already published Artifacts can survive a dropped future; uncertain current publication follows existing behavior. Do not promise rollback or transparent resumption. `Verifier::plan` and `Step::add_depends_on` incorrectly say only explicit dependencies order steps, although the canonical planner also infers edges from action/when references.

Make one bounded prose pass over these new/materially changed items, including items changed by this remediation. Document intent and invariants, missing fields and applicable Errors/Panics; correct scheduling language and project/link existing cancellation behavior. Preserve actual runtime semantics and assertions. No new checker, helper, synthetic prose test or separate style-only round is needed.

## Preserved boundaries and non-goals

- Keep existing canonical types, snapshots, sibling/external intent, shared loader validation/order, equal-definition deduplication and genuine active-definition conflict refusal.
- Keep Cards upload/declare and state pull/hydrate owners, per-Artifact source maps, scratch lifetime, sole-submission uploads, retained publication, errors, permissions/tenancy, exact identity/hash attribution and existing scheduling.
- No wire, endpoint, dependency/feature, schema, retry/lifecycle, durable-state, permission, concurrency or rollback changes. No Python/TypeScript implementation or unrelated cleanup.
- Identity-only authored Artifact references remain under the reviewed server-owned UID binding contract. No UID finding or remediation is retained.

## Acceptance and focused proof

| Finding | Required closure observation |
|---|---|
| FIND-TASK-001-1 | Direct and replacement-edited same-identity Trigger declarations match and retain only the current Trigger. Workspace file→Scope and file→other-file retain only current publication descriptors. Replacing one of two destinations sharing an Artifact preserves the other. Genuine active conflicting captures still refuse. |
| FIND-TASK-001-2 | Every required inventory row has native construction/canonical parity evidence and usable public SDK construction coverage. All missing named families and required/optional versus explicit-null cases above are mapped to precise proof. No full-surface claim rests solely on the finance story. |
| FIND-TASK-001-3 | Independent inspection of cumulative changed items finds meaningful docs, applicable errors/panics and correct partial-progress/scheduling contracts, including private/test items that compiler checks do not cover. |

For dependency logic, add focused local regressions before the producer correction and record expected RED, GREEN and regression results under the existing implementation workflow. No live journey is required for stale-upload closure: an obsolete missing source with no publication descriptor directly closes the cause because the unchanged Cards loop consumes exactly those descriptors. Public construction coverage can be a small test in the existing SDK target; do not add a test binary or fixture harness.

Commands for the existing named local proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::offline_edits_preserve_snapshots)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::core_primitives_match_canonical_contracts)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::declarations_match_file_authored_tree)'
```

Run the exact selector for each newly named test and record it in completion evidence. Run only the affected public SDK construction test(s), not the full integration journeys. Confirm existing mise task definitions before choosing narrow format/lint/doc commands. Appropriate focused checks for the anticipated client/SDK write set are:

```bash
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-ai --all-targets -- -D warnings
mise exec -- cargo doc --locked -p wyrd-client -p wyrd-ai --no-deps
mise exec -- git diff --check
```

Broaden only if the actual correction changes another owner or exposes a relevant failure. Run Cargo sequentially. Do not rerun full user journeys or broad aggregates for this remediation; they run once at change review. Preserve the existing passing source-backed baseline evidence in this review directory and record the remediation candidate for focused review of closure, fix diff and affected consumers.

## Evidence

Remediation candidate: `790da3369..c0a06eacd` (`3cd30ab24` FIND-1, `4d3fc8c0a` FIND-2, `c0a06eacd` FIND-3).

### FIND-TASK-001-1 workflow

- RED: `replacements_keep_only_current_dependencies` failed at the first assertion. The edited Binding kept both `daily` 1.0.0 Trigger definitions, and the direct Binding kept one.
- GREEN: `Binding` holds the current Trigger's capture in its own `trigger` field, and `set_runs_on` replaces it. `AgentCall` holds one capture per workspace path: `add_workspace_file` inserts it and `add_workspace_input` removes it. Both are merged into the parent through a private `into_parts` at the existing attachment points (`Agent`/`Service::add_verifier`, `compose`, `Action::agent`, `Check::agent`). Graph, loader, Cards and state are unchanged. The `binding` field was renamed `canonical` for `clippy::struct_field_names`.
- Regression: with the `workspace.remove` line deleted, the test fails again. All four authoring tests pass.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Direct and edited same-identity Trigger declarations match and keep only the current Trigger; a registered replacement drops the sibling | `authoring/mod.rs` `Binding::{set_runs_on,into_parts}` | `authoring::tests::replacements_keep_only_current_dependencies` | PASS |
| file→Scope and file→other file keep only the current publication descriptors; an obsolete missing source is not published | `authoring/verifier.rs` `AgentCall::{add_workspace_input,add_workspace_file,into_parts}` | same test (`missing/obsolete.pdf` gives `artifacts == []` / `[policy]`) | PASS |
| Replacing one of two destinations that share an Artifact keeps the Artifact | per-path `workspace` map | same test | PASS |
| Two current conflicting definitions still refuse | Graph unchanged | same test (`SpecDuplicateSubmission`), plus `offline_edits_preserve_snapshots` | PASS |

### FIND-TASK-001-2 inventory map

Public proof: `sdks/wyrd-sdk-rust/tests/integration/authoring.rs::public_paths_author_every_native_family`. It uses `wyrd_ai::` paths only, needs no server, and compares each authored value with the `wyrd_ai::Spec::from_kind_and_value` decode of its canonical form. Native proof: the existing `core_primitives_match_canonical_contracts` and `declarations_match_file_authored_tree`.

| Inventory row | Construction and canonical comparison | Result |
|---|---|---|
| Data, Model, Experiment, Policy, Mcp, Audit, Source, Trigger, Operator, Prompt | `CardDefinition::new` over the typed Spec; each declaration is one submission of its kind | PASS |
| Prompt Card, Artifact spec | `Prompt::to_card`, `Artifact::set_spec` | PASS |
| Agent, Verifier, Service | public test (Agent with bindings), plus native `core_primitives…` and file parity | PASS |
| All 56 ComparisonOperator variants | each through `Check::assert`, `Check::each` and `Check::trace`; the count is asserted at 56; `Verifier::validate` accepts all; non-Equals `set_when` is native | PASS |
| Actions extract/agent/code (inline, artifact, runtime) and Checks agent/code/code_with_format/drift | public test | PASS |
| Slots Json/Media/Trace/Samples/Source; source input and source outcome references; `order_by` | group Verifier copied from the `conversation-verifier` fixture; drift Verifier | PASS |
| `runs_on`, `on_failure`, `on_verdict_change` in sibling, inline and registered forms | Binding on an Agent; the declaration includes Trigger and Operator submissions | PASS |
| Required/optional × null-admitting/null-rejecting | authored canonical slots plus `InputSlot::required()` table (public test) | PASS |
| Supplied `null` follows the schema, not requiredness; optional omission | `vala-verify blocks::engine::tests::supplied_null_follows_the_slot_schema_not_requiredness` (new) and the existing `missing_and_optional_evidence_resolve_honestly` | PASS |

Mutations: sending `on_verdict_change` to `on_failure`, or `Check::each` to `path`, each fails the public test. No export, wrapper or helper was added:
- Source slots, `NamedResponseFormat`, and Trigger/Operator specs are built through existing `Deserialize`/`Spec` access.
- `SourceText` has no public constructor, so a Source slot is built as `serde_json::from_value(json!("verifiers.…"))`. This is usable but untyped. Revisit if the Python/TypeScript wrappers need a typed path.

### FIND-TASK-001-3

Docs and contracts added:
- Every new or changed item now has docs and the applicable `# Panics`/`# Errors`:
  - authoring `From` impls, the `Prompt` field, and `mod tests`;
  - every local test and helper;
  - SDK `Story` fields and methods, and the panicking helpers;
  - `verify_both` Errors/Panics.
- `Cards::declare_authored` gains a Cancellation section: no rollback, published Artifacts stay registered, interrupted uploads behave as `upload_artifact` describes, and a retry is a new call under the version rules.
- `WyrdState::declare_authored{,_with}` gain Cancellation sections.
- `pull_declared` gains Cancellation: the temp directory is removed, a `bundle_dir` keeps its prior bundle, and staging may remain.
- `Verifier::plan`, `Step::add_depends_on` and the `Step` docs now say references to a step's output or status order steps too. This matches `VerifierSpec::plan`.

Doc repairs outside the authoring files:
- The prescribed `cargo doc -p wyrd-client -p wyrd-ai --no-deps` failed on three default-feature links: `Observe::drift` no longer exists, and `Bifrost::writer_table` and `Observe::stage` are `internal`-only. All three are repaired.
- Four private-item link warnings in `transport/trace.rs` were unlinked.

### Commands

| Command | Result |
|---|---|
| `cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::offline_edits_preserve_snapshots)'` (and `core_primitives_match_canonical_contracts`, `declarations_match_file_authored_tree`, `replacements_keep_only_current_dependencies`) | PASS |
| `cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'` | PASS |
| `cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::engine::tests::supplied_null_follows_the_slot_schema_not_requiredness)'` | PASS |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked -p wyrd-client -p wyrd-ai -p vala-verify --all-targets -- -D warnings` | PASS |
| `cargo doc --locked -p wyrd-client -p wyrd-ai --no-deps` | PASS, 0 warnings |
| `mise run check:docs` | PASS |
| `git diff --check 790da3369..HEAD` | PASS |

Non-goals were held:
- No wire, endpoint, dependency, feature, schema, upload, registration, loader, Graph, retry or lifecycle change.
- No Python or TypeScript work.
- Full journeys were not rerun.

The diff touches the authoring module, the Cards/state docs, three client doc-link fixes, one vala-verify engine test and the SDK authoring test file.
