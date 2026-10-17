---
id: TASK-006-R1
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 10
parent_task: TASK-006
remediates: [FIND-TASK-006-1, FIND-TASK-006-2, FIND-TASK-006-3, FIND-TASK-006-4]
---

# Finish vocabulary enforcement and current consumer closure

Route directly to `$wyrd-implement`; UI asset work also uses `$wyrd-ui`. This task packages only the four independently validated causes in this review's findings-validation.md.

## Subject and authority

Approved specification: `changes/active/verifier-building-blocks/spec.md` revision 10.
Original task: `changes/active/verifier-building-blocks/tasks/TASK-006-verification-vocabulary-cutover.md`.
Base exclusive: `b323327f4cbb690628303d3a6b5249a67af50c9b`.
Reviewed candidate: `380663d83f3d120d3833ea807d29b7444929d392`, inclusive span starts f711c6834.

This directory's verdict, validation and focused follow-up supply the adjudicated diagnosis. Nothing has shipped. No new migration, compatibility alias, historical conversion, recovery service or object cleanup is needed. Preserve the already implemented forward development-schema retirement and runtime deletion/move.

## FIND-TASK-006-1 — Required check bypasses its approved boundary

REQ-019/022 and AC-011/014/016 require the one existing owner to protect current content and owned paths while permitting narrow semantic exceptions. In `scripts/checks/deps.sh:127–150`, content hits are discarded by whole-file or whole-record exclusions; no filename-only candidate is inspected; the pattern also misses expanded product-shaped owned names. The served matcher at `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs:1406–1442` has the same expansion hole.

Concrete reachable counterexamples: a positive old import or successful evals grant inside the existing multipurpose SDK/CLI/MCP test files is blanket-exempt; `eval_fixture = XGBClassifier(eval_metric="logloss")` loses its prohibited owned name because the same line includes a permitted external token; a neutral-content eval.rs/eval.yaml path produces no content hit; EvaluationRun/evaluation_runs are universally missed. Current recorded probes exercise ordinary nonexempt locations, not these gaps. The defect is required check behavior, not an allegation that the current production runtime exposes those APIs.

**Selected correction:** extend the existing deps block and served assertion only. Cover current owned paths independently of content, and exclude only actual permitted occurrences rather than hiding whole multipurpose files or unrelated hits on a mixed line. Detect clearly product-shaped expanded owned reintroductions within the same rule. Reuse installed rg/shell and the current diagnostics/probe procedure; introduce no checker framework, dependency, second owner or permanent test-of-check. Preserve explicit negative/check/historical inputs, applied migrations, standard GenAI projections and content, ordinary computation including MetricEntry.is_evaluation, external syntax and unrelated lexical matches. This is not a universal ban on the word evaluation or authorization to invent a semantic classifier.

**Proof:** reversible probes in existing formerly exempt positive Rust/Python/TS targets, a filename-only source/fixture, mixed permitted/prohibited records, and expanded owned product names must fail. Their actual negative literals and all legitimate controls must pass. Remove probes before final verification. Exercise the existing served-document seam with an expanded forbidden owned contract while retaining descriptive Evaluation prose and actual allowed schemas. Do not rename MetricEntry or change its meaning to satisfy the check.

## FIND-TASK-006-2 — SDK prose directs callers to a removed member

`crates/shared/wyrd-client/src/state.rs:1041–1043` says to match implementation for Drift/check; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:2807` says spec.implementation. Actual Rust returns &VerifierSpec, while TS returns its envelope. VerifierSpec has inputs/steps/requirements/decide/context_capture, not an implementation union. Following the modified public instructions yields invalid member access. This violates REQ-001/017 and AC-009/016; green typing does not check prose.

**Selected correction:** delete the obsolete access instructions and describe the already implemented pure-spec return versus envelope.spec building-block fields. Check sibling current accessor descriptions for this same false instruction. Preserve signatures, runtime, schemas, errors and accessors. No compatibility field, dispatch change, wrapper or new prose assertion test.

**Proof:** compare descriptions against `wyrd-spec/src/card/verifier/mod.rs:50–72` and generated VerifierSpec/RegisteredVerifierCard types; normal Rust/TS hygiene suffices. No SDK rebuild, generation or full journey solely for prose.

## FIND-TASK-006-3 — Complete only the validated changed-item rustdoc

AGENTS §16 explicitly makes missing substantive rustdoc, Errors and applicable Panics contracts blocking. The retained algorithms are correct but their extracted/narrowed owners and adapted fixtures omit these item contracts. Complete this bounded inventory in place:

- `vala-verify/src/jsonpath.rs`: extracted match extraction and zero/one/many normalization helpers; new narrow test module, adapted four tests and local static-path helper error/panic contracts.
- `vala-verify/src/assertion.rs:119–164`: hash Errors and accurate serde_json byte semantics; narrowed capture test module and adapted three tests' panic conditions. Preserve hash bytes; do not substitute JCS.
- `vala-verify/src/error.rs:18–46`: named fields' payload roles and accurate owner prose. The type is exported; zero JSONPath matches normalize to null. Preserve enum/error mapping.
- `vala-verify/src/blocks/engine/tests.rs:22–31` and `tests/integration/blocks_agent.rs:31–54`: reconstructed span and locally extracted tid/structured_prompt fixture panic invariants.
- `wyrd-spec/src/verification/plan.rs:246–346`: adapted module/helper Errors/Panics and seven adapted graph tests' actual assertion/expect/panic contracts.
- `wyrd-spec/src/vala/trace/mod.rs:2071–2095`: extended roundtrip test's new-key/no-alias intent and panic contract.

**Selected correction:** documentation only, explaining actual workflow role and retained failure/normalization/panic behavior. Preserve all algorithms, assertions and error behavior; add no checker or lint allowance. Explicitly exclude byte-equivalent moved ID/operator tests, import-only callers and unrelated crate-wide documentation. Source review of the listed items proves closure; format/lints support it. No new runtime test solely for rustdoc.

## FIND-TASK-006-4 — Retire obsolete current reference workflows

The product ledger at `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md:3–7` supersedes styling only and retains route/IA/state authority. Its C-10 and O-08/O-09 entries (`:184–194,309–327`) still prescribe deleted /observe/evaluations routes, six-task/workflow definitions and task drawers under new Verifier labels. Corresponding cards.svg/observe.svg/mobile.svg/golden-O-09.svg artboards and retained Service references carry the same unsupported model. Runtime routes/projectors were correctly removed, so the current reference points contributors at absent capabilities instead of the approved building-block contract (REQ-017/018/022, AC-009/016).

Offline render evidence also confirms the retained Service SM-01 light/dark labels Verifiers and Drift overlap after expansion; runtime already removed the extra investigation entry. Existing source coordinates and /tmp render paths are in domain-review-ui.md and findings-validation.md. This is an artifact finding, not a runtime Svelte overlap claim.

**Selected correction:** remove obsolete inventory/detail/task-workflow artboards and their positive ledger/link entries through this existing reference package. Retire obsolete golden O-09 rather than redesign it. In retained Service/M-02/SM-01 references, remove the extra investigation item and reuse runtime's existing four-signal navigation; use the already selected current Verifier Card destination for surviving check links. Keep unaffected artboards/current Verifier/Changes content and runtime behavior, updating affected counts/golden descriptions consistently. Correct surviving malformed ordinary-computation words locally while editing those assets. Add no replacement screen, compatibility route, scenario model, design system or historical-asset machinery.

**Proof:** inspect remaining current positive routes/model descriptions against actual routes; view retained changed artboards in both themes and existing mobile widths. Source/XML and narrow render review suffice for asset-only edits. If executable UI code changes, retain the exact existing Observe/Service controls and applicable UI checks below. No new screenshot framework or backend journey.

## Acceptance and verification scope

| Finding | Acceptance evidence |
|---|---|
| FIND-1 | Required formerly missed content/path/expanded-product probes fail; legitimate negative/standard/ordinary/external controls pass; existing deps check and served assertion pass after probes removed. |
| FIND-2 | Public Rust and TS descriptions match the actual returned shapes; runtime/types unchanged. |
| FIND-3 | Listed item contracts complete and accurate; unchanged moves excluded; bodies/assertions unchanged. |
| FIND-4 | Obsolete reference routes/workflow panels gone; retained destinations supported; retained mobile labels separate in both themes. |

Run only the owning checks for the actual write set:

```bash
mise run check:deps
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload)'
mise run test:principals:integration
mise run fmt
mise run lints
mise run ts:format:check
mise run ts:lints
```

The exact served selection supplies focused iteration; its existing principals lane is required by served-OpenAPI policy. Run docs:check if docsite/current reference links change. No codegen or SDK journey is needed for the selected prose-only accessor changes. No full gate, full SQL/Bifrost/Verifier journey suite, live sweep or external example rebuild is required for this remediation.

For asset proof, use the existing installed offline browser and view its PNG, retaining the fallback-font limitation:

```bash
mise exec -- /usr/bin/google-chrome --headless --no-sandbox --disable-gpu --disable-background-networking --host-resolver-rules='MAP * ~NOTFOUND' --hide-scrollbars --window-size=956,2136 --screenshot=/tmp/task006-service-mobile-after.png "file://$PWD/crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/service/mobile.svg"
```

Inspect SM-01 light/dark with view_image; repeat for retained M-02 using product/mobile.svg's existing sheet dimensions (956×9894), and inspect each retained changed artboard. No render is needed for a deleted obsolete panel. This does not prove unavailable remote-font metrics or runtime interaction.

Only if executable UI files change, run the owning check/build and exact relevant controls (they start their own mock dev server; no backend wrapper):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t '^overview restores the shared scope and carries it into every signal link$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts -t '^drift/verification intelligence keeps subject, version, threshold and scope-preserving Observe links$'
```

Record exact probe/check results and distinguish static proof from execution. Correct the task evidence note: ts:format:check exists; prior ts:format was a write-mode run, not check-mode execution. Do not manufacture P2 historical RED or add speculative historical recovery/cleanup for the disclosed P3/projection-debt notes. Preserve user edits and prior reports. TASK-005 still needs its own independent PASS; this task does not grant it. Independent TASK-006 re-review alone can accept this remediation; integrated broad proof remains change-review work.

## Implementation Evidence

Commits: `78ee7df6b` (FIND-2/3), `8d045080c` (FIND-4), `da955da10` (FIND-1 and the TASK-006 `ts:format:check` correction).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1 formerly missed content fails | `scripts/checks/deps.sh` vocabulary block: whole-file and `.*` line exemptions replaced by exact `path:content` refusal/check-input records (line numbers dropped, so any new or edited line is rechecked); external `eval "$`, `literal_eval(`, `eval_metric=` stripped token by token before rechecking | Reversible probes, each RED: positive import in MCP `query.rs`, successful grant in CLI `cli.rs`, grant in Python `test_verify_in_real_time.py`, import in TS `observe.test.ts`, `EVAL_SCOPE … // retired \`evals\`` in `permission.rs`, `eval_fixture = XGBClassifier(eval_metric=…)` | PASS |
| FIND-1 filename-only paths fail | Same block scans `rg --files` paths with the same vocabulary | Probes `vala-verify/src/eval.rs` (neutral content) and Python `tests/fixtures/eval.yaml` RED | PASS |
| FIND-1 expanded product names fail | `eval_product`: `Evaluation(s)`/`evaluation(s)_`/`EVALUATION(S)_` + Run/Suite/Task/Job/Dataset/Report/Record/Workflow, `/evaluations` route, `evaluations:` permission, `vala.evaluations` namespace | Probes `pub struct EvaluationRun;`, `"evaluation_runs"`, `/t/acme/observe/evaluations` RED | PASS |
| FIND-1 legitimate controls pass | Score/Result excluded from nouns; prose with a space is not a product name | Controls GREEN: `is_evaluation evaluate_operator retrieval revalidate WireValue`; `GenAiEvaluationResult evaluation_results gen_ai.evaluation.score.value`; `Evaluation results … CEL evaluation runs`; `XGBClassifier(eval_metric=…)`; `ast.literal_eval(…)`; `eval "$(mise activate bash)"`. Existing refusal literals pass via exact records; `mise run check:deps` GREEN after probes removed | PASS |
| FIND-1 served assertion | `pg_openapi_contract.rs` `retired_vocabulary` helper + `EVALUATION_PRODUCTS`: same abbreviation boundaries plus expanded product nouns and `/evaluations` | Temporary 404 description probes: `EvaluationRun`, `evaluation_runs`, `/t/acme/observe/evaluations` each FAIL with the snippet; descriptive `Evaluation … CEL evaluation runs … GenAiEvaluationResult evaluation_results` PASS; exact test and `test:principals:integration` GREEN after revert | PASS |
| FIND-2 accessor prose matches shapes | `wyrd-client/src/state.rs` (returns `VerifierSpec` blocks), TS `index.ts` (`spec.inputs/steps/requirements/decide/context_capture`), sibling Python wrapper `sdks/wyrd-sdk-python/src/state/mod.rs` | Compared with `card/verifier/mod.rs` `VerifierSpec`; signatures unchanged; `lints`, `ts:format:check`, `ts:lints` GREEN | PASS |
| FIND-3 listed rustdoc complete | `vala-verify` `jsonpath.rs`, `assertion.rs` (serde_json `preserve_order` bytes, not JCS), `error.rs` (exported, zero matches → null, field docs), engine `tests.rs`, `blocks_agent.rs`; `wyrd-spec` `verification/plan.rs`, `vala/trace/mod.rs` roundtrip | Source review; bodies/assertions unchanged; moved ID/operator tests untouched; `fmt`, `lints` GREEN | PASS |
| FIND-4 obsolete references retired | `brand/renders/product`: C-10, O-08, O-09, M-06 artboards and ledger entries removed, `golden-O-09.svg` deleted; check links → Verifier Card; investigate rows → Logs/Metrics/Traces/Drift; counts updated; malformed words fixed | No `/observe/evaluations` left in package; remaining `/observe/*` routes exist under `src/routes`; SVGs parse; headless renders of SM-01/M-02 (both themes), O-01, C-09, S-04/S-05, golden O-04 show no overlap | PASS |

Commands: `mise run check:deps`, `mise run fmt`, `mise run lints`, `mise run ts:format:check`, `mise run ts:lints`, `mise run test:principals:integration` (22 `pg_openapi_contract` tests pass), exact `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::verification_contract_publishes_five_typed_operations_and_the_local_upload)'`, `git diff --check` — all GREEN. All probes reverted before final runs.

Non-goals held: no migration, alias, runtime, schema, signature or executable UI change; MetricEntry untouched; no checker framework, dependency or permanent probe test. Docs-site unchanged, so `docs:check` not run; UI check/build/vitest not run because no executable UI file changed.

Limits: renders use fallback fonts (remote fonts blocked) and prove no runtime interaction. Product-noun list is a bounded heuristic, not a semantic classifier. Out of scope and left as is: C-08 Workflow Card still describes ordered evaluation stages, and Experiment E-07/E-17 reference `?section=evaluations` (no Experiment workspace exists in the app).


## Spec revision 11 note

The owner removed the repository-wide vocabulary scan from `check:deps` (`10fa9269c`) and approved spec revision 11, which drops the permanent check from REQ-022/AC-011. Evidence above for the `check:deps` vocabulary block and its probes is historical; the served OpenAPI assertion in `pg_openapi_contract.rs` (`retired_vocabulary`) and review are the remaining enforcement. After removal, `mise run check:deps` and `mise run docs:check` pass.
