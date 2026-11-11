---
id: TASK-001-R4
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 4
parent_task: TASK-001-R3
remediates: [FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10, FIND-TASK-001-11]
---

# Complete canonical admission and the approved Artifact/edit surface

Route directly to `$wyrd-implement`. Correct the independently validated
implementation gaps under approved revision 4; do not reopen the grammar
decision or reimplement registration.

Approved spec: `changes/active/card-authoring/spec.md`.
Original task: `changes/active/card-authoring/tasks/TASK-001-common-rust-authoring.md`.
Current parent: `changes/active/card-authoring/tasks/TASK-001-R3-canonical-authoring-grammar.md`.
Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
R3 base: `a387f2a47a145a867c8a4948966fcbd136af9961`.
Reviewed candidate / remediation starting point:
`37b3c2f96b1bd425bf1f0923a1950d0363c40907`.
Review authority and complete source traces: [findings-validation.md](findings-validation.md).
Prior context and packet hashes: [subject.md](subject.md); verdict: [verdict.md](verdict.md).

Original FIND-TASK-001-1/2/3 are closed. The eight issues below are newly
validated revision-4 gaps, not retrospective findings against the
human-approved API change.

## Outcome and diagnosis

The existing finance journeys work, but the authoring boundary still
distinguishes raw spelling from canonical meaning, erases some malformed input,
omits local Artifact dependencies/admission, and leaves required public
operations unavailable. Restore canonical equality/refusal, complete the
existing Artifact root/reference paths and approved edits, then batch the
finite documentation and assertion repairs.

### FIND-TASK-001-4 — Canonically equal Cards falsely conflict

At `authoring/mod.rs:978–982`, same_content compares raw envelopes before
native decoding. Two same-identity Service children, one with omitted
components and one with components=[], decode to the same ServiceSpec but
raise SpecDuplicateSubmission when composed under different aliases. Shared
Service composition is supported. REQ-008 and AC-001/002 require equal active
definitions to declare once.

Use the existing canonical decoder/submission projection to decide
same-identity content equality, excluding source labels. Retain current
dependency traversal and true active child/parent conflicts, valid snapshot
source handling and invalid-definition diagnostics. Do not write per-kind
default rules, remove conflict detection or choose arbitrary raw content.
The correction is at the existing Graph comparison boundary; a different
normalizer or graph workflow is unnecessary.

### FIND-TASK-001-5 — Malformed raw specs become valid empty Cards

At `authoring/mod.rs:834–838`, CardDefinition::new retains an object spec
but silently discards a string/null/other non-object into default Body. A raw
Service string therefore becomes {} and can declare successfully; the actual
equivalent YAML spec refuses through the canonical decoder. This violates
REQ-002/004 malformed-input preservation and AC-002 before publication.

Preserve the supplied raw spec through staging so the existing shared decoder
sees and refuses it. Keep deliberate empty-object named construction valid
and editable. Do not reject at the constructor, invent a type validator or
add downstream publication guards. The invalid state originates at raw-spec
staging, not the registry.

### FIND-TASK-001-6 — Artifact.file discards the selected reference intent

At `authoring/kinds.rs:268–273`, Artifact.file keeps the captures returned
by Artifact's existing FieldValue conversion but replaces its sibling value
with a plain external CardRef. A source-less Artifact selection in a
standalone Verifier code step captures two Cards without their required
sibling edge, so canonical ordering refuses multiple roots. REQ-008 and
AC-001/007 require the construction/snapshot reference parity independently
of executing selected bytes.

Build the selection from both the value and captures of that existing
conversion. It already chooses the right branch: source-less Card sibling,
local-source external plus publication, registered reference external without
publication. Do not synthesize a byte inventory, fetch remote content or add
automatic uploads for descriptor-only Cards. This correction concerns declared
reference intent, not proof that an arbitrary selected file exists.

### FIND-TASK-001-7 — Local Artifact captures bypass complete graph admission

At Graph::absorb/declaration (`authoring/mod.rs:913–968`) and
Artifact::local (`kinds.rs:280–301`), local Artifacts queue separately
without visiting their current spec captures; only ordinary Cards pass
through the loader's complete tree admission. ArtifactSpec::validate checks
artifact_kind, not unresolved refs or the whole captured graph.

A local payload.schema_ref(local_schema) referenced by an Audit queues only
the payload, then fails remotely because its supplied schema was never
published. A local schema_ref path passes local validation despite the loader
owning its refusal; an earlier valid publication can commit before that
locally detectable error. Conflicting active local/ordinary Artifact identities
also escape the separate identity partitions. These violate REQ-004/008 and
AC-002/007, not server authorization.

Extend existing authored collection/admission to visit current local Artifact
captures dependency-first, reconcile active identity conflicts across both
partitions, and run their canonical descriptors through the shared loader
admission before publication. Preserve external local references and independent
sole requests. Reuse the canonical reference inventory and validation phase;
do not duplicate schema_ref checks or force independent external local roots
into a fabricated composite root. The demonstrated nested local-to-local case
uses existing Artifact uploads in dependency order. No new arbitrary
heavy-to-light publication contract or Model/Data/Experiment uploader is required.

### FIND-TASK-001-8 — Named Artifact root adaptation is absent

Artifact at `kinds.rs:222–339` lacks the root conversions supplied for other
named Cards; direct Cards.declare(&artifact) cannot compile. The public
inventory reconstructs a generic CardDefinition from artifact.spec(),
masking the missing named path and dropping metadata/source/extract/captures.
Also, local Artifact.validate uses reference-position lowering, produces no
ordinary root and fails the existing loader's empty-tree check. REQ-001/004/006
and AC-007/008 explicitly promise these ordinary named-Card operations.

Complete root admission with the Artifact's full current metadata, captures
and source/extract state, keeping reference-position lowering distinct.
Validate a real Artifact root through existing canonical admission without a
fabricated parent. Source-bearing root declaration must use the existing sole
Artifact preparation/saga and return its actual ordinary Artifact receipt,
not a second empty-graph submission or synthesized outcomes.

The public upload_artifact contract still returns its CardRef; its underlying
saga already produces the receipt. Reuse/factor that existing owner as needed,
preserving source/scratch lifetime and errors, rather than add another upload
workflow. Complete the shared argument adapters; preserve existing state
hydration/root eligibility and its receipt-to-pull tail, without inventing
unrelated new state-loading capabilities.

### FIND-TASK-001-9 — Four required field replacements are missing

The Action/Check bodies at `authoring/verifier.rs:245–419` have no public
replacement for Action.from, Action.agent, Check.each or Check.agent, despite
the approved tables and REQ-001/002/AC-007. Constructors are exercised; edits
are not. Rebuilding the parent cannot supply the promised field operation
while retaining sibling inputs/workspace/operator data and current captures.

Add Action.from through the existing Body field owner. For the other three,
preserve the exact approved constructor and setter call grammar while
separating associated-constructor lookup from the editable receiver where
names collide. Reuse Body and the existing canonical conversion boundary to
Step/Requirement/FieldValue. The editable result retains its discriminant,
other fields and approved methods, with owned setters and mutable additions.

This selects the correction boundary, not concrete type names or generic
structure: representation is explicitly implementation-owned in revision 4.
Simply adding same-named inherent methods to current Self-returning constructor
structs cannot compile. Renaming approved calls, requiring extension-trait
imports, removing setters or adding a second variant grammar does not close
the finding. Any representation must preserve the approved calls and direct
Step/Requirement use; no new public grammar decision is authorized.

The additional representation work is justified only by the explicit required
edits, not a reason to build a speculative hierarchy or duplicate contracts.

### FIND-TASK-001-10 — Finite new mandatory documentation is incomplete

The original five wrapper panic contracts are closed. New gaps in rewritten
items remain under the explicit changed-item rustdoc gate:

- `examples/finance-release/finance_release.rs:23–24` main lacks item
  documentation and its actual fallible-workflow Errors contract.
- `authoring/kinds.rs:126,139,154` Model/Data/Experiment public type docs
  restate only their names.
- `cards/handle.rs:315–320` private durable submit omits cancellation,
  partial-progress and fresh-call retry guidance from its existing saga.

Add concise actual workflow/domain/Errors/cancellation documentation, linking
the existing owners where appropriate. Preserve bodies, signatures and
assertions. Batch these mandatory corrections with the functional changes;
no checker, test harness or wider style audit.

### FIND-TASK-001-11 — Source-IO proof accepts unrelated error families

The deferred-source assertion at `authoring/tests.rs:573–578` accepts
WYRD_LOADER_500_IO or any code containing IO. The real diagnostic is
**WYRD_LOADER_400_IO**, produced by Diagnostic::io. PERMISSION and
VERIFICATION family strings can satisfy the substring predicate. This violates
the exact catalog-code testing rule and weakens the required negative proof.

Assert equality to the actual WYRD_LOADER_400_IO diagnostic at that existing
point. Retain the scenario's read-timing and fresh-snapshot assertions.
Production mapping is correct; no production error change or new test is needed.

## Preserved behavior and scope

Keep all revision-4 approved constructors, YAML names/value forms, infallible
edits, complete-snapshot validation, by-value capture/replacement, duplicate-add
refusal, canonical types and code/file diagnostics. Keep true active conflicts,
external/sibling distinctions and existing sole-publication/source-map/scratch
ownership, registry errors/versions, same-client hydration and cancellation.

No wire/schema/endpoint, auth/tenant/permission/audit, migration, dependency,
feature, rollback, scheduler or foreign-language implementation changes.
Keep the runnable finance example's one writer, Service-level three requirements
and exact output; its behavior already passed. Do not reopen prior closed
findings, rename unrelated runtime APIs or repeat an unrelated codebase audit.

## Focused closure proof

| Finding | Required observation |
|---|---|
| 4 | Canonical-equal omitted/explicit-empty same-identity child definitions yield one submission; genuinely differing canonical content still refuses. |
| 5 | Raw string/null specs refuse through the same canonical family/path as equivalent envelopes; {} and named editable construction remain valid. |
| 6 | Descriptor selection yields the sibling edge and valid standalone Verifier snapshot; local/registered selections retain their external/publication distinctions. No execution of absent byte inventory is required. |
| 7 | Local schema -> payload -> Audit plans both sources dependency-first once; replacement drops obsolete schema capture; unresolved path and cross-partition conflict refuse before IO/publication. One focused existing Artifact journey establishes nested registered references. |
| 8 | Public proof passes owned/borrowed Artifact roots without JSON reconstruction, preserving labels/annotations/source/extract; valid local validate avoids empty-tree error; existing Artifact journey checks a source-bearing root receipt/files through the existing publication owner. |
| 9 | Public proof edits all four fields, preserves discriminants/sibling fields, replaces the Agent capture without dropping a live workspace Artifact, and keeps existing constructor-to-Step/Requirement and mutable-add calls valid. |
| 10 | Exact cited item contracts and any newly changed items have meaningful applicable rustdoc; no runtime change is made for prose. |
| 11 | Existing deferred-source test asserts exactly WYRD_LOADER_400_IO. |

Use focused regressions in existing local/public tests before each functional
correction; record genuine RED/GREEN and exact source cause. Add only enough
surface to execute a failing assertion where new call forms otherwise do not
compile. Historical mutation sensitivity is not a claim that a pre-fix
scenario ran. Documentation/literal-proof repairs need no manufactured RED.

Extend the existing tests rather than add a harness or test binary. Exact
existing local/public selectors, each named above through its role:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::offline_edits_preserve_snapshots) | test(=authoring::tests::replacements_keep_only_current_dependencies) | test(=authoring::tests::field_edits_defer_validation_and_preserve_snapshots) | test(=authoring::tests::yaml_and_code_share_diagnostics) | test(=authoring::tests::core_primitives_match_canonical_contracts) | test(=authoring::tests::declarations_match_file_authored_tree)'
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'
```

If a new test is warranted, record its exact selector before running RED.
Do not replace the canonical comparisons or weaken old assertions.

For only the materially changed Artifact root/nested-publication paths, extend
the existing focused Artifact test and reuse its server/fixtures. Do not rerun
the full SDK journey suite or aggregates:

```bash
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::local_artifacts_reuse_and_keep_versions)'"
```

Run the narrow write-set format/lint/docs checks, sequentially:

```bash
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -- -D warnings
mise exec -- cargo doc --locked -p wyrd-client -p wyrd-loader -p wyrd-ai --no-deps
mise exec -- git diff --check
```

Carry unrelated passing executor/security/component/finance journeys from
this review forward unless the actual fix changes their boundary. Full
journeys and broad gates run at change review, not as this remediation's
checklist. Record the immutable candidate and focused evidence for review of
these eight closures, the actual new diff and affected consumers.

## Stop conditions

Stop for a genuine requirement to change approved public calls, reference
intent, sole-publication/receipt/identity semantics or any excluded material
boundary. Ordinary staging, normalization factoring, editable-return
representation and existing-owner receipt preservation are implementation-owned.
Do not introduce new machinery merely to avoid extending the inspected owners.

## Implementation evidence

Status: **IMPLEMENTED**. Starting point `37b3c2f96`; candidate `9be515a10`
(commits `42b3a7e58`, `9be515a10`).

Owner/reuse: the existing `Graph` comparison and collection owner, the
loader's `parse_card_value`/`declaration_input_from_cards` admission, the
`card_kind!` root adapters, `Body`/`fields!`, and the existing
`ArtifactUpload` + declaration saga. Gap closed by moving a local Artifact's
source onto `Card`, so one capture list carries every Card: local Artifacts
are traversed, compared, and admitted on the same path as ordinary Cards and
partitioned only when the declaration is built. No new workflow, normalizer,
validator, or upload lifecycle. `Declaration.input` is `None` when the root
is itself a local Artifact; `Cards::declare` then returns that publication's
receipt through the private `publish_artifact`, which `upload_artifact`
delegates to (its public `CardRef` contract and scratch lifetime unchanged).
Receivers: `Action::agent` → `AgentAction`, `Check::each` → `EachCheck`,
`Check::agent` → `AgentCheck`, each with the approved table's setters and
`From` into `Action`/`Check`/`FieldValue`; call grammar unchanged. One
consequence: a homogeneous array mixing `Check::each(..)` with other
constructors needs `.into()`.

RED: fixes were written before their regressions, so RED was established by
reverting each fix singly against the new assertions (all failed with the
reviewed symptom, then sources restored): M4 raw comparison →
`SpecDuplicateSubmission` for `shared-child`; M5 dropped raw spec → declared
`spec: {}`; M6 card-ref selection → "multiple roots detected"; M7a no local
traversal → wrong publication list; M7b no local admission → unresolved path
admitted; M7c source ignored in comparison → cross-partition conflict
admitted; M8 always build graph → "Cannot order an empty card tree". FIND-8
public adapters and FIND-9 setters were compile-RED (calls did not exist).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-4 canonical equality; real difference refuses | `authoring/mod.rs` `same_content` (decoded metadata+spec, source/extract) | `offline_edits_preserve_snapshots` (omitted vs `[]` one submission; `description` differs refuses); M4 RED | PASS |
| FIND-5 raw string/null refuse alike; `{}` valid | `Body::malformed`, `CardDefinition::new` | `yaml_and_code_share_diagnostics` (YAML vs code diagnostics equal; `{}` declares); M5 RED | PASS |
| FIND-6 descriptor selection is sibling; local/registered external | `kinds.rs` `Artifact::file` uses the reference's own value | `core_primitives_match_canonical_contracts` (Artifact+Verifier, sibling source; local 1 publish, registered 0); M6 RED | PASS |
| FIND-7 nested local dependency-first once; replacement drops; unresolved path and cross-partition conflict refuse before IO | `Graph::absorb` single list, `Card::local` runs loader admission | `replacements_keep_only_current_dependencies`, `offline_edits_preserve_snapshots`; M7a/b/c RED; journey nested schema→payload→Audit registers payload referencing schema UID | PASS |
| FIND-8 named Artifact roots; local validate; source-bearing root receipt | `card_kind!(Artifact)`, `Declaration.input: Option`, `Cards::declare`/`publish_artifact` | `core_primitives…` (local root validates, labels/source/extract kept, blank kind refuses); `public_paths_author_every_native_family` (owned/borrowed `Declarable`, `CardDefinition::from(&artifact)` keeps labels/annotations); `local_artifacts_reuse_and_keep_versions` (root receipt names published Artifact with UID, files listed); M8 RED | PASS |
| FIND-9 four edits keep discriminant/sibling fields; Agent capture replaced, workspace Artifact kept | `verifier.rs` `Action.from`, `AgentAction`, `EachCheck`, `AgentCheck` | `public_paths_author_every_native_family` (edited == direct; only `new-judge` declared; mounted policy still published; constructors still feed `Step`/`Requirement`) | PASS |
| FIND-10 docs | finance `main` workflow+`# Errors`; Model/Data/Experiment domain docs; `submit` `# Cancellation`; new items documented | `cargo doc` warning-free; clippy `-D warnings` | PASS |
| FIND-11 exact `WYRD_LOADER_400_IO` | `tests.rs` deferred-source assertion | `field_edits_defer_validation_and_preserve_snapshots` | PASS |

Commands (all passed, run sequentially):

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E '<the six task selectors>'   # 6 passed
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'   # 1 passed
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::local_artifacts_reuse_and_keep_versions)'"   # migrations 2 passed; journey 1 passed
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -- -D warnings
mise exec -- cargo doc --locked -p wyrd-client -p wyrd-loader -p wyrd-ai --no-deps
mise run docs:check
mise exec -- git diff --check
```

Non-goals held: no wire/schema/endpoint, auth/tenant/audit, migration,
dependency, feature, scheduler, or foreign-language change; finance example
behavior unchanged (doc only). `architecture/wyrd-design.md` updated for
canonical comparison and local-root declaration. Carried forward unchanged:
executor/security/component/finance journeys. Linker `__eh_frame` warning is
the known cached-link message and fails nothing.
