# Maintainer review — Task 001 cumulative R2

Result: **PASS**

## Immutable subject and authority

- Base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Candidate: `542957a3622461e259aee5f8108d978e18adad1c`.
- Latest remediation locator: `eb1681461897ee1eb9fe1fa4e00cb0b435658559..542957a3622461e259aee5f8108d978e18adad1c`.
- Reviewed the complete 26-file original-base-to-candidate source change, original Task 001, approved specification r6, initial verdict/remediation, and R1 verdict/validated ledger/R2 remediation. Prior conclusions were closure hypotheses, not acceptance evidence.
- Applied `AGENTS.md` §§4–8, 11, 15–16, `architecture/agent-rules.md`, active Wyrd design/doctrine, the reference router, spec-driven-development, Rust-core/error guidance, and `languages/maintainer-style.md`. Task 001's staged handoff and the owner's explicit Task 005 judge-removal direction delimit this review.
- CodeGraph was used before code discovery, followed by complete owner bodies, cumulative/latest diffs, caller searches and focused neighboring implementations. No other current discovery report or intended verdict was consumed.

This is a maintainer/complexity audit. It does not independently certify every runtime acceptance obligation or replace the separate security and behavior reviews.

## Changed-surface coverage

Paths below are relative to the repository root. Each row includes the new or materially modified items, their surrounding owner, relevant callers and tests.

| Changed files | Owner, consumers and proof inspected | Maintainer result |
|---|---|---|
| `crates/wyrd-spec/src/card/blocks/mod.rs` | All authoring types, defaults/predicates, custom flattened `DriftCheck` codecs and their consumers in validation, reference visiting, result contracts and Vala call signatures. | PASS. Closed action/check enums and declarative values keep the required distinction between data-producing steps and verdict-producing checks. No IO or runtime dependency is attached to Card data. |
| `crates/wyrd-spec/src/card/blocks/reference.rs` | `Reference`, roots/segments/error, parser, `Tokens`, accessors, formatting, FromStr/serde/schema implementations and all three inline tests; compared with existing `JsonPath` and Vala JSONPath extraction. | PASS. This parses the approved execution-scope grammar and its restricted roots; existing JSONPath starts with `$` and evaluates runtime data, so it cannot supply the same authoring/position contract. No second runtime evaluator is added. |
| `crates/wyrd-spec/src/card/blocks/schema_path.rs` | Complete `Walk`, shared budget/reference resolution, `admits`, `fits`, `fits_object`, finite-value/type/required helpers, new `json_type` and `literal_name`, and selector tests; traced both consumers in `validate.rs`. | PASS. One bounded owner handles related structural analyses. Latest helpers implement the bounded R2 cases without a regex engine, schema registry, new authoring profile or generalized inclusion framework. |
| `crates/wyrd-spec/src/card/blocks/validate.rs` | Entire production validation module and inline fixtures/tests: domain methods, resolver, ID namespace, derived dependency/ancestor state, slot/position/Agent/media/code/drift/operator/schedule rules, generated Verdict schema, all public-entry tests. | PASS. `Validator` groups a real pass's resolver and derived lookups; it neither owns durable state nor duplicates a registry/loader. Test fixtures are shared across realistic pure authoring outcomes. Long functions correspond to closed rule dispatch; no refactor is required solely for length. |
| `crates/wyrd-spec/src/card/blocks/extract.rs` | All format-discriminated outputs, explicit required nullable fields, `schema_for`, three tests and validation's known-output consumer. | PASS. One source declares the required output shape; generated branch selection avoids a parallel hand-authored extraction schema. No redundant counts, extracted-image helper or runtime parser appears. |
| `crates/wyrd-spec/src/card/mod.rs` | New module export and surrounding existing Verifier export; old `card/verifier.rs` and live `Spec` ownership. | PASS. Staged blocks are available to parallel owners without changing the live Card discriminator or accepting two live authored forms. |
| `crates/wyrd-spec/src/card/agent.rs` | `BuiltinTool` variants, shared catalog, names, exact lookup and filesystem classification; declaration validation, permission scope and provenance consumers. | PASS. Closed catalog serves three distinct contract consumers; no executable registry or one-implementation service trait is introduced. |
| `crates/wyrd-spec/src/auth/permission_scope.rs` | Tool variant, coverage and validation in complete existing scope owner; runtime `Permission::validate`, covers/subsumes and `PermissionSet::intersection`. | PASS. Exact equality extends the existing scope relation; no separate tool authorization algorithm/store exists. |
| `crates/shared/wyrd-runtime/src/permission.rs` | Resource/string handling, tool constructors, scope/resource/action validation, existing wire codecs, coverage/intersection, new focused tool test and surrounding permission tests. | PASS. Existing generic permission machinery carries global/exact grants and attenuation; tool handling does not recreate it. |
| `crates/shared/wyrd-runtime/src/request_context.rs` | New accepted-authority test and existing principal/delegation serialization owner. | PASS. Test exercises the existing encoding and exact grant meet; no production authority wrapper is added. |
| `crates/wyrd/wyrd-server/src/components/gateway/batches.rs` | Complete `invocable` filter and exhaustive scope match. | PASS. The new non-gateway scope follows the existing non-gateway arms; the established gateway reach logic remains owned in this helper. |
| `crates/skald/skald-spec/src/media.rs` | Existing `MediaRef`/`MediaKind`, all new `MediaFormat` variants/canonical names/MIME aliases/capabilities, error and conversion/schema traits, four tests. | PASS. Format vocabulary belongs beside shared native media, with separate legacy/OOXML cases. No IO resolver or provider runtime is moved into this owner. |
| `crates/skald/skald-spec/src/lib.rs` | Shared media exports and existing ownership. | PASS. Exposes the one vocabulary; no alias layer. |
| `crates/skald/skald-spec/src/prompt.rs` | New `media_kind` accessor, retained compiler-derived media-kind state, surrounding response/media methods and caller in placeholder validation. | PASS. Small accessor exposes existing compiler knowledge; Verifier validation does not reparse provider requests or reauthor placeholders. |
| `crates/wyrd-spec/src/card/prompt/parameter.rs` | Added ordering derives; parameter-name owner and block input maps. | PASS. Derives support deterministic typed maps without a new wrapper. |
| `crates/wyrd-spec/src/ids.rs` | Slot-name macro instantiation, shared identifier predicate, execution UUIDv7 macro instantiation, new ID error and two tests; existing macro constructors/serde validation patterns. | PASS. New identities use existing validated-ID mechanisms and the existing parameter grammar rather than new string parsing infrastructure. |
| `crates/wyrd-spec/src/refs/mod.rs` | Staged `Visit` implementation and three helpers, new visitor test, surrounding existing Agent/Verifier/Drift visitor and slot mutation mechanism. | PASS. New blocks enter the canonical loader visitor; there is no second reference graph, loader, cache or identity resolver. |
| `crates/wyrd-spec/src/vala/eval/plan.rs` | `validate_dag` delegation, `plan_stages`, unchanged topological/cycle recovery implementation and existing DAG tests/callers. | PASS. Two real node representations use the extracted planner. Block dependency extraction supplies graph edges; it does not implement a competing graph algorithm. |
| `crates/wyrd-spec/src/vala/eval/mod.rs` | Shared planner export. | PASS. Existing owner remains discoverable. |
| `crates/wyrd-spec/src/vala/eval/operator.rs` | New inherent `ComparisonOperator::validate` and existing regex rule. | PASS. Existing operator owns its validation instead of duplicating regex checks in Verifier validation. |
| `crates/wyrd-spec/src/vala/eval/spec.rs` | Complete changed `validate_task_operator` delegate and callers from existing Eval validation. | PASS. Keeps existing Eval consumers on the same rule. |
| `crates/wyrd-spec/src/verification/execution.rs` | All evidence/request/upload/status/result/provenance/Verdict/Citation/Locator contracts, stable error conversion, three wire tests; Vala handoff and generated schema consumer. | PASS. Shared records cover attribution without a new result envelope, raw tool-output store or error catalog. Required no-output digest/byte semantics are documented. |
| `crates/wyrd-spec/src/verification.rs` | Staged execution module export, existing live request/status/result owners. | PASS. No live routes switched by this export. |
| `crates/wyrd-spec/src/error.rs` | Visibility extension of existing `message_details`, existing docs/body and new conversion caller. | PASS. Reuses existing stable code/message production; no parallel code/status/problem mapping. |
| `crates/vala/vala-eval/src/blocks.rs` | Complete call-position/resolved-input/source/deadline/output/error/executor contract and tests; compared with `TaskExecutor`, `Executors`, `JudgeInvoker`, `JudgeError` and current Vala context extraction. | PASS. Async seams are narrow injection boundaries for separately implemented action/check kinds; no extra orchestrator, production mock or fallback implementation is added. Existing `TaskExecutor` is tied to legacy `EvalTask`, snapshots and `TaskOutput`; forwarding through it would not express the approved resolved-value/data-versus-verdict handoff. |
| `crates/vala/vala-eval/src/lib.rs` | Staged block module export. | PASS. New contract is exported without mounting a competing execution path. |

## Complete-workflow reuse assessment

| New or extended mechanism | Existing alternative inspected | Why extension/new code is sufficient and earned |
|---|---|---|
| Scope parsing | `vala::eval::JsonPath`, `vala-eval::context::{extract_jsonpath_from,extract_required_jsonpath_from}`, existing `Ref`/`CardRef` and `refs::Visit` | JSONPath is a different grammar and runtime query; Card refs identify Cards. The approved scope grammar needs typed roots/positions. The candidate adds only pure parsing, uses existing ID owners, and leaves Card identity/loading to `Visit`. |
| Schema structural analysis | Shared compiled `Prompt` response schema, existing Prompt shape validation, generated typed Verdict/Citation/Locator, existing schema facilities | Existing validation checks Prompt compilation/shape, not whether its output can supply Verdict fields or a reference is known missing. The candidate uses those outputs directly and adds one bounded local walk; no remote retrieval, schema compiler, validator platform or duplicate provider compiler. |
| Graph planning and access rules | Existing `validate_dag` topological planner/cycle recovery | Planner is extended in place and still used by Eval. Domain edge derivation and upstream visibility are the missing block-specific facts, held only for a validation pass. No persistent graph or second traversal platform is present. |
| Block validation | Current Verifier validation and existing Prompt, Agent, Drift and operator contracts | Legacy Verifier dispatch validates one implementation and cannot express new named-slot block rules. Staging a replacement domain validator is required while live cutover remains Task 005; underlying Prompt compilation, Drift validation, operator checks and references continue through their owners. |
| Tool authorization/catalog | `Permission`, `PermissionScope`, `PermissionSet`, accepted authority, Agent tool names | Existing permission coverage/meet are reused whole. One closed vocabulary is shared by declarations, exact scopes and provenance; no new principal, grant store or checker. |
| Execution seams | Existing `TaskExecutor` and `JudgeInvoker` | The new contracts supply values/source slots, position/deadline, data versus verdict, retryability and provenance that legacy signatures lack. The task explicitly hands these to parallel runtime owners. Their presently staged production-caller count is not evidence that the approved handoff should be deleted. Legacy judge removal remains Task 005. |

## Prior-finding closure from candidate source

| Stable finding | Current evidence | Maintainer disposition |
|---|---|---|
| FIND-TASK-001-1: tool permissions | Scope and resource cases feed current `Permission::validate`, axis coverage and existing intersection. Exact/global constructors and accepted-authority fixture use the same types. Gateway's exhaustive arm ignores Tool alongside other non-gateway scopes. | Closed within this contract task; no parallel auth owner or widening conversion found. |
| FIND-TASK-001-2: catalog | `BuiltinTool::ALL` contains all ten approved names, `from_name` is used by step validation, and permission/provenance use the enum. The public validation test covers all ten and rejects unknown names. | Closed; no private six-name list remains. |
| FIND-TASK-001-3: bounded Verdict compatibility | `Walk::fits` preserves own keywords after insufficient authored `allOf` parts, derives missing primitive type from finite values, and checks exact anchored new pattern fields in `fits_object`. `validate_agent` invokes it through the one check-position guard. Latest public validation fixture includes the prescribed positive/negative reason/outcome/pattern cases, plus generated/local-ref schemas, inline/resolver checks, step-check checks and arbitrary action schema. | Closed for the bounded required correction. No general regex/schema theorem or public schema profile is inferred. |
| FIND-TASK-001-4: tool provenance | `ToolCallRecord` includes typed digest and returned bytes with honest no-output docs; `ActionOutput` and `StepResult` share that record. Existing result fixture covers output/error records and Vala handoff test carries digest/count through serialization. | Closed; no runtime projector is demanded in Task 001. |
| FIND-TASK-001-5: schema recursion | Both analyses share `Walk`'s node/ref-hop budget across recursive helpers and branches. Selector exhaustion remains unknown/admitted; compatibility exhaustion is unproven/refused. Public input/output cyclic tests and finite-tree known-missing test remain. | Closed; no unbounded replacement walk found. |
| FIND-TASK-001-6: rustdoc | See mandatory documentation pass below, including latest formatter Errors sections. | Closed. |

## Mandatory changed-item rustdoc pass

The cumulative pass covered the new private/test/trait items, not only the two latest formatter locations:

- Authoring values and fields/default predicates; custom `DriftCheck` serialization/deserialization explain flattened `on`, delegated shape checks and Errors.
- Reference parser/accessors/cursor field and methods; Display, FromStr associated error/method, serde and schema methods; inline test module/functions. `Reference::fmt` now documents writer failure under Errors, with its body unchanged.
- Complete schema walk: owner/budget field, public-private entry functions, resolver/spending/fit/object/values helpers and type/required predicates. The new `json_type` and `literal_name` have operation-specific rustdoc; object/fit docs describe finite type inference, allOf fallthrough and the bounded exact-pattern case. Selector test has its contract and Panics documentation.
- Every fallible validation method/helper has Errors describing its actual refusal boundary; `verdict_schema` records the serialization invariant under Panics. The test module, shared fixtures and all tests have substantive role/outcome docs and relevant Panics sections.
- Extraction types/fields and schema selection describe required nullable fields, ordering, source/locator meanings and unsupported formats; tests and module have rustdoc.
- New tool/resource/scope cases and constructors; modified validation/coverage docs; new permission/accepted-authority tests; planner extension, operator validate and Eval delegate; new IDs/error and ID tests; new visitor methods/helpers/test module/test; complete execution records and stable-error conversion.
- Shared `MediaFormat`, its tuple-error field, aliases/capabilities, trait-associated error, conversion/schema methods and tests are documented. `MediaFormat::fmt` now documents writer failure under Errors without changing canonical output.
- Vala call-value/source/position/deadline/output/error/executor items and test helpers/functions are documented. Module contract states authority construction, engine-owned deadline/drop cancellation, resource release and retry semantics; both fallible executor methods document Errors.
- New exported modules have their own module-level docs; re-exports reuse the defining items. No unrelated legacy rustdoc rewrite is required.

No remaining missing or placeholder rustdoc on a new/materially changed item was identified. Formatting preferences and import placement without a material consequence are not converted into blocking findings.

## Findings

Material proposed findings: **none**. Source-local finding ledger: **empty**.

No optional redesign is proposed. The concrete `Walk` and pass-local `Validator` own meaningful budget/derived invariants; the planner and permission owners are extended instead of copied. New declaration types express approved wire distinctions. There is no new dependency, config knob, registry, transport, persistence layer, framework or execution lifecycle to delete within this task's approved scope.

## Evidence and limits

Inspected the current review's focused log (the exact Verdict validation target passes), library log (869/869 wyrd-spec tests), owner Clippy log (successful completion) and codegen log (successful checks). Earlier task/remediation evidence supplies four-library 1139-test and workspace consumer compilation coverage. This report's conclusions come from source inspection; it did not rerun those commands or claim new dynamic proof. Candidate HEAD matched the immutable subject when checked during the audit.

Skipped: live providers, server/SQL/sandbox runs, SDK journeys and broad aggregates, as deferred by the approved staged task. Risk: those later runtime owners must implement and prove authority enforcement, cancellation, evidence validation and final cutover; this contract-only PASS does not certify them.
