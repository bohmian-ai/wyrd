# Maintainer review

Overall result: **FAIL**

Subject: base `08b38f74255d344e1c5ec9a6d278935a26277622` → candidate `896e563bee69ab6b5f47b31977d7de1a0ca88d57`.

The contract ownership and reuse are generally straightforward. One consolidated mandatory-rustdoc finding blocks acceptance; no optional redesign or style-only issue contributes to this result.

## Authority and method

Reviewed the complete cumulative 21-file diff and the changed symbols in their owning modules, with surrounding callers and tests. `HEAD` remained the pinned candidate; the working-tree differences are unrelated documentation/planning files. Used CodeGraph before code discovery, then direct source and caller searches. Did not read another reviewer's report or change production source.

Applied `AGENTS.md` §§2–9, 11–12, 16 and 18, `architecture/agent-rules.md`, `architecture/references/languages/maintainer-style.md`, `architecture/references/languages/spec-driven-development.md`, Wyrd design Doctrine/Client model/Prompt/Agent/Verifier/reference-authoring sections, Wyrd doctrine, ownership patterns, evaluation guidance, approved revision-6 spec and original Task 001. The approved replacement/staging instructions supersede the older implementation discriminator for this task; activation and obsolete-type removal remain Task 005 work. The owner's explicit BlockError/JudgeError decision is respected.

## Changed-surface coverage

| Changed surface | Symbols, owners, callers and tests inspected | Assessment |
| --- | --- | --- |
| `crates/skald/skald-spec/src/media.rs`, `lib.rs` | `MediaFormat`, `UnknownMediaFormat`, format metadata/parser/serde/schema implementations and all four new tests; existing `MediaRef`, `MediaKind`, `MediaSource`; new `InputSlot`, file declaration, extractor-schema and binding-validation consumers | Shared pure vocabulary is in the existing media owner. Canonical tokens, legacy Office distinctions and the format enum avoid downstream string vocabularies. Documentation incomplete: MNT-001. |
| `crates/skald/skald-spec/src/prompt.rs` | `Prompt::media_kind`, retained `media_kinds`, existing media binding/compiler behavior and new `Validator::validate_placeholders` caller | Accessor projects compiler-owned metadata; it does not recompile or reauthor Prompt state. No maintainer blocker. |
| `crates/wyrd-spec/src/card/blocks/mod.rs`, `card/mod.rs` | Every new authored type/field/variant, defaults and serde predicates; `DriftCheck` custom serde; staged module export | Cohesive declarative values, existing `AgentSpec`, `InlineableRef`, `Ref`, `DriftSpec`, comparison operators and IDs are reused. No IO owner leaks into Cards. Documentation incomplete: MNT-001. |
| `crates/wyrd-spec/src/card/blocks/validate.rs` | All `VerifierSpec` methods; `Validator` state, run ordering and validation methods; `step_references`, `ancestors`, slot/operator/workspace helpers; six tests and their setup helpers | One retained validation context, synchronous pure behavior, existing shared DAG planner and operators. New ancestry lookup serves step-check accessibility rather than another runtime graph. Documentation incomplete: MNT-001. |
| `crates/wyrd-spec/src/card/blocks/reference.rs` | `Reference`, roots, selectors, error, parser/cursor, display, serde/schema and three tests; construction and reference consumers in validation | Approved scope grammar needs root/position information absent from existing JSONPath. No runtime evaluator, loader or graph store is introduced. Documentation incomplete: MNT-001. |
| `crates/wyrd-spec/src/card/blocks/schema_path.rs` | `admits`, local-ref resolution, type exclusion, bounded hops and its test; input schema and step-output consumers | Separate narrow known-missing-shape analysis, with documented conservative unknown handling. Existing value extraction is not a schema walker. No maintainer blocker. |
| `crates/wyrd-spec/src/card/blocks/extract.rs` | All nine output variants and nested DOCX/XLSX/table/cell types; required nullable fields; `schema_for`; three contract/schema tests; validator output-schema consumer | One typed output contract drives schema filtering and field checks, with no duplicate extraction runtime. No maintainer blocker. |
| `crates/wyrd-spec/src/verification/execution.rs`, `verification.rs` | All request/input/upload/result/status/provenance/citation/verdict types; `From<&WyrdError>`; three tests; existing status, verdict, error, drift and upload types | Staged values keep lifecycle separate from verdict and reuse existing upload/error/state values. No alternate live HTTP route is added by this diff. |
| `crates/wyrd-spec/src/ids.rs` | `SlotName`, `ExecutionId`, identifier validation, `IdError::InvalidIdentifier` and both new tests; existing ID macros and parameter grammar | Existing newtype mechanisms reused. Distinct execution/input-slot identity has an approved contract purpose. |
| `crates/wyrd-spec/src/card/prompt/parameter.rs` | Added ordering derives; new Agent/code input maps and placeholder validation; existing parameter construction/grammar | Required for deterministic typed map keys; no parallel parameter parser. |
| `crates/wyrd-spec/src/error.rs` | Visibility change to `message_details`; existing public error-body formatting and new execution-error conversion consumer | Existing error metadata/rendering remains the source; no parallel error catalog. |
| `crates/wyrd-spec/src/refs/mod.rs` | New `Visit` impl and Agent/code/requirement helpers, reference-slot test; existing `visit_agent`, `DriftSpec::visit_at`, `SlotValue`, dispatch; loader resolution and validation callers | Extends the existing slot inventory. The staged `Visit` impl is not yet selected by `Spec::Verifier`, as intended for deferred activation. No second loader. |
| `crates/wyrd-spec/src/vala/eval/{plan,mod}.rs` | `plan_stages`, `validate_dag`, full topological walk, cycle recovery; `EvalSpec` planning/validation callers and new Verifier planning consumer | Existing complete DAG workflow is generalized once. Two real consumers justify generic input; no duplicate planner. |
| `crates/wyrd-spec/src/vala/eval/{operator,spec}.rs` | `ComparisonOperator::validate`, regex helper, modified `validate_task_operator`, existing operator extraction/validation and task callers | Existing regex rule is centralized in its operator owner. Modified private fallible item lacks mandatory rustdoc: MNT-001. |
| `crates/vala/vala-eval/src/{blocks,lib}.rs` | All new executor-boundary types/fields/variants/methods, Echo implementation and focused test; existing `TaskExecutor`/`Executors`, `JudgeInvoker`, `SkaldJudgeInvoker` and its Agent/Prompt/media resolvers | Narrow injected data-action/check-result seams, per-execution authority/deadline ownership described. This diff adds no second orchestrator or provider dispatch. Runtime implementations remain later tasks. |

No Python, TypeScript, authored stubs, N-API declarations, server implementation, SQL, storage, scheduler, generated file or manifest is changed. Their activation/projection is a staged handoff, not an inferred implementation in this candidate.

## Independent existing-owner comparison

- **Graph planning and ancestry:** `EvalSpec::{validate, execution_plan}` already use `validate_dag`; the candidate makes that wrapper and `VerifierSpec::plan` call the same `plan_stages`. Shared stages/cycle handling are reused as a workflow, not merely leaf helpers. `ancestors` derives upstream visibility from that same plan and declared dependencies, a new required registration rule that the older evaluator did not own.
- **References and loading:** existing `JsonPath` requires `$` and serves JSON document evaluation through Vala's `extract_jsonpath_from`/`extract_required_jsonpath_from` (`context.rs:317,327`). The approved grammar explicitly omits `$` and distinguishes input/step/verdict/requirement roots for registration. The new parser's missing behavior is root/selector parsing and position analysis; replacing it with the old runtime JSONPath extractor would lose those distinctions and put runtime machinery in the pure contract layer. Card references continue to use `Ref`/`InlineableRef` and the existing `Visit`/`ReferenceSlotVisitor` loader inventory; `resolve_card_references` and `check_resolved_references` remain the complete loading owners.
- **Validation and Prompt state:** existing Prompt compilation owns variables, response types and native media kind metadata; `media_kind` reads that retained state. The new validator checks binding relationships using those derived values, without another Prompt compiler or registration service. `ComparisonOperator::validate` reuses the complete old regex rule for offline Eval and new checks. Drift validation stays `DriftSpec::validate`.
- **Execution and errors:** old `TaskExecutor` returns task/assertion outcomes and `JudgeInvoker` invokes a constrained judge. They cannot directly represent data-producing extract/code/Agent actions with original-slot provenance or the new position/deadline call contract. The candidate stages only the new injected seams, not another execution workflow. Removing JudgeError from these contracts is owner-approved, while old offline evaluation owners are preserved. A one-implementation trait objection would be inappropriate for this explicitly required injected handoff.
- **Evidence and uploads:** upload plans, exact Card references, SHA-256 identity, trace identity, drift detail and status/error vocabulary are reused. No new artifact repository, cache, transport or durable evidence owner appears.

## Material finding

### MNT-001 — VIOLATION — required Rust documentation is incomplete

**Changed anchors:** `crates/wyrd-spec/src/card/blocks/validate.rs:145`; `crates/wyrd-spec/src/card/blocks/reference.rs:179`; `crates/wyrd-spec/src/card/blocks/mod.rs:335`; `crates/skald/skald-spec/src/media.rs:300`; `crates/wyrd-spec/src/vala/eval/spec.rs:182`.

**Governing requirement:** `AGENTS.md` §16 requires substantive rustdoc for every new or materially changed Rust item, regardless of visibility, and a `# Errors` section on every fallible method/function. It expressly makes missing rustdoc a hard blocker. `architecture/agent-rules.md` and Maintainer Style §Documentation repeat that requirement. This is the mandatory-rustdoc exception to the review's style-only non-blocking threshold.

**Source evidence:**

- New fallible helpers in `validate.rs` describe their names but omit `# Errors`: `requirement_owners` (145), `Validator::run` (208), `validate_evidence_mode` (244), `validate_step_shape` (292), `validate_requirement` (355), `validate_drift` (401), `validate_code` (430), `validate_agent` (468), `validate_placeholders` (566), `validate_reference` (639), `validate_slot` (743), and `validate_operator` (783).
- New `Tokens::rest` (`reference.rs:179`) has no field rustdoc. New `Reference` methods `fmt` (231), `from_str` (269), `serialize` (275), `deserialize` (281), `schema_name` (288), and `json_schema` (292), together with its associated `Err` type (267), have no item rustdoc.
- New `DriftCheck::{serialize, deserialize}` (`mod.rs:335,345`) lack item documentation and error conditions despite custom flattening and missing/unknown-field behavior.
- New `MediaFormat` methods `fmt` (300), `serialize` (334), `deserialize` (340), `schema_name` (348), `json_schema` (352), its associated `Err` (306), and `UnknownMediaFormat`'s tuple field (183) lack item documentation. `FromStr::from_str` itself does have `# Errors` and is not part of this omission.
- Materially changed `validate_task_operator` (`vala/eval/spec.rs:182`) still has no rustdoc or `# Errors`; this is an explicitly changed symbol, not a demand to document its untouched neighboring functions.

**Concrete maintenance consequence:** a maintainer cannot inspect the private validation stages or custom wire implementations' rustdoc to determine refusal conditions, delegation, and schema/serialization invariants. Successful Clippy does not satisfy this explicit repository acceptance requirement.

**Smallest correction:** complete substantive rustdoc on the above changed items, documenting each actual error condition and how its delegated conversion/validation operates. Check remaining newly introduced modules/items for the same omission in the same correction. Do not add a new lint/check, refactor behavior, or document untouched old symbols. Existing `Reference::parse`, `VerifierSpec::validate_with`, and new test `# Panics` sections are nearby examples of the intended shape.

**Focused closure proof:** inspect the candidate documentation at these exact locations against the complete touched-item requirement, confirm every fallible changed method has meaningful `# Errors`, and run the repository formatting lane. This is a documentation-only correction; a new behavior test or broad journey rerun would not prove it.

**Severity:** must-fix / BLOCK_BEFORE_MERGE.

## Non-blocking notes and verification limits

No optional simplification is required. The cohesive validation owner, closed enums, existing visitor extension and shared DAG/operator changes are reasonable under the approved task. General field-name/line-count preferences are deliberately omitted.

The pinned task also requires global/exact tool permissions, while `PermissionScope` and runtime permission owners remain unchanged; this was handed to the orchestrator for acceptance/security review rather than represented as a separate maintainer-style defect.

This was static review. No tests, rustdoc build, lint, codegen or full integration/journey suite was rerun. The supplied green checks are implementation evidence only, and codegen was explicitly reported unrun. Nothing in this report independently certifies those lanes. Risk: mandatory documentation remains incomplete until MNT-001 closes.
