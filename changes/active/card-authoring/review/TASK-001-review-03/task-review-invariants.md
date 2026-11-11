# TASK-001-R3 invariant review

**Result: FAIL.** Three source-backed implementation defects remain in the reviewed invariant scope. These are discovery proposals for independent validation, not assigned stable FIND IDs.

## Subject and coverage

Reviewed immutable candidate `37b3c2f96b1bd425bf1f0923a1950d0363c40907` against immediate task base `a387f2a47a145a867c8a4948966fcbd136af9961`, with original typed Graph/capture behavior at that base and review-02 verdict as prior context. Read the approved revision-4 specification, current R3 task, task-review skill, repository rules, maintainer guide, workflow, and owning in-memory authoring/reference/registry architecture. HEAD remained the candidate; no implementation or packet edits were made.

Fresh inspection covers `Body`, `FieldValue`, `Captures`, location composition, generated setters/additions, all nested body conversions, Card envelope/decode/snapshot, Graph capture/deduplication, Artifact local/reference/selection conversion, and their loader/publication consumers. Read the changed declaration dispatch and state tail, the six local test bodies and relevant public inventory/Artifact callers. This deliberately does not repeat unchanged server authorization, executor, storage, or SDK language-boundary discovery. Prior FIND-TASK-001-1 closure was a hypothesis to recheck because the representation changed substantially, rather than assumed acceptance of the new implementation.

No Cargo commands were run: the root orchestrator owns those checks. Test references below describe inspected source coverage, not fresh execution claims.

## Acceptance matrix

| Requirement / invariant | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Replacing a field discards only that field's captures and duplicate attempts | `authoring/mod.rs:227–240,255–295`; `Body.captures` is keyed by field, setters replace its entry, additions relocate into it | `offline_edits_preserve_snapshots`, `replacements_keep_only_current_dependencies` inspect map reset, component replacement, Trigger replacement, and shared Artifact cases | PASS |
| Nested additions retain duplicate diagnostics and locate deferred files correctly | `Captures::under` at `mod.rs:111–119`; `Body::push/insert/into_field` at `243–325`; Step/Requirement/Action/Check/Component/Binding conversions preserve captures | Local duplicate input and source-read tests; source trace follows source → action → step → steps[index] and nested check → requirement → requirements[index] | PASS |
| Child insertion and returned declarations capture by value; later edits revalidate | Owned `Value` and cloned captures; `CardDefinition::declaration` builds a new Graph; completed declarations contain owned canonical submissions | `offline_edits_preserve_snapshots`, `field_edits_defer_validation_and_preserve_snapshots` | PASS |
| Construction/editing performs no source reads; every snapshot reads current files | `CodeSource::file` only records path and null placeholder at `mod.rs:530–540`; `Card::envelope` reads at `642–650` | Deferred source test creates missing file after edit, changes file between declarations, compares both snapshots | PASS |
| Equal active definitions deduplicate by canonical submitted content; true conflicts refuse | `Graph::absorb` uses `same_content`, but `same_content` compares undecoded envelope JSON | Existing test proves identical clones and genuine differing Agent config only; empty/default canonical equivalence is absent | FAIL — INV-R3-001 |
| Card-valued Artifact selection preserves sibling intent; exact registered refs stay external | Ordinary Card/Artifact conversion correctly distinguishes intent; `Artifact::file` discards the converted reference value | Current file-selection tests cover local `from_path` and registered `CardRef`, not spec-only Artifact selection | FAIL — INV-R3-002 |
| Malformed raw domain values survive staging and meet canonical validation before publication | `CardDefinition::new` drops non-object `spec` into empty `Body` before loader decoding | YAML/code diagnostic test covers malformed nested fields, not malformed top-level spec shape | FAIL — INV-R3-003 |
| Local Artifacts validate before publication, source paths remain local, and existing upload/saga/state owners remain in control | `Graph::declaration` constructs canonical input before return; `Cards::declare` at `handle.rs:295–311` invokes existing `upload_artifact` then `submit`; state at `state.rs:655–664` uses same Cards/client and existing `pull_declared` | Inspected local/public Artifact and declaration journey assertions; execution coordinated by root | PASS within unchanged lifecycle scope, subject to the malformed-shape defect above |
| No second graph ordering/parser/publication workflow | Graph retains local capture collection; canonical decoding is `parse_card_value → parse_single_envelope → Spec::from_kind_and_value`; canonical loader validates/orders; Cards reuses upload/saga; state reuses hydration | Full affected production bodies and sibling owners inspected | PASS |

## Findings

### INV-R3-001 — INCORRECT / REGRESSION: deduplication uses raw spelling instead of canonical content

**Obligation:** REQ-008 and AC-002 require equal active definitions to declare once and equivalent construction styles to produce the same canonical content. The prior Graph compared typed canonical specs after removing dependency staging; the new raw representation must preserve that policy.

**Changed location:** `crates/shared/wyrd-client/src/authoring/mod.rs:978–982` (`same_content`), consumed by `Graph::absorb` at `965–966`.

**Reachable producer-to-consumer path:** create two `Service::new("shared", "1.0.0")` values, and apply `.components([] as [Component; 0])` to only one. Attach them under distinct aliases of a containing Service. This is ordinary supported reuse of the same Card through field-oriented construction. One child envelope contains `spec: {}` and the other `spec: {components: []}`. `ServiceSpec.components` explicitly defaults to an empty vector and skips empty serialization (`wyrd-spec/src/card/service.rs:22–24`), so both canonical submissions are identical. `Body::set_list` correctly creates the explicit empty array, but `Graph::absorb` invokes `same_content` before canonical decoding. Both envelopes are valid, and their raw JSON differs, so it returns `SpecDuplicateSubmission` instead of one shared child submission. The same cause covers omitted versus explicit empty map/list/default/null fields across Card kinds; it is not a separate finding for each setter.

**Consequence/materiality:** composing equal Cards built by reusable helpers and full-field/empty-collection setters fails locally. This is a direct regression in the required deduplication policy, not a request to equate genuinely different authored definitions or ignore active dependency conflicts.

**Existing owner and smallest correction:** compare successful snapshots through the existing canonical Card decoder/normalizer when deciding whether an identity has identical submitted content. Do not add a default-value rule table or a second canonicalizer. Preserve all capture traversal and actual conflicting child refusal; preserve ordinary diagnostics for invalid definitions and source descriptor behavior. The current raw-envelope comparison cannot implement canonical equality. This is a local comparison-boundary correction, not a new graph workflow.

**Focused proof:** the containing Service with two equal child identities above produces exactly one child submission; compare it with the direct single-child canonical content. Retain a same-identity differing description/config case that refuses. Existing local test infrastructure suffices.

### INV-R3-002 — INCORRECT: spec-only Artifact file selection changes sibling intent to external intent

**Obligation:** REQ-004/008, AC-001/007 and architecture's in-memory authoring section require a Card at a permitted reference position to contribute a sibling definition; exact `CardRef` stays external. Local source Artifacts are the explicit external/publication-first exception.

**Changed location:** `crates/shared/wyrd-client/src/authoring/kinds.rs:268–273` (`Artifact::file`), specifically `selection(self.card.card_ref().value(), ...)` at line 271.

**Reachable producer-to-consumer path:** construct a spec-only `Artifact::new("checks", "1.0.0").artifact_kind("code")`, select `.file("checks.py")`, and pass it to `Action::code(..., "run")` in a standalone Verifier step. `Artifact::file` calls `FieldValue::from(self.clone())`. For a non-local Artifact, that existing conversion at `kinds.rs:328–329` correctly yields a sibling reference plus the captured Card definition. The method retains the captures but replaces the reference value with raw `card_ref().value()`, an external reference. Thus the Graph includes both Artifact and Verifier nodes, while the code source claims the Artifact is already registered. Canonical reference visitation traverses code sources, but canonical graph building deliberately creates edges only for sibling references (`wyrd-spec/src/graph/mod.rs:170–183`). With no Service root to choose, the two unconnected roots trigger `MultipleRoots` in `graph/root.rs:16–53`, surfaced through loader order at `wyrd-loader/src/order.rs:38–39`. This failure happens before any network call.

**Sibling writers/consumers:** direct Card-valued reference fields use `FieldValue::from` unchanged; `CardRef::file` deliberately writes a plain external value (`mod.rs:483–485`); local Artifact conversion deliberately writes external value and captures publication (`kinds.rs:331–337`). All workspace/action/check consumers receive the `ArtifactFile`'s value unchanged through its `From` adapter. There is no downstream consumer that can restore the lost intent.

**Consequence/materiality:** the explicitly supported spec-only Artifact/Card selection cannot be used in a valid standalone Verifier/Agent graph. In a Service graph the sole-Service root fallback can mask ordering failure, but the selection still asserts external lookup rather than the captured sibling. This violates an explicit reference-intent obligation; the finding does not require adding byte upload behavior for spec-only Artifacts.

**Smallest correction:** use the existing Artifact-to-FieldValue conversion's reference value inside the fixed selection, together with its captures. That owner already supplies the correct sibling/external distinction. Do not add a new dispatcher, registration operation, or consumer guard.

**Focused proof:** a spec-only Artifact selection embedded in a code step produces a sibling selection, one Artifact submission, and a valid Verifier-root declaration. Keep the existing registered selection external/no-publication assertion and local selection external/one-publication assertion. One local test can exercise the three branches.

### INV-R3-003 — INCORRECT: raw non-object specs are silently replaced with valid empty specs

**Obligation:** REQ-002, AC-002 and YAML/code validation parity require correctly typed malformed domain values to remain present until canonical validation refuses the completed snapshot; validation must precede publication.

**Changed location:** `crates/shared/wyrd-client/src/authoring/mod.rs:834–838`, `CardDefinition::new`.

**Reachable producer-to-consumer path:** call the public `CardDefinition::new(CardKind::Service, "service", "1.0.0", json!("malformed"))`, then `.declaration()` or either declaration owner. The signature accepts `Value`, so this is not a host-language type error. The constructor initializes `Body::default()` and copies fields only for `Value::Object`; the string is discarded. `Card::envelope` emits an empty object instead, and `Card::decode` never sees the supplied malformed value. All Service fields have defaults, so canonical decoding and empty single-Service graph validation succeed. The same envelope with its original string spec goes through `parse_single_envelope → Spec::from_kind_and_value → decode<ServiceSpec>` (`wyrd-loader/src/parse.rs:231–237`, `wyrd-spec/src/envelope.rs:755,784`) and refuses its non-map type. A native caller can therefore publish a different empty Card in place of the malformed definition it supplied.

**Sibling producer inspection:** named Card constructors intentionally start with an empty object; that is valid staging and must stay valid. Their one-field setters preserve malformed values in `Body.fields`, which then reach canonical serde correctly. `Body::value` also flattens Operator.action and Check.drift.spec, but these cannot by themselves recreate this valid-empty-Service path: their required canonical discriminant/payload decoding still refuses malformed values. There is no need to add an independent validation check at every setter or consumer. The missing preservation occurs in the raw-spec constructor.

**Consequence/materiality:** code and YAML disagree on malformed raw input, and declaration can durably accept unintended empty content. The public raw constructor is supported and used by the public inventory; this is explicit malformed-input integrity behavior, not a request for general malformed-input hardening.

**Smallest correction:** preserve the supplied raw spec value until the existing shared decoder sees it, while keeping valid object and empty-object staging intact. Let the existing canonical deserializer issue the error; do not add a parallel schema/type rule or a downstream publication guard. This is a local staging correction.

**Focused proof:** equivalent raw string/null spec code and parsed-envelope inputs both refuse with the canonical error family/path, while `{}` remains a valid empty Service and ordinary named constructors remain editable. Existing local diagnostic test helpers suffice.

## Limits

No source edits, Cargo/test execution, live server checks, or unrelated runtime rediscovery. The three proposed findings need independent validation; root-owned checks may establish additional execution evidence but do not replace the source paths above.
