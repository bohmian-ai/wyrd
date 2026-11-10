# Remediation discovery — TASK-001-R5

Result: **PASS**. Proposed material finding ledger: **empty**.

## Subject, scope and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; original base
`5234961d51798581a026834cfffd5efaadc226b4`; immediate base
`9be515a10607ba7d930594b1c314757ee05a0424`; candidate
`c24f5bae7d720d8a64f01d8127ec00c91367f48e`. Inspected the complete immediate
five-file diff. HEAD remained the candidate and tracked source was clean.
No CodeGraph index exists, as recorded in subject.md. This reviewer changed
only this report and ran no Cargo commands.

This is fresh remediation discovery, separate from implementation and final
validation. The scope is the revision-5 constructor-only correction, deleted
receiver exports/conversions, remaining builders, public-test rewrite,
rustdoc and affected consumers. It does not restart original-task discovery.
Read subject.md, the task-review skill, approved revision-5 support tables,
original task and R5 task, applicable root contributor rules,
changes/spec-driven-development.md, agent_docs/index.md, rust.md and
maintainer-style.md, architecture/README.md and the owning In-memory authoring
section. Implementation evidence is a claim, not the basis for acceptance.

Carry unchanged acceptance and FIND-1–8/10–11 closure from review-04
findings-validation.md and its task-review-invariants.md and
domain-review-identity-publication.md, which retain review-03 coverage.
FIND-9's duplicate-constructor-field target is superseded by revision 5;
restoring its setters would violate the present obligation. The changed
constructors are rechecked below rather than relying on that former target.

## Acceptance matrix

`mod.rs`, `kinds.rs`, `verifier.rs`, and `tests.rs` below are beneath
`crates/shared/wyrd-client/src/authoring/`; `integration/authoring.rs` is
the Rust SDK's existing integration target.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 / AC-007/008: named Card name/version are constructor-only; CardRef accessors survive | `card_methods!`, mod.rs:749 onward, deletes only the two identity setters and retains labels/annotations, reference/spec access and validation. `CardRef::name/version`, mod.rs:482/488, remain read-only. | Complete macro diff and authoring source inspection; retained metadata and identity assertions in public inventory. | PASS |
| Component alias/ref and Binding verifier are constructor-only | kinds.rs:292–337 retains constructors writing these fields through Body and removes only the corresponding fields! entries. Component source/config/credentials/verified_by and Binding activation/operator edits remain. | Constructor-to-FieldValue inspection; public Component/Binding canonical and captured dependency assertions remain at integration/authoring.rs:1729–1811. | PASS |
| Slot constructor schema/formats/features/source have no duplicate edits; retain supported non-fixed fields | verifier.rs:115–173 removes schema/formats/features and add_format/add_feature. json directly uses Body::set; media/samples share private listing using the existing Body::set_list; source and trace construction are unchanged. required/max_bytes methods remain. | Existing canonical json/media/trace/samples/source and optionality proof remains in public inventory and core_primitives_match_canonical_contracts; six client tests are unchanged. | PASS |
| Step id/action and Requirement id/statement/check are constructor-only | verifier.rs:192–240 retains Body::new constructor assembly and deletes fixed-field setters; Step depends_on/when/requirements/timeout/retries and incremental methods remain. | Direct Step/Requirement use throughout retained local tests, public inventory, finance example and journey; exact constructor bodies inspected. | PASS |
| All Action/Check constructor operands are fixed, with ordinary return types and no receiver indirection | verifier.rs:258–399: Action agent/extract/code and Check assert/each/trace/agent/code/drift all return Self and put fixed arguments directly in Body; drift retains flatten. All operand setter entries disappear. AgentAction/AgentCheck/EachCheck and their conversions disappear; mod.rs:49 reexports only native authoring owners. | Explicit `Action`/`Check` assignments at integration/authoring.rs:1550–1555 and mixed Check array without each conversion at :1598–1604; source search finds no removed receiver in Rust client/SDK/examples. | PASS |
| Retain remaining builders and full native-family/operator/slot coverage | Remaining Action/Check inputs/workspace/runtime/response_format fields use unchanged fields! expansion. Verifier, Service and all other Card spec methods are unchanged. Body and fields! remain the single editing/capture owners. | Actual test diff preserves all 56-operator assert/each/trace iterations, other Action/Check variants, drift, slots, named kinds and generic CardDefinition proof. Service verified_by replacement assertion remains :1786–1811; mutable adds remain. | PASS |
| Preserve capture and current-dependency semantics; replace obsolete edit proof without weakening independent replacement proof | Action/Check From<FieldValue> consumers transfer the same Body as deleted wrappers did. Step/Requirement constructors still consume Action/Check through existing conversions. Rewritten composed graph :1813–1859 uses direct constructors and retains discriminant, one shared Agent and one shared mounted Artifact assertions. | replacements_keep_only_current_dependencies remains unchanged: runs_on replacement removes obsolete Trigger; components replacement removes old Agent; workspace replacement removes obsolete Artifact, retains another live mount and refuses live conflicts. Six tests' entire file is unchanged. | PASS |
| Preserve deferred validation/IO, malformed raw input, canonical equality and file/code parity | No Body, FieldValue, Card decode, Graph, loader or file owner changes. New listing performs precisely the former list-setter conversion and no validation/IO; json performs the former one-setter operation. | Existing six authoring tests retain snapshots, duplicate/identity refusals, raw malformed input parity, source-read timing and exact WYRD_LOADER_400_IO assertion at tests.rs:714–722. Carry prior independently validated closure; current execution belongs to orchestrator. | PASS |
| Preserve R4 Artifact intent/admission/order/root receipts and publication reuse | No change to Artifact selections, local capture/admission, Graph identity/order, Cards publication/receipt, loader, saga or state. Changed Action/Check producers retain FieldValue captures rather than reconstructing JSON. | Carry review-04 invariant/domain and validation rows for FIND-6/7/8. Existing real-server local_artifacts_reuse_and_keep_versions proof is unchanged, including root receipt and nested schema UID edge. | PASS |
| Documentation and public consumers describe the corrected surface; mandatory changed-item rustdoc | Architecture wyrd-design.md:2321–2336 states constructor-only values and ordinary returns. card_methods! docs now omit identity setters. Changed json/agent/each methods and private listing have applicable intent/operand docs; existing public-test rustdoc accurately describes retained canonical inventory and checks. Rust SDK reexports the authoring module; no separate removed-type export survives. | Inspected SDK reexports, finance-release constructor call sites, packet language sketches and Rust searches. No affected obsolete call remains in inspected consumers; compiler/docs/lint results must be reconciled by orchestrator. | PASS |
| Original task runtime/security/state/component behavior and non-goals remain satisfied at unchanged owners | Five-file diff adds no server, wire, registry, upload, execution, SQL, credentials/tenancy, transport, feature/dependency or foreign-language implementation. Revision-5 intentional API removal is authorized; no compatibility surface or substitute grammar is added. | Carry review-04 findings-validation and its referenced review-03 evidence for original REQ-001–008, AC-001–004/007/008 and INV-001–005; no affected-owner evidence invalidates it. | PASS |

## Source trace and correction burden

The changed production path is constructor arguments → existing Body →
FieldValue → Step/Requirement or Verifier → existing declaration graph.
Previously the three receivers merely stored a Body and converted that Body
into Action/Check. The candidate writes the same Body directly into those
owners. It neither adds a second authority nor drops reference/file captures.
All ordinary constructors and their sibling consumers use the same conversion
path. The private listing helper has two real callers and reuses set_list;
its operation is identical to the deleted formats/features setters. No new
validation or configurable workflow is introduced.

Source search covered removed type names, removed slot additions and likely
obsolete fixed-field calls in client authoring, Rust SDK, finance example and
packet sketches. The Data schema setter is intentionally retained because
Data::new fixes only identity. CardRef name/version are intentionally retained
as readers. The remaining method set may still admit a field on the wrong
canonical variant and refuse it during shared validation, as prior accepted
deferred-validation behavior requires; this removal does not introduce another
variant hierarchy or require eager checking.

The public test rewrite removes only revision-4 operand replacement assertions
whose public methods are now prohibited. It keeps their relevant composition
outcomes with direct construction, while independent non-fixed replacement
tests continue to detect stale captures. Thus this is an authorized acceptance
change, not a test relaxation hiding a failure. No timeout, retry, skip,
assertion-status relaxation or gate bypass enters the diff. The compiler RED
claim is credible as a return-type mismatch and must be reconciled against
orchestrator evidence; this review does not claim to have reproduced it.

No supported-use regression, missing revision-5 behavior, mandatory rustdoc
gap or unearned machinery was established. No new finding ID is proposed.
The repeated review scope converges here on the human-directed removal;
there is no basis for restoring rejected setters or reopening unchanged
Artifact/security/execution acceptance.

## Coverage and limits

All five changed files and complete changed production bodies were inspected,
along with generated-method templates, Body set/list/FieldValue conversions,
Step/Requirement consumers, retained local replacement/deferred-IO tests,
public inventory, SDK reexport and finance constructor consumers. Prior
source-backed invariant/domain coverage is carried only at unchanged owners.
No fresh sensitive-domain, lifecycle, system or broad repository specialist
is warranted by this bounded deletion; final independent validation remains
required by the orchestrator.

Skipped Cargo, broad journeys, cross-platform execution and unrelated source
rediscovery as assigned. Runtime verification is supplied separately by the
orchestrator; no additional material risk was found in this review scope.

Before report completion, the orchestrator supplied fresh successful results
for all six exact client authoring tests (run
`a7bb4f70-5698-44bf-86cc-52f7dc97f5b0`), the public SDK inventory
(`1d5a6d0a-50d8-4859-8aca-1f4968ffb344`), server build, both migrations and
the real-server Artifact journey (`b265ca87-b7fa-41f0-ab7d-b0cc2e23d2c0`),
scoped Clippy with warnings denied and cargo doc. Those independently executed
results supplement the static matrix above; this reviewer did not rerun them.
Format verification was still underway when those results arrived.
