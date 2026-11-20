# TASK-005 independent findings validation

Behavioral verdict: **FIX_REQUIRED**.

Immutable subject: base `89d9638d4986d8ed833bfc99cd4ce28007afdb8b` → candidate `cb6269a5143cf95210e88412f71cd8972080ad3d`, inspected from `/tmp/wyrd-task005-review-cb6269a51`. Contract: approved SPEC-verdict-response-mapping revision 7, REQ-015–020, original TASK-005 and its TASK-004 carryover. Candidate HEAD and clean source status were checked. No reviewed source changed.

## Independence, authority and proof

This validator independently read the approved requirements, candidate source/cumulative diff, relevant root and nested SDK/docs instructions, hosted-resource/Verifier architecture, testing and maintainer guidance, then reconciled `task-review-behavior.md`, `task-review-invariants.md`, and `task-review-repo.md`. The assigned primary, lifecycle/resource specialist and public-contract/provider-error specialist cover the affected boundaries. No additional specialist is required for the retained findings: the lifecycle specialist traced cancellation, concurrency, tenancy and resource accounting, while the public-contract specialist traced refusal sanitization and result access. There is no migration or new durable storage boundary. The detached worktree has no CodeGraph index; source was read there, rather than from the newer main checkout.

The current request expressly carries the maintainer's decisions dropping the L1 benchmark and replacing the held-out retrieval experiment with the live OpenAI evidence judge. Both are accepted task-specific overrides, despite older spec/task paragraphs. Neither is a finding. Wait inventory remains applicable. The live result proves the one configured passage-selection case; it supplies no general retrieval-quality claim.

Recorded execution summaries and actual assertions were inspected, including the final shared-mock consumers and owner tests reported by the maintainer. No Cargo build, test suite, paid provider call or code generation was rerun by this validator. The invariant specialist additionally ran a narrow source-extracted traversal self-check; it is not a full tool journey. The retained implementation defects below follow exact source/ownership paths, rather than presumed test failures. Broad integrated lanes remain change-review work.

## Reconciled acceptance matrix

| Requirement / scenarios | Implementation and assertions / execution evidence | Verdict |
| --- | --- | --- |
| REQ-015 / 9 | Visible five-Agent/one-Verifier/one-Service/one-call smoke, explicit mock/live fixture, real JPEG, five outcomes and final verdict; task records mock/live passes and model identities. OpenAI evidence judge and canonical line-3 assertion match the newer direction. Decisions still uses custom labels and an outcomes map. | **FAIL**, FIND-TASK-005-1. |
| REQ-016 / 2, 4–5 | Shared AgentCheck boolean/validation, client and three SDK projections; Python connected `test_required_citation_needs_one_accepted_reference` covers absent/false, empty/rejected/whole/precise/mixed references and each authored outcome. Python/TS transform refusal tests and engine accepted-count enforcement inspected; corresponding task results recorded. | **PASS**. No separate fresh Rust/TS connected boolean read-back run is claimed; their generic shared authoring projection was traced. |
| REQ-017 / 1–3 | Engine `classify` and hosted evidence reader; exact ready/line-2 Python regression, mixed/unreachable/whole/required and retryable/busy engine assertions, actual reader-format/PR/Artifact cases. Task records focused passes. Known-invalid references do not swallow later transient failures; whole-source validation avoids location extraction. | **PASS**. Disclosed magic-byte fixtures prove reader/sniffing behavior, not new decoder support. |
| REQ-018 / 4–5 | Nullable/absent whole-source contract, rejected metadata, historical exact round trip, existing outcome recording and data conversion; inspected SDK/MCP immediate and retained results, UI accessible accepted/rejected lists. Recorded SDK/MCP/UI/type/lint checks support these claims. Maintained references still prescribe the old workspace model citation contract. | **FAIL**, FIND-TASK-005-5; canonical implementation/read-back otherwise supported. |
| REQ-019 / 6–7 | Connected evidence search/read and extracted PDF page journeys; owner assertions cover shared extractor coordinates, selected JSON visibility, stale-view invalidation, scratch exclusion, continuation/result cuts, unsupported views and host busy. Task records focused passes. The directory walk cut is lost in evidence mode; transient input units also lose their resource charge. | **FAIL**, FIND-TASK-005-3, FIND-TASK-005-4, FIND-TASK-005-5. |
| REQ-020 / 6, 8 | Named reference schema, actual returned IDs, canonical final SDK citation; Python invented/expired/mixed-reference errors, parallel collection isolation/fresh scope/shown-only unit assertions, reference grow/refuse/release test. Final shared-mock consumers include the new Python building-block case; caller reports owner tests passing. Raw compatible schemas are also specialized, outside the named-format boundary; materialized input units are uncharged during consumption. | **FAIL**, FIND-TASK-005-2, FIND-TASK-005-4, FIND-TASK-005-5. Scenario 8 lacks its own appended evidence entry; no dedicated focused execution transcript is claimed. |

Existing malformed data/output/provider-error handling retains its Agent/engine owner. Concurrent canonical slot narrowing has an unchanged-Card/concurrent-request assertion; requiredness remains engine policy and does not require a schema-cache policy discriminator. Tool-free/code/native transforms and general Agent steps retain their ordinary roles, except for the supported raw-schema check regression retained below.

## Validated findings

### FIND-TASK-005-1 — DRIFT: the Decisions example uses the rejected custom-label shape

Discovery B1: **CONFIRMED**. The repo report's initial table described both native examples as canonical; that statement disagreed with candidate source. The primary report and the source establish the finding.

- **Requirement:** REQ-015 expressly requires Jev and Decisions canonical Choice labels and the same ChoiceTransform without outcomes maps. TASK-005's carryover paragraph requires reconciling the existing source against that shape. The current request changes the OpenAI evidence judge and verification obligations, not the Decisions example contract.
- **Location:** `sdks/wyrd-sdk-python/tests/smoke/test_verification_providers.py:87` (Noul/Choice/Score questions), `:93` (custom labels), `:145` (transform outcomes map).
- **Starting state and trigger:** a developer reads/runs the delivered five-provider example in mock or live mode.
- **Expected:** the image check presents canonical `passed`, `failed`, `inconclusive` choices and uses an ordinary ChoiceTransform selecting that question, like Jev.
- **Actual/proof:** the source asks ancillary Noul/Score questions, selects `next_action` with `wipe_with_cloth`, `dispatch_janitorial`, `ready_for_meeting`, and maps those labels through an outcomes dictionary. Passing the smoke demonstrates this different artifact. Historical source commits mentioned by the task do not override the task's explicit reconciliation instruction.
- **Consequence:** the agreed delivered example and its simple shared native Choice workflow are absent, even though its different live workflow passes. This is an explicit artifact requirement, not a preference for fewer questions.
- **Smallest correction:** restore the existing Decisions question/transform and affected shared-mock answer selection to the approved canonical Choice example. Preserve the real photo, five visible Agents, single verification call, explicit live fixture and newer OpenAI evidence judge. Rerun only this mock/live smoke. No provider-runtime redesign is needed.

### FIND-TASK-005-2 — REGRESSION: raw Verdict schemas are rewritten into reference mode

Discovery B2 and REPO-01: **CONFIRMED**, consolidated as one cause.

- **Requirement:** REQ-020 limits specialization to direct **named-Verdict** checks. TASK-005 explicitly preserves raw behavior; REQ-014 preserves existing workflows outside approved exceptions.
- **Location:** `crates/vala/vala-verify/src/blocks/agent.rs:472` (by_reference gate), `:381` (schema edit), `crates/wyrd-spec/src/verification/execution.rs:1669` (`cite_by_reference`).
- **Starting state and trigger:** a supported direct Agent check uses a raw canonical JSON schema containing `$defs/Citation`, a workspace and fs.read/search. The provider returns its authored canonical slot/locator response. `validate.rs:898–905` admits compatible handwritten schemas; its `verifier_spec_requires_check_agents_to_produce_a_verdict` test explicitly admits the canonical schema as raw input.
- **Expected:** preserve that raw canonical contract; only the named format adopts reference selection.
- **Actual/proof:** Role::Check marks any no-transform check as `verdict`; `by_reference` checks no authored-format identity. `bind` rewrites `$defs/Citation` to the closed reference-only object, and the ordinary Agent validates against that altered schema. Previously valid canonical citations are now refused/repaired. Inline equivalent raw schemas escape this rewrite because only the generated definition name is recognized.
- **Consequence:** existing supported workspace checks change response protocol and can become inconclusive based on private JSON-schema layout.
- **Smallest correction:** require the authored named Verdict format at the existing executor specialization boundary; leave raw schemas and their canonical responses alone. Add one focused executor regression using a raw canonical schema with `$defs/Citation`, workspace/fs.read, canonical output, plus the existing named reference-mode assertions. Reuse current schema/Agent owners; no new resolver or schema-normalization framework.

### FIND-TASK-005-3 — INCORRECT: evidence search reports an omitted traversal as complete

Discovery INV-1: **CONFIRMED**.

- **Requirement:** REQ-019 requires distinguishing cuts/skipped evidence and forbids implying exhaustive search after truncation.
- **Location:** `crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:1564` (receives cut), `:1592` (drops it in evidence branch), `:1661` (returns false).
- **Starting state and trigger:** an allowed `/work` directory exceeds `walk_limit`; search evidence mode for a term not found in the visited prefix, without filling the output-byte budget. At an existing supported/tested 1024-byte result limit, 65 empty files exceed the 64-entry walk ceiling. One code.run can create them; this does not require exceeding tool-call or scratch-content budgets. A later extraction may hold the sought evidence.
- **Expected:** `truncated: true` discloses that some files were not searched.
- **Actual/proof:** `walkdir` returns `(work, cut)`, but `search_evidence` receives only candidate paths/matcher/references and normal completion always sets false. `unavailable` counts visited files only. The specialist's source-extracted helper self-check returned 64 of 65 valid files with cut=true; independent source tracing verifies the lost flag and final false response. This is not a claim that a full tool regression was run.
- **Consequence:** a judge can treat an empty result as exhaustive while the evidence is in an omitted file.
- **Smallest correction:** propagate the existing cut through evidence search at Workspace. Add one owner test exceeding walk_limit with an empty or non-full match result; assert true, while preserving result-byte cuts, unavailable count, file mode and shown-only reference issuance.

### FIND-TASK-005-4 — VIOLATION: materialized input evidence units outlive their memory admission

Discovery INV-2: **CONFIRMED**.

- **Requirement:** REQ-019–020 retain existing admitted-resource limits; `architecture/wyrd-design.md` hosted capacity requires materialized memory to remain charged while in use, shared across verification and other workloads. The approved workspace design also requires existing memory charges for evidence backing.
- **Location:** `crates/wyrd/wyrd-server/src/verification/hosted/workspace.rs:1744–1760` (selected JSON), `:1768–1778` (ordinary input units), consumers `:1463` and `:1638`.
- **Starting state and trigger:** ordinary mounted text/JSON inputs fit the ledger, and admitted fs.read/search evidence calls materialize and consume their units; concurrent calls are supported.
- **Expected:** owned unit text/coordinates stay charged until the consumer releases them, and capacity exhaustion takes the existing busy-stop path.
- **Actual/proof:** `units_of` locally charges source bytes in `_held`, allocates owned strings/citations through `units`/`cited`, returns `Arc<[Unit]>`, and drops `_held` as the function returns. read_evidence/search_evidence then retain/iterate that Arc with no accompanying charge. `Invocation::blocking` holds CPU/work tracking, not memory. A no-match search issues no references, so reference charging cannot cover it. Concurrent callers may reuse capacity while prior full unit collections are still alive; many short units also own substantially more structure than their input byte count. By contrast, registered extracted views explicitly retain `_memory` beside their units.
- **Consequence:** process admission undercounts active hosted evidence allocations and can admit work beyond the shared budget. No OOM observation is claimed; Rust ownership/drop and the missing downstream charge establish the resource invariant violation.
- **Smallest correction:** keep the input-unit allocation under the existing ByteLedger for the full read/search consumer lifetime, accounting for the materialized representation through existing admission/busy machinery. Add a focused owner test retaining an input view and verifying its charge persists until drop, with a capacity-refusal case. Preserve exact coordinates and selected-value visibility; add no quota, pool or registry.

### FIND-TASK-005-5 — INCORRECT: maintained reference instructions describe the old workspace citation contract

Discovery REPO-02: **CONFIRMED**, bounded to false operational instructions rather than optional exposition.

- **Requirement:** REQ-018–020 require truthful projected tool arguments, reference selection and canonical final results; docs AGENTS requires shipped facts and reference options.
- **Location:** `docs/src/content/docs/reference/agent-tools-and-authorization.md:20–23`, `:50`; `docs/src/content/docs/reference/verifier-card.md:180`.
- **Starting state and trigger:** an author follows the existing Agent-tools/Verifier references to configure a named-Verdict workspace judge declaring fs.read/search.
- **Expected:** distinguish default file mode and `view: evidence`, returned attempt-local references and the model's `{reference}` items, while final results remain canonical. Existing extraction provenance/invalidity rules must not be misrepresented.
- **Actual/proof:** the tools reference says read/search serve raw lines and unconditionally claims nothing is parsed unless fs.extract runs, omitting the newly shipped view argument. The edited Verifier reference still instructs named-Verdict models to cite bare slots without the workspace exception. Actual descriptors at `hosted/tools.rs:759–803` expose parsed original evidence/reference results, and the named workspace outgoing schema rejects canonical slot/locator items.
- **Consequence:** the maintained instructions produce the wrong response contract for the newly delivered supported workflow and conceal its primary argument.
- **Smallest correction:** update only the existing reference sections for file/evidence modes, returned references versus canonical recorded citations, evidence availability and extraction freshness/refusal. Verify content against the corrected named/raw executor gate and current descriptors, then run affected existing docs checks. No new guide, framework or example suite is necessary.

## Wait reconciliation and verification limits

No new PostgreSQL, Scribe, object-storage or Vault wait category was found in the changed production paths. The moved `reader.validate(&asked).await` in `engine.rs::classify` retains the existing pinned-evidence reader boundary: File/Artifact metadata is resolved through existing execution admission and precise source reads use that existing admitted evidence path. Continuing after a known-invalid citation can visit a later source previously skipped by first-invalid abort; REQ-017 explicitly requires that continuation and its transient-error behavior. Whole-source citations skip location extraction. Local metadata/read/hash/extraction runs on the existing tracked blocking route. Reference issue/resolve is synchronous, no new lock spans IO, session settlement is unchanged, and result/audit recording stays asynchronously staged at its existing owner. The benchmark waiver makes no measured-performance claim.

The scenario-8 task evidence omission is disclosed, not elevated to a code defect or a demand for unrelated suites: the actual connected assertions exist, the final 140-pass shared-mock run includes their file, and the current maintainer summary reports relevant owner tests. Likewise no unsupported claim of codegen/napi drift is retained merely because this review did not rerun generation. Confirmation here does not certify a new clean candidate-wide test run.

TDD: **VIOLATED**, historical and nonblocking at the maintainer's direction. Preserve the task's recorded production-before-test cases (including schema, UI, workspace/Origin and extraction/invalidation), without demanding retroactive RED evidence or production churn.

No implementation, commit, push, merge or deployment occurred. Duplicate-upload SQL behavior and UI-dependent bootstrap checks remain outside this review's correction boundary.
