# Repository standards review — TASK-001 cumulative R2

**Overall: PASS.** No material repository-standards finding remains. In particular, **FIND-TASK-001-6 is source-closed**, including the two formatter error sections and the cumulative private/helper/trait/test documentation surface. The nonblocking layout deviations below do not change this verdict.

## Review Findings

### Critical

None.

### Important

None proposed by this standards audit.

### Suggestions

No optional redesign or additional runtime work is requested.

## Immutable subject and method

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Original base: `08b38f74255d344e1c5ec9a6d278935a26277622`.
- Previous candidate: `eb1681461897ee1eb9fe1fa4e00cb0b435658559`.
- Reviewed candidate: `542957a3622461e259aee5f8108d978e18adad1c`.
- Inspected all 26 files in the original-base-to-candidate source diff, expanding into manifests, lockfile, shared schema generator, reference visitor, permission coverage/intersection, accepted-authority representation and the gateway sibling consumer. The latest four-file remediation was a locator, not the acceptance scope.
- Read the approved r6 specification, original task, both prior verdicts/validated finding ledgers/remediation tasks and the immutable subject. Did not read other current discovery reports.
- Read the invoked task-review skill and followed the reference router. CodeGraph exploration preceded code discovery; subsequent source reads completed the actual changed bodies and documentation. No implementation summary was used as a substitute for source.
- HEAD matched the immutable candidate; candidate-relative `crates/` diff was empty when checked. The only write owned by this reviewer is this report. Production source, spec, tasks, previous reports and generated outputs are untouched.

The approved specification explicitly replaces the old implementation split, while Task 001 stages contracts without activating a second live protocol. Consequently, old design vocabulary in still-live owners is not treated as a new regression or an instruction to perform Task 005 early. Full legacy judge removal, including offline evaluation, remains Task 005 per current owner direction; scenario simulation is not blanket-removed.

## Authority coverage

All rows also apply `AGENTS.md` §§1–6, 10–12, 15–16 and 18, `architecture/agent-rules.md`, the invoked task-review skill, the active design/doctrine, and the approved staging scope. References below are under `architecture/references/` unless fully qualified.

| Changed surface, including all cumulative files | Applicable routed authority | Source coverage / applicability |
|---|---|---|
| `crates/wyrd-spec/src/card/blocks/mod.rs`, `card/mod.rs` | `doctrine/positioning-and-vocabulary.md`, `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/rust-core.md`, `languages/errors.md`, `languages/agent-harness.md`; design §§Doctrine, Verifier, Reference forms | Declarative inputs/actions/checks/decision, serde defaults/closure, shared IDs/references and staged export. No Card-kind or live `Spec` cutover. |
| `card/blocks/reference.rs` | Same contract/ownership/Rust/harness authorities; design reference model; AGENTS §16 | Parsed execution-scope grammar, cursor state, Display/FromStr/Serde/schema trait items, field/variant docs and all tests. This scope grammar is distinct from durable Card references. |
| `card/blocks/validate.rs`, `card/blocks/schema_path.rs` | Same authorities plus `languages/testing-workflows.md`, `languages/maintainer-style.md`, `domain/evaluation.md`, `domain/drift-monitoring.md` | Full Validator/Walk owners, pure helper/test bodies, both schema consumers, generated Verdict source, schedule and drift reuse; no provider, external schema or registry IO. |
| `card/blocks/extract.rs` | Contract/Rust/harness/testing authorities, `domain/evaluation.md`, `domain/telemetry-observations.md` | All nine tagged outputs, required nullable fields, source locator semantics, selected schemas, complete tests. No extractor or new evidence store. |
| `verification/execution.rs`, `verification.rs`, `ids.rs`, `error.rs` | Contract/Rust/errors/harness/testing authorities, design §§Observation identity, Verifier, Registry error catalog; `domain/telemetry-observations.md` | Typed wire identity/status/result/provenance/citation contracts, stable error reuse, schema derives, staged module export and all new identifier tests. |
| `auth/permission_scope.rs`, `card/agent.rs`, `crates/shared/wyrd-runtime/src/permission.rs`, `request_context.rs` | Contract/Rust/errors/testing authorities; `architecture/wyrd-security-posture.md` §§Authorization and policy, Delegation, Principal lifecycle; design doctrine 18 | One ten-name catalog, exact Tool scope, generic three-axis checking/attenuation, accepted authority and new tests. No built-in-role grant or separate permission service. |
| `crates/skald/skald-spec/src/media.rs`, `prompt.rs`, `lib.rs` | `doctrine/architecture-constraints.md`, `architecture/patterns.md`, `languages/rust-core.md`, `languages/testing-workflows.md`, `domain/evaluation.md`; design §§Prompt, Agent | Shared canonical media vocabulary, complete codec/schema/trait/helper/test docs, derived media-kind accessor and re-exports. Existing media constructors are unchanged. |
| `card/prompt/parameter.rs`, `vala/eval/operator.rs`, `vala/eval/plan.rs`, `vala/eval/spec.rs`, `vala/eval/mod.rs` | Rust/ownership/errors/testing authorities, `domain/evaluation.md`, `domain/vala-architecture.md` | Ordered parameter newtype, existing comparison owner and validation delegate, generalized existing DAG owner, unchanged sibling Eval entry point and exports. |
| `refs/mod.rs` | Design doctrine 19 and Reference-slot inventory; contract/Rust/ownership/testing authorities | Existing Visit inventory extended for Agent, code Artifact and drift refs; inline Agent Prompt slots reuse existing traversal; all added helper/test docs. |
| `crates/vala/vala-eval/src/blocks.rs`, `lib.rs` | `domain/vala-architecture.md`, `domain/evaluation.md`, `domain/telemetry-observations.md`, Rust/ownership/errors/testing authorities | Narrow asynchronous injected action/check seams, retained source-slot provenance and deadline/position contracts, test implementation and result handoff. No engine, worker or host lifecycle added. |
| `crates/wyrd/wyrd-server/src/components/gateway/batches.rs` | Security posture authorization/delegation; design §§Agent, Runtime identity; ownership/Rust authorities | Full `invocable` filter/projection body: new Tool arm remains disjoint, gateway matching/authorization unchanged. No handler, SQL or audit writer added. |
| Manifests, generated artifacts and verification (supporting reads; not modified) | `languages/spec-driven-development.md`, `languages/implementation-execution.md`, `languages/testing-workflows.md`, `languages/agent-harness.md`, AGENTS §§4, 8, 11–12; `mise.toml` | Four owner manifests, workspace dependency/lint policy and lockfile, `gen_schemas.rs`, `gen_ts_types.py`, codegen/family/dependency tasks and supplied logs inspected. No new dependency/feature/gate/hand-edited artifact. |

Selected references were read for their full applicable guidance, including maintainer style, spec-driven development, bounded execution, Rust core, errors, harness, testing, vocabulary, architecture constraints/patterns, Vala, evaluation, drift and telemetry. Security posture, design and doctrine were read as governing authorities. Bifrost execution/storage, SQL, HTTP handler registration, Python/PyO3, TypeScript/N-API and UI styling rules have no changed implementation surface here. Their future integration obligations are not certified by this report.

## Applicable rule results

| Rule / governing source | Evidence in candidate | Result |
|---|---|---|
| Pure foundational contracts; no IO/async/PyO3/runtime dependencies — AGENTS §§2–3, 6–7; architecture constraints | Entire new `card/blocks/*` and `verification/execution.rs` use pure values/serde/schemars; runtime Instant/async-trait are in Vala. `wyrd-spec/Cargo.toml` is unchanged. | PASS |
| Shared media stays at its narrow Skald owner — AGENTS §§2–3; patterns | `skald-spec/media.rs:149` owns MediaFormat; `prompt.rs:467` reads existing derived media kinds. Wyrd contract users import that owner. | PASS |
| Typed durable identity and references — AGENTS §§2, 4, 9; design doctrine 19 | SlotName and ExecutionId use existing ID macros (`ids.rs:131,385`); results use CardRef/CardUid/RunId/TraceId/Sha256Hex. AgentCall and CodeSource use existing InlineableRef/Ref. | PASS |
| Fourteen Card kinds; no Tool Card/compatibility noun — design/doctrine, AGENTS §2 | BuiltinTool at `card/agent.rs:61` is a closed runtime-name vocabulary; no envelope/kind alteration or alias. | PASS |
| Clear invariant owners and synchronous pure computation — AGENTS §§5–6; Rust core | Validator (`validate.rs:184`) retains spec/resolver/derived lookup state; Walk (`schema_path.rs:43`) owns finite remaining work; Tokens (`reference.rs:178`) owns parsing cursor. Helpers are invocation-local pure transformations. | PASS |
| Reuse existing DAG/comparison/reference/Prompt owners — AGENTS §15; patterns | `validate_dag` delegates to `plan_stages` at `plan.rs:110`; block plans call same owner. `ComparisonOperator::validate` at `operator.rs:591` reuses regex validation; legacy task delegate calls it (`spec.rs:187`). Visit helpers reuse existing inline Agent traversal. | PASS |
| No second live protocol, unapproved runtime or premature cutover — original task staging; design/spec authority order | New modules are exported but live Spec/route/generator roots still select existing Verifier contracts. No second server route, listener, queue, storage or worker owner occurs in diff. | PASS |
| General global/exact scope authorization and attenuation — security posture; AGENTS §§2, 9 | `PermissionScope::covers` `:153` matches exact Tool values; `Permission::validate` `permission.rs:342` requires Tool/Invoke for exact scope; existing covers/intersection consume the same relation (`:305,:980`). Existing wildcard/all semantics remain. | PASS |
| No implied data grant, default Tool grant or caller identity injection — security posture; spec INV-003 | Permission constructors encode only Tool/Invoke; role definitions unchanged; request wire has no trusted tenant/principal; accepted-authority test `request_context.rs:357` proves exact scope and chain survive serialization. Runtime enforcement remains deferred. | PASS within contract scope |
| Live sibling scope handling remains exhaustive/disjoint — security posture | `gateway/batches.rs:169` filters Gateway/Invoke and ignores Tool at `:190`. It does not project tool scope into gateway deployment or model reach. | PASS |
| Mandatory public/private/helper/trait/test rustdoc — AGENTS §16; agent rules | Detailed cumulative inventory below; all new/materially changed items have substantive docs. Two prior formatter omissions now document actual writer failure. | PASS |
| Every changed fallible operation documents Errors; panics/async cancellation explained — AGENTS §16 | Validator methods and helpers, DriftCheck/Reference/MediaFormat codecs, plan_stages and operator delegation have Errors sections; invariant schema expectation and assert/expect-based tests have Panics docs; Vala module documents deadline/drop cancellation and retry ownership. | PASS |
| Stable public error source, local error ownership — AGENTS §4; errors reference | Validation reuses WyrdError::Validation with rule/path; drift retains its error owner. VerificationError derives code/message from existing WyrdError metadata (`execution.rs:386`); no parallel catalog/status implementation. BlockError is an engine-local seam. | PASS |
| Required/null extraction fields and closed authoring shapes stay source-defined — contract/harness rules | `extract.rs:78,105,135` combine required serde deserialization with schemars-required nullable fields; tests refuse omissions/extra fields. VerifierSpec/check/action types deny unknown authored fields. | PASS |
| Bounded untrusted pure schema work — Rust/input-boundary rules; prior FIND-5 | `schema_path.rs:14,50,57,68,109,234` share one budget over node/ref/finite-value recursion. Unknown selector analysis admits; unproven Verdict compatibility refuses. No external reference fetch. | PASS |
| Generated outputs derive from source — AGENTS §§8, 12; agent-harness | New types have JsonSchema or custom schema implementations. `gen_schemas.rs:51,174` still selects live card::verifier::VerifierSpec. No generated artifact changed; supplied current codegen check passes. The gate does not independently certify served OpenAPI/MCP behavior. | PASS for staged scope |
| No raw SQL capability, widened tenant query or transaction ownership violation — agent rules | Actual changed fields/signatures and full gateway helper contain no PgPool/PgConnection/Transaction, TenantConn operation or SQL. No production SQL capability is hidden behind an unscanned path. | PASS (no SQL change) |
| Nonblocking audit/one Scribe writer and SSRF effective-value rules — agent rules/security posture | No audited boundary, durable writer or URL fetch was introduced; pure permission algebra is not a permission decision requiring a new audit sink. No change can bypass existing URL screening. | PASS (no new effect) |
| Rust tests own Rust behavior, inline and credential-free — AGENTS §11; testing workflows | Added tests are in owning source modules, pure or the small Tokio executor test; no Python/Node interpreter, DB/server, provider credentials or heavy standalone test binary added. | PASS |
| Focused proof and no weakened gate/test — AGENTS §§11–12; bounded execution | Current library/focused/Clippy/codegen logs pass. Diff adds positive/negative assertions, no assertion deletion, ignore/allow, sleeps, retry workaround, gate/CI changes or broadening boundary globs. | PASS |
| Source-driven manifests/features and dependency cost — AGENTS §§2, 4; agent rules | No Cargo.toml/Cargo.lock changes, no new feature, dependency, build.rs, custom runtime or permanent checker. Existing schemars 0.8 and async-trait/Tokio are used by their current owners. | PASS |
| Wyrd-native permanent documentation, no task/agent history — agent rules; maintainer guide | New source docs describe domain/operation contracts and invariants, not active task IDs or agent conversations. Historical untouched comments are outside this diff. | PASS |
| Module-scope imports and bare imported signature types — agent rules | `reference.rs:318` and Skald `media.rs:375` have function-local schema imports; new tests also have local imports; `blocks/mod.rs:48` and some helper/schema signatures retain qualified type paths. | FAIL, nonblocking layout only |

The final row has no established behavioral, public-contract, security, tenancy or durability consequence. Under the invoked review skill it cannot block acceptance; no new FIND ID or remediation is requested. This is not a rustdoc exemption.

## Cumulative rustdoc inventory and prior FIND-6 closure

Reviewed complete bodies and adjoining rustdoc, not only public items or Clippy output:

| Surface | Documentation evidence | Result |
|---|---|---|
| `card/blocks/mod.rs` | Module docs define workflow/staging, every declaration/field/variant is explained, serde predicates have purpose, InputSlot/Check/Decide/ScheduledInput methods explain projection, DriftCheck Serialize/Deserialize at `:335,:350` explain flattening and errors. | PASS |
| `card/blocks/reference.rs` | Root/path variants and fields, ReferenceError, every Reference method, Tokens and retained cursor field, all cursor helpers, associated Err and every trait codec/schema function documented. Display `:233–237` now has Errors naming formatter writer failure. Test module and all three tests have docs/Panics. | PASS — former formatter residual closed |
| `card/blocks/schema_path.rs` | Module and BUDGET, admits/fits contracts, Walk/budget, spend/resolve/admits/fits/fits_object/values, types/required/excludes_type, latest `json_type` at `:265` and `literal_name` at `:278`, plus test module/function documented. Boundedness, unknown selector behavior, allOf/finite-value/literal-name limits are stated. | PASS |
| `card/blocks/validate.rs` | ResolvedAgent and fields, Position variants, invalid, all VerifierSpec methods, dependencies/requirement_owners/step_references, Validator fields and all validation methods, schema/output/slot/operator/path/ancestor helpers; each Result-returning function has Errors. Generated schema invariant has Panics. Test module plus parse/rule/with/agent/verdict_schema/judge/review/drift helpers and all tests carry substantive docs; assert/expect paths have Panics. | PASS |
| `card/blocks/extract.rs` | Module, every output/field/variant, schema selection and test module/functions documented; required nullable values and ordering/merged-cell semantics explained. | PASS |
| `skald-spec/media.rs`, `prompt.rs` | New format/error/tuple field/catalog/accessor methods, Display/FromStr and associated Err, serde/schema methods and test module/functions documented. Display `:303–307` now names writer failure under Errors. New Prompt::media_kind describes derived portable/native behavior at `:463`. | PASS — former formatter residual closed |
| `verification/execution.rs`, `ids.rs` | Every new request/result/provenance/locator field/variant, error projection, ID macro documentation, identifier validator Errors and new test module/helpers/functions reviewed. Digests/no-output semantics and delayed result visibility are explained. | PASS |
| `refs/mod.rs`, shared planner/operator | New Visit implementation and all three helpers at `:357,:380,:396,:411` have domain-role docs; blocks test module/test have docs/Panics. `plan_stages:113` describes ordering/unique-ID precondition and Errors; existing validate_dag docs remain valid. New ComparisonOperator::validate and materially changed validate_task_operator document actual regex refusals. | PASS |
| Runtime and Vala additions | Tool variant/constructors, changed Permission::validate docs and added permission/authority tests documented; all Vala seam types/fields/variants/methods, Echo implementation, both tests and test module documented. Module establishes deadline/drop/retry semantics shared by both traits. | PASS |

The pre-existing `operator.rs::validate_regex_pattern` still has no rustdoc; its only cumulative edit narrows `pub(super)` to private, without changing its body or error behavior. This is a non-material visibility reduction at the now-private implementation helper, not a new or materially changed algorithm. The newly public entry point and changed caller both have the required docs. Similarly, unchanged neighboring legacy items are not pulled into a documentation sweep. No pre-existing undocumented item is being offered as precedent for a new item.

**FIND-TASK-001-6: CLOSED.** Original helper/codec/private/test omissions and the R1 residual two formatter Errors sections are all corrected at the actual owners. No placeholder section or undocumented new/material helper was found. Clippy alone was not used to establish this result.

## Other prior-finding source checks

This is standards closure corroboration, not a replacement for the independent behavior/security acceptance reviews.

| Stable finding | Source-confirmed present correction and proof | Standards assessment |
|---|---|---|
| FIND-TASK-001-1 | Shared Tool scope/resource and strict resource/action validation; generic coverage/intersection remains authoritative. Exact/global, invalid names/placements, wildcard/domain separation and accepted-authority tests exist (`permission.rs:1401`, `request_context.rs:357`). | Closure retained within pure contract scope |
| FIND-TASK-001-2 | Ten variants/ALL/name lookup in one BuiltinTool, used in scope/provenance and action validation (`validate.rs:535`); all-builtins test at `:1427` and position/media negatives remain. | Closure retained |
| FIND-TASK-001-3 | Latest fits retains same-node facts after authored allOf (`schema_path.rs:130`), derives missing types from finite values (`:154`), and handles exact literal pattern fields (`:202`). Public fixture `validate.rs:1456` adds the selected positive/negative cases while preserving generated/local-ref and prior incompatible cases, both check positions/resolution paths and arbitrary action output. | Selected bounded correction source-present; no standards expansion into generic inclusion/regex proof |
| FIND-TASK-001-4 | One typed ToolCallRecord carries optional canonical SHA-256/required returned bytes (`execution.rs:315`); same record appears in ActionOutput and StepResult; result and handoff tests cover output/no-output serialization. | Closure retained; no deferred projector required |
| FIND-TASK-001-5 | One finite Walk covers recursion/ref hops, with input/output public cycle and finite-recursion tests (`validate.rs:1586`) plus known-missing tests. | Closure retained |
| FIND-TASK-001-6 | Complete cumulative documentation inventory above, including Errors on both formatters and new R2 helpers. | CLOSED |

No new stable finding is proposed. In particular, numeric-bound strength, string format, arbitrary regular-expression inclusion and a new public schema profile are outside the prior selected correction. They are not converted into repository obligations by the walker's broad summary sentence.

## Open Questions

None affecting the bounded repository-standards verdict.

## Verification Notes

Read the current review's actual logs and subject rather than interpreting green checks as complete source review:

- `library-check.log`: 869 wyrd-spec library tests passed, zero skipped.
- `focused-check.log`: the exact `card::blocks::validate::tests::verifier_spec_requires_check_agents_to_produce_a_verdict` selection ran one test and passed.
- `clippy-check.log`: owner all-features wyrd-spec/wyrd-skald-spec library/test Clippy finished successfully with `-D warnings` per recorded command.
- `codegen-check.log`: source-generation and theme checks passed; subject records unchanged tracked generated outputs and cumulative whitespace proof.
- Prior R1 evidence records 1139 tests across all four owners, nine exact selections, all-features owner Clippy/workspace test compilation, fmt, dependency and codegen checks. Latest source changes touch the bounded schema owner/test and two documentation sections; no manifest/feature/dependency/gate changed.

`mise.toml` confirms codegen verifies consumed source-derived outputs, not served OpenAPI or runtime MCP; family lanes provide broader environment-owned coverage, and the bounded remediation expressly selects owner library proof instead. This reviewer did not rerun Cargo jobs alongside the orchestrator's shared-target verification. No red gate is ignored or attributed away.

Skipped: independent build reruns, live server/provider/SQL/sandbox tests, SDK journeys and broad aggregate gates. Risk: this PASS certifies staged repository standards only; later runtime isolation, authorization/audit enforcement, durable execution and final integrated public-surface journeys still require their assigned proof.
