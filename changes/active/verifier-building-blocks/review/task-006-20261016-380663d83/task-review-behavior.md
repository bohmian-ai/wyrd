# TASK-006 independent behavior review

**Result: FAIL.** The runtime deletion and retained-engine move are coherent, but the required vocabulary check has coverage holes. Two modified public SDK descriptions also still teach the deleted authoring discriminator. Proposed findings below require independent validation; they are not final stable FIND IDs.

## Immutable subject and authority

Read `subject.md` first. All source/Git inspection used `/tmp/wyrd-task006-review-380663d83`, base exclusive `b323327f4cbb690628303d3a6b5249a67af50c9b`, candidate `380663d83f3d120d3833ea807d29b7444929d392`. The complete span includes `f711c6834` and its CLI removal, not merely subsequent renames. No CodeGraph directory exists in the frozen tree. Final HEAD matched; `git status --porcelain` was empty.

Authority: original TASK-006, approved spec r10, AGENTS (especially ownership, complete replacement, verification and §16 documentation), agent rules, spec-driven-development, maintainer style, reference router, current Verifier design/doctrine, and Bifrost namespace/table and evidence boundaries. The current evaluation reference and UI skill were inspected for the affected documentation/UI consumers. R10 supersedes scenario retention and explicitly authorizes the forward development-schema retirement; neither compatibility preservation nor deletion of the owner-required check is recommended.

This is a static acceptance review. No test, build, generator, broad gate, provider call, source mutation, or new diagnostic framework was run. Verification below means inspected committed test source and recorded implementation command results, not a fresh execution claim.

## Caller-to-result and reuse inspection

- CLI: `Cli`/`Command` no longer imports or dispatches Eval. The binary uses the existing clap error path before dispatch; the new loopback-listener test checks both retired forms, help, usage exit 64, and zero connections. CLI error printing was moved intact from the removed output module to `CliBoundaryError::print`; current commands still use the same metadata/output path.
- Contracts: the old Card/Eval contract modules, schemas and unused public error variants are removed. The live `TaskId`, `JsonPath`, comparison catalog and `plan_stages` moved into the existing `verification` owner. Their validation/planning algorithms are retained; only obsolete scenario IDs, task-map adapters and their error conversion disappear. `ContextCapture` retains full/hash/redact serde and runtime semantics. Contract generators and SDK projections consume that owner.
- Retained engine: compared pre/post `blocks/engine.rs`, operators, source seam, helper bodies and tests using old-path/new-path blob diffs. `VerificationEngine::run` still validates evidence, plans, admits evidence through its injected reader, runs bounded stages/deadlines, and aggregates the same requirements. `HostedBlocks::engine` is still shared by direct and queued execution with the accepted caller/evidence, same process pools, trace source, DriftChecks, and route overrides. Changed server/runner/result/SQL consumers are import/type-path substitutions.
- Live helper closure: `assertion::{evaluate_single,evaluate_item_mode}` is the former tasks/assertion implementation; comparison, first type error, empty-array success and failure counts are unchanged. `capture_actual` and hash bytes are the old results helpers. `jsonpath` keeps old JSONPath matching and null/single/array collapse. `walk` retains the same JoinSet lifecycle, completion-error propagation, declaration-order settlement and abort-on-drop behavior; only obsolete ledger iteration consumers disappear. `trace_source::project_spans/project_one` is the former trace-task projection with the same fields. The shared trace fixture is replaced locally while the retained engine assertions remain. No second engine, walker, parser, scheduler or lifecycle owner was introduced.
- Permissions: removed `Resource::Evals`, parser match, helper and built-in grants. String/structured/any_of inputs fail the existing parser. Stored raw test grants reach `wyrd-auth::issuance::permission_set_from_rows`, whose serde decode refuses the entire role; OAuth maps that existing role-corrupt failure to `server_error`, and SDK projection yields the recorded `WYRD_SPEC_500_INTERNAL`. This is the ordinary existing corrupt/unknown-role behavior, not a compatibility handler. Valid unrelated permissions and exact/global Verifier grants remain distinct controls.
- Tables: removed the built-in and closed namespace variant; public SQL/describe/write conversion cannot construct the retired `TableRef`. `BifrostCatalog::list_tables` filters stale registrations through the same namespace owner, while describe/provider use closed typed references and tenant-qualified physical binding. MCP's new test actually inserts a stale result-items registration before listing/describe/query; Rust/Python/TS and CLI compare retired SQL refusal with `vala.absent`, and retain a current-table successful call. The ordinary SQL code is query-invalid-SQL, rather than an invented compatibility 404.
- Upgrade: the new forward migration deletes snapshot-expiration claims referencing retired tasks before the tasks, then tightens the task namespace check. Remaining namespaces/evidence are untouched. The old planning-demands table was already dropped by `20261003000100_drop_forge_planning_demands.sql`; there is no surviving demand table requiring another delete/constraint. Applied migrations are unchanged. Claims' task FK explains the deletion order; no conversion, reader, object erasure, or archival subsystem appears.
- External GenAI: only owned projection names/serialized `evaluation_results` change; all result content fields, validation, canonical `gen_ai.evaluation.result` strings and lossless trace storage remain. P8b asserts the new key and absent old key; the canonical event assertion remains.
- Consumers: Observe evaluation routes/types/projectors and corresponding obsolete tests are removed, navigation and mock references follow current Verifier Cards; surviving drift/trace paths remain. Public generated TS/error sources, Python authored state/testing stubs, N-API testing projections, config fixtures, selector CI/release package names and manifests follow the engine name. Documentation closure has the residual defects described below. No scope expansion, dependency addition or new configurable runtime surface is needed.

## Explicit acceptance matrix

`PASS (regression)` means the TASK-006 diff preserves that requirement; it does not independently complete TASK-001–005 or replace integrated proof. P1–P10 and counts refer to the exact commands and recorded results appended to the original task.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 one declarative Verifier | `card/verifier/mod.rs::VerifierSpec`, unchanged building-block fields; no Eval Card module | spec 758; served contract source | PASS (regression); obsolete SDK prose in BEHAVIOR-2 |
| REQ-002 named slots | retained `VerificationEngine::run/conform`, scope and input contracts | retained engine suite/P9 | PASS (regression) |
| REQ-003 references/graph execution | moved `verification::{ids,plan}`, unchanged scope/drive/walk | retained planning/engine assertions | PASS (regression) |
| REQ-004 preparation | existing Agent/extract/code executor seams and hosted composition retained | retained Agent test assertions; recorded P10 | PASS (regression) |
| REQ-005 checks | assert/trace primitives compared to old bodies; Agent/code/drift unchanged | engine 49 and P9 | PASS (regression) |
| REQ-006 missing/skip/aggregation | unchanged scope gap causes, judge/decide | retained engine assertions | PASS (regression) |
| REQ-007 Prompt/route ownership | `AgentExecutor` and hosted registry/gateway only change imports | retained Agent tests | PASS (regression) |
| REQ-008 bounded tools/workspaces | tool seam unchanged; hosted tool/workspace ownership unchanged | focused P10, prior task proof retained | PASS (regression) |
| REQ-009 code execution | sandbox consumers only change engine type imports | prior sandbox proof, no fresh claim | PASS (regression) |
| REQ-010 evidence/citations | server evidence/extract consumers only path substitutions | P10 and preserved engine assertions | PASS (regression) |
| REQ-011 execution surfaces | shared client verify/submission transport unchanged | P4–P6/P10 | PASS (regression) |
| REQ-012 server/HTTP/MCP | existing verification service/hosted engine preserved | served OpenAPI P7; principals lane | PASS (regression) |
| REQ-013 queue/replay/lifecycle | runner/authority/results only path changes | P10; full lifecycle suites deferred | PASS (regression) |
| REQ-014 drift/reactions | drift checks, baselines, dispatch/runner semantics unchanged | P9/P10 plus unchanged assertions; full drift lane deferred | PASS (regression) |
| REQ-015 attributable result | moved ContextCapture, result contract/SQL unchanged | engine capture assertions; P10 | PASS (regression) |
| REQ-016 examples | no example runtime redesign; consumers compile against renamed shared API | recorded `check:examples`; external app tests not rerun | PASS (regression) |
| REQ-017 consumer/docs completion | generation/UI/stubs close most surfaces; WyrdState docs retain removed discriminator | codegen/docs/types green do not validate semantic prose | FAIL, BEHAVIOR-2 |
| REQ-018 delete superseded subsystem | CLI dispatch/contracts/scenario/orchestrator/table/resource removed | P1/P2/P4–P7; static caller inventory | PASS |
| REQ-019 repurpose live dependencies | old/new helper and engine comparisons; approved projection naming | P8a–c/P9/P10, engine/spec suites | PASS runtime; current reference path remains a nonblocking naming note |
| REQ-020 no compatibility aliases | closed permission/namespace sets; no route/table/re-export aliases | P1/P2/P4–P7 | PASS |
| REQ-021 unreleased schema cutover | forward SQL deletion order/constraint; typed table refusal/filter | P3/module and MCP stale catalog test | PASS within stated claims-test limit |
| REQ-022 permanent narrowly scoped check | `deps.sh:127–150` ignores whole files and only searches contents | recorded probes don't cover those omitted cases | FAIL, BEHAVIOR-1 |
| INV-001 server/engine/pure contract ownership | existing owners retained; pure moved contracts | dependency/type checks recorded | PASS |
| INV-002 first-class client parity | shared transport, language wrappers, declarations unchanged except names/testing | P4–P6; types/codegen/NAPI | PASS |
| INV-003 tenant/permission boundaries | parser deletion and same tenant-bound catalog/engine owners | P2/P4–P7; exact/unrelated controls retained | PASS |
| INV-004 audit/durability | no audit/outbox control-flow mutation | source comparison, P10 | PASS |
| INV-005 no general runtime/external writes | obsolete workflows deleted; same bounded hosted executors | source comparison | PASS |
| INV-006 honest evidence | same skip/inconclusive/verdict/citation flow | retained engine assertions/P9 | PASS |
| INV-007 resource isolation | same pools, workspace/sandbox, deadlines, JoinSet drop behavior | source comparison/P10 | PASS |
| INV-008 complete replacement/preserve unrelated data | approved deletion; only retired maintenance tasks/claims removed | P1–P7/P3; source comparison | PASS runtime; prose gap BEHAVIOR-2 |
| INV-009 naming-only engine move | exact algorithm comparisons and retained assertions; approved projection exception | P8–P10; engine 49 | PASS |
| AC-001 | contracts/registration only import/name substitutions | original proof remains; full suite not rerun here | PASS (regression) |
| AC-002 | unchanged engine/scope/walk/comparison behavior | retained engine suite/P9 | PASS (regression) |
| AC-003 | unchanged Agent/extract/code hosted behavior | retained assertions/P10; integrated suites deferred | PASS (regression) |
| AC-004 | unchanged sandbox/resource ownership | original proof, no fresh isolation execution | PASS (regression) |
| AC-005 | real public verify/background paths preserved | P4–P6/P10; full journeys at change review | PASS (regression) |
| AC-006 | public stale-role/table negatives reach actual server | P1/P2/P4–P7; prior lifecycle cases preserved | PASS (regression) |
| AC-007 | unchanged scheduled/drift algorithms/result paths | source comparison; full drift lane deferred | PASS (regression) |
| AC-008 | example runtime outside TASK006 write set | recorded check:examples; no new live claim | PASS (regression) |
| AC-009 consumer/generated/docs | generated/served/runtime surfaces updated; two public docs wrong | codegen/types/served/MCP; semantic source comparison | FAIL, BEHAVIOR-2 |
| AC-010 scoped verification | recorded format/lint/types/deps/tenant/docs/examples and P1–P10; ts:format is real task substitute | no broad test demanded at this review | PASS within explicit limits |
| AC-011 narrow permanent check/live closure | helpers retained, but check exemptions exceed refusal literals | script source vs approval; recorded probes incomplete | FAIL, BEHAVIOR-1 |
| AC-012 ordinary public refusals | CLI/parser/catalog closed sets; true server stale role/catalog fixtures | P1/P2/P4–P7 | PASS |
| AC-013 retained verification/drift | retained helper algorithms and assertions | P9/P10; full Bifrost/Verifier at change review | PASS task scope |
| AC-014 standard/generated/served | GenAI expanded projection, same canonical events, no old aliases | P8a–c/codegen/served OpenAPI/P7/types | PASS |
| AC-015 fresh/upgrade schemas | task/claim retirement; planning demands already absent | P3/module; full SQL deferred | PASS task scope, claims not seeded |
| AC-016 aligned prose/check | current reference path and removed discriminator remain; check omissions | docs/deps green insufficient for semantics | FAIL BEHAVIOR-1/2 |
| No second engine/store/scheduler/transport/validator | only retained primitive extraction plus existing parser/catalog/migration mechanisms | cumulative source/caller comparison | PASS |
| No compatibility/history conversion/object cleanup | deletion and one approved migration only | cumulative diff | PASS |
| TASK005 independent PASS prerequisite | latest frozen task-005-r3 verdict says FIX_REQUIRED, not PASS | subsequent source corrections cannot establish independent review PASS | NOT ESTABLISHED; sequencing/evidence limit, not reopened TASK005 finding |

## Proposed findings

### BEHAVIOR-1 — INCORRECT: required vocabulary boundary silently skips positive current fixtures and path-only names

- **Obligation:** REQ-022; TASK006 Scenario 4 explicitly covers owned identifiers/paths and current positive fixtures, with only semantic negative/history/external exclusions; AC-011/016.
- **Location:** `scripts/checks/deps.sh:129–140,149–150` (also global external-syntax exclusion at 147).
- **Producer → consumer:** `rg` emits matching content as `path:line:text`; `eval_exempt` then drops every line in the listed test files based solely on filename. These are substantial current CLI/MCP/SDK/observe tests containing valid successful workflows, not files dedicated solely to refusal literals. For example, a new positive `evals:run` role in `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs` or a positive `vala.eval.result_items` SQL query in MCP `query.rs` is always discarded by the path-only regex regardless of context. Those exact successful workflow fixtures are within the required check boundary. Additionally the initial `rg` pattern selects content only: a newly added `eval.rs` module/path with otherwise benign contents produces no candidate line, so its forbidden owned path is never examined. Excluding every `migrations/**` file also exceeds the wording “applied migration files,” although the concrete full-file and path holes suffice independently.
- **Observable consequence:** `check:deps` can report green while a current positive fixture or owned module path reintroduces the removed model. This is the exact cross-language/string/path case the owner explicitly required the permanent check to prevent. Compilation cannot establish the fixture vocabulary constraint.
- **Sibling/existing owners:** all exemption entries in the same block share this source; inline permission/Card/loader/table rules already demonstrate occurrence-level exclusions. Reuse this single existing boundary block; no new checker framework, additional permanent self-check, dependency or product policy.
- **Smallest safe correction:** restrict exemptions to the actual approved negative/history/external occurrences rather than whole current test files; inspect relevant owned paths as well as file contents in the existing check. Preserve standard GenAI expanded projections, ordinary compute words, external syntax and historical exclusions. Do not delete the required check or blanket-exclude tests/directories to clear it.
- **Focused closure:** temporary/reversible probes in an existing whole-file-exempt mixed test must reject a positive old permission/table/export/import while the real negative literal remains permitted. A path-only prohibited module/fixture probe must fail even with neutral content. Retain the legitimate GenAI/ordinary-word probes and run the owning `mise run check:deps`; remove probes afterward. No broad build or checker-test framework is necessary.

### BEHAVIOR-2 — INCORRECT: modified SDK docs still tell callers to access the removed `implementation` discriminator

- **Obligation:** REQ-001, REQ-017, INV-008 and TASK006's consumer/documentation closure; AC-009/016. This is a concrete invalid public access path, not preference about wording.
- **Locations:** `crates/shared/wyrd-client/src/state.rs:1041–1043`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2807`.
- **Producer → consumer:** shared Rust `WyrdState::verifier` returns `&VerifierSpec`; that struct's actual fields (`card/verifier/mod.rs:50–72`) are description/inputs/steps/requirements/decide/context_capture, with `deny_unknown_fields` and no implementation union. The TypeScript method returns the envelope whose `spec` is that same generated VerifierSpec. The new Rust prose says to match `implementation` to reach “Drift or check”; the new TypeScript prose tells callers its body is `spec.implementation`. Both were modified by substituting the old Eval word while retaining the obsolete model.
- **Observable consequence:** a literal human/agent consumer following public Rust/TypeScript API documentation selects a nonexistent member/discriminator, producing type/compile failures or undefined JS data, rather than navigating the one declarative Verifier. The task specifically requires current consumers and SDK prose to close that replacement.
- **Sibling/existing owners:** the Rust accessor and TS wrapper already project the right runtime contract; generated `VerifierSpec` and the authoring module are authoritative. Python's changed start_bifrost prose correctly removes the old table and needs no related runtime change. No accessor, compatibility field, alternate model or new wrapper is required.
- **Smallest safe correction:** describe the existing Rust pure-spec return and TypeScript envelope's `spec` building-block fields directly; remove the instructions to match/access `implementation` and the kind split. Keep runtime signatures and behavior unchanged. Check sibling current API descriptions for the same removed-field instruction, without changing unrelated prose.
- **Focused closure:** source/API-doc comparison against the existing typed VerifierSpec and current generated TS type suffices; run only owning documentation/type checks affected by the correction. Do not add a trivial prose assertion test.

## Nonblocking notes and evidence limits

- REQ-022 specifically asks to rename the current `architecture/references/domain/evaluation.md` path and its incoming consumers; candidate leaves that path, heading and router route in place. This is a naming/documentation closure gap, but no broken link or runtime loss was established, so it does not independently drive FAIL under this review skill's materiality rule.
- The migration's claim deletion is logically required by its existing FK and uses a precise task-derived selection. P3 does not seed claims, but no faulty query/order was found; this is a proof limit rather than an invented data defect. Demands were already dropped, so no request to recreate their obsolete state.
- P2 has no captured RED. Final parser source plus recorded focused GREEN establishes refusal; do not manufacture a retrospective RED or misstate the evidence.
- Recorded SVG rename work lacks visual overflow inspection. No specific overflow defect is asserted here; UI visual evidence belongs to its owner.
- Full `test:sql`, Bifrost, scheduled-drift and all-language Verifier journey suites are deliberately left to integrated change review by the approved task. Their absence here does not justify running broad suites during static discovery or claiming them green.
- Latest frozen TASK005 R3 verdict is FIX_REQUIRED, and no later independent PASS artifact is present. The task's sequencing prerequisite is not proven; this report neither reopens its old findings nor treats subsequent code corrections as independent PASS.

No source behavior was changed by this review. Only this report was written. Remaining risks are the required-check holes and invalid SDK usage prose above; no runtime regression in the retained engine was identified.
