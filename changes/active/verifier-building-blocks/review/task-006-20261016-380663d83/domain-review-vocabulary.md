# TASK-006 domain review: vocabulary and generated/public contracts

Result: **FAIL** — one proposed finding, `VOC-1`, in the required permanent vocabulary check. No currently published obsolete export or changed external GenAI payload was found.

## Immutable subject and boundary

Read `subject.md` first. All candidate authority/source/Git reads used `/tmp/wyrd-task006-review-380663d83`, HEAD `380663d83f3d120d3833ea807d29b7444929d392`; complete cumulative range is `b323327f4cbb690628303d3a6b5249a67af50c9b...380663d83`, including first implementation commit `f711c6834`. No `.codegraph` exists in that worktree. No peer review or intended verdict was read. No generator, compilation, gate, broad test, live call, source edit, commit, or extra agent was used. Only this report is written outside the immutable subject.

This review owns vocabulary enforcement and generated/public contract naming, including direct GenAI projections, SDK declaration/export closure, served OpenAPI/catalog proof, and current documentation/reference closure. It does not adjudicate engine algorithms, SQL retirement correctness, credential semantics, UI layout, or other reviewers' runtime domains.

## Authority and coverage

Applied revision 10's REQ-017–022, INV-009 and AC-009/011/014/016; earlier requirements remain regression constraints. In particular, REQ-019 approves the exact `vala-verify`, `wyrd_spec::verification`, `ContextCapture`, expanded GenAI result/module/field names; REQ-022 and task Scenario 4 require enforcement over owned identifiers/paths, positive fixtures, current documentation and generated surfaces, with semantic exclusions only. The owner-approved permanent rule exception in AGENTS.md §12 is valid; deleting the rule because of the general ban-check policy would violate this task.

Authorities inspected: pinned AGENTS.md and agent rules; task-review domain role; reference router; spec-driven development and maintainer-style guidance; current Wyrd design/doctrine and their changed passages; architecture-constraints and positioning/vocabulary; evaluation, telemetry/observations; errors, Python stubs, TypeScript, agent-harness references; task and appended evidence. Source inspection follows authority rather than treating green evidence as the contract.

| Boundary / obligation | Source and proof inspected | Assessment |
|---|---|---|
| Existing single check owner, permanent owner exception | `scripts/checks/deps.sh:122–156`; `mise.toml` check:deps; AGENTS §12 and changed architecture-constraints/testing-workflows | Existing owner extended, no checker framework. Owner exception recorded. Enforcement coverage fails as below. |
| Active contents and exclusions | Entire new regex/exemption/rg block; repository-wide content search using that exact pattern; existing negative tests in every exempt file and inline negative fixtures | Present residual matching content is negative, historical/check-definition, or external syntax. No present positive obsolete fixture established. Exclusions nevertheless authorize new positive usages and production names, contrary to required enforcement. |
| Filename / path coverage | `rg --files --hidden` inventory; new check's content-only `rg -n` | Actual current matching paths are expanded GenAI result and `architecture/references/domain/evaluation.md`. No abbreviated current filename remains, but future filename-only names are not checked. |
| Rust contract/public path closure | `wyrd-spec` deleted card/vala Eval modules and schema root; `verification.rs` and moved ids/operator/plan; consuming import diff; Rust SDK public exports; `vala-verify` manifest/root; shared client | Old owner removed, retained contract owners renamed without aliases. Repository content search finds no current old import/export. |
| Generated TS and error projections | `gen_schemas.rs` roots; card-types.ts `ContextCapture`, removed `EvalContextCapture`, VerifierSpec reference; error.rs removals, TS error union and docs error removals; codegen tasks in mise | Source and committed declarations agree on naming. No old generated naming found. Recorded codegen check is supporting evidence; not rerun. |
| Python/TS/Rust SDK closure | Python state/testing stubs and native-testing edits, Python public observe refusal/import tests; TS public index and testing native.d.cts changes; real-time integration tests in each SDK; Rust public export inventory | No retained old export/alias found. Runtime-dependent export and type checks are recorded GREEN, not independently executed here. No requirement to invent SDK bindings for previously Rust-only GenAI typed projections. |
| Served OpenAPI | Removal of obsolete Eval tag in `http/openapi.rs`; `pg_openapi_contract.rs:1302–1443`, five operations/schema assertions and new retired-name assertion; task records restoration-of-tag RED and full `test:principals:integration` GREEN | Actual runtime document is the proof target, not a checked-in schema. New assertion permits `Evaluation` while detecting ordinary abbreviated identifiers. No current stale tag/schema established. |
| MCP catalog | `query.rs:1132–1240`, real transport/list_all_tools and catalog assertion, current-table controls; changed MCP dependencies/imports | Recorded P7 proves real catalog discovery and stale-table refusal. Catalog assertion checks tool-name components, while source/check scans cover other static naming; no actual retired tool found. |
| Expanded GenAI names | `trace/mod.rs`, renamed result file, `gen_ai_span.rs`, P8a/b/c source and recorded GREEN | `GenAiEvaluationResult`, `gen_ai_evaluation_result`, `evaluation_results` replace abbreviations without aliases; serialized old key absent assertion exists. |
| Canonical external GenAI semantics | `attributes.rs` canonical `gen_ai.evaluation.*` constants; result fields/validation and span source comparison; canonical event test | Constants preserved. Mechanical normalization proves span file unchanged apart from approved naming; result file differs only by approved names and a format! line wrap. Scores, explanations, response ids and validations remain unchanged. No lossless ingestion/storage rewrite entered this rename. |
| Current docs and authorities | Full cumulative docs/architecture diff; current reference router/links; queue rustdoc and SDK startup prose; service lists, Audit signal prose | Stale positive abbreviated prose corrected. Existing evaluation reference path remains, discussed below as a non-blocking explicit path-closure omission. No broken current incoming link introduced by a move. |
| Historical/negative/external exceptions | Changes packet and immutable review exclusions; migrations; permission/card/table/refusal literals; shell eval, ast.literal_eval, XGBClassifier eval_metric; ordinary evaluate/retrieval/revalidate/WireValue | Exceptions are required. Proposed correction below preserves them and does not broaden the task into a parser/framework. |

## Proposed finding VOC-1 — required check can accept prohibited positive names

- Classification: **INCORRECT** (required check behavior), medium.
- Violated obligations: REQ-022, AC-011/016, TASK-006 Scenario 4's active-code/owned-path/positive-fixture coverage and semantic-only exclusions. AGENTS §12 explicitly approves this permanent check to protect string-typed surfaces compilers cannot protect.
- Changed source: `scripts/checks/deps.sh:128–150` (whole-file and whole-line exemptions plus content-only selection).
- Root cause: the pipeline rejects matching *lines*, then exempts an entire file or entire line on the presence of one permitted fragment. It does not determine whether other matching text on that line is permitted. It also never evaluates the pathname independent of a content hit. Those are missing behaviors of the explicitly requested existing check, not hypothetical runtime hardening.

Reachable examples were evaluated in a tiny read-only self-check that reads the exact pattern/exemption strings from pinned `deps.sh`, feeds text to the installed `rg`, and applies those exemptions. No pinned file was mutated:

| Candidate rg record | Expected | Actual |
|---|---|---|
| `./sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts:2:const permission = "evals:run";` used as a positive successful grant fixture | Reject | Exempt solely because the file also contains negative tests |
| `./crates/wyrd/wyrd-cli/tests/card_lifecycle.rs:2:const CURRENT_CARD_NAME: &str = "eval-suite";` positive Card fixture | Reject | Exempt entire file |
| `./crates/shared/wyrd-runtime/src/permission.rs:2:pub const EVAL_SCOPE: &str = "evals";` | Reject | Reject (control) |
| Same production declaration followed by `// retired `evals`` | Reject | Exempt entire line because its comment matches the refusal exemption |
| `./sdks/wyrd-sdk-python/tests/unit/cards/model/conftest.py:2:eval_fixture = XGBClassifier(eval_metric="logloss")` | Reject owned `eval_fixture`, preserve external `eval_metric` | Exempt entire line by external API syntax |
| Standard GenAI names and `retrieval revalidate WireValue` | Permit | Permit (control) |

The first two paths already contain many valid positive user journeys and fixtures; they are not negative-test-only containers. No deliberate comment spoofing is needed for that bypass. The mixed external-API example likewise shows an ordinary fixture naming change bypassed by an existing dependency keyword. The production comment case establishes why line-scoped does not necessarily mean match-scoped.

For path selection, a current positive fixture `tests/fixtures/eval.yaml` containing otherwise clean YAML produces no content hit. Likewise an owned module filename `eval.rs` with `pub const ENABLED: bool = true;` produces none. `rg -n PATTERN .` tests contents, and only prints a filename after a content match; it does not test names against PATTERN. The approved task expressly covers owned paths. No permanent extra script is needed to close this.

Observable consequence: `check:deps` can remain GREEN while a new supported positive test fixture or Wyrd-owned identifier/path reintroduces exactly the naming it is required to reject. This finding does **not** claim the current production runtime still exposes those examples, and it does not ask for arbitrary syntax handling beyond the approved naming/exclusions.

Minimum safe correction: keep the one rule and its existing owner. Replace whole-file negative exemptions with narrow exemptions for the actual negative/check/historical occurrences, and limit external-syntax exclusions to the external matched token rather than unrelated prohibited matches on the same line. Avoid a broad `.*` permission to discard an entire mixed line. Evaluate selected current filenames/paths with the same owning rule independently of content. Preserve current semantic exception controls and historical migration/evidence allowance. No general checker framework, dependency, semantic parser, permanent test-of-check, second rule owner, production guard, or runtime compatibility service is justified.

Focused closure proof: repeat reversible temporary probes in a real existing positive region of each currently blanket-exempt language family, a production inline-test file with permitted text and an additional prohibited identifier, a mixed external-API line, and a filename-only positive fixture; each must fail `check:deps`. Existing legitimate refusal literals, GenAI projections/events, ordinary compute/lexical words, shell eval/literal_eval/eval_metric must still pass. These are bounded additions to the task's existing probe evidence, not a request to build a new verification framework.

## Non-blocking source-closure note

REQ-022's final paragraph explicitly says to update the current `architecture/references/domain/evaluation.md` **path and consumers**. The candidate edits one sentence but retains that path and all links (`architecture/references/README.md:58,73`, skill/task/spec references). The existing reference teaches Verifier evaluation rather than a second product, so this omission has no demonstrated behavioral or public-contract consequence and is not independently blocking under the task-review finding threshold. If the naming correction includes the requested move, update current incoming links together and preserve immutable historical evidence. Do not infer that all ordinary uses of “evaluation” are forbidden; the spec explicitly preserves ordinary compute vocabulary.

## Verification limits and final assessment

Evidence records P8a–c, served OpenAPI, P7, codegen, deps, docs and touched SDK checks GREEN. Their relevant assertions and implementation sources were inspected. No new build/generator/gate or live surface was run. The temporary-probe evidence in the task demonstrates the normal nonexempt paths and one appended production-line hit; it does not cover the positive regions of blanket-exempt files, mixed allowed/prohibited lines, or filename-only input above. Read-only self-checks establish the proposed bypasses; the first attempt using the `python` shim could not run, then `/usr/bin/python3` completed the checks. No conclusion relies on the failed shim call.

Final pinned-worktree check: HEAD remains `380663d83f3d120d3833ea807d29b7444929d392`, `git status --porcelain` empty. Required generated/public naming and external GenAI preservation otherwise pass this domain's static review. Proposed `VOC-1` requires independent validation before assigning a final FIND-TASK-006 ID.

Skipped: generators, builds, gates, live calls and unrelated runtime domains, as required for this immutable discovery pass. Risk: the current permanent check overstates its positive-fixture and owned-path enforcement.
