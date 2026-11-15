# TASK-001 behavior review

Overall result: **FAIL**.

## Immutable subject and approach

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; base `443a79337fe33dfbfe9ccc64e01d10369d469131`; candidate `40bcb4a072dcbb33bee734f9b1c3ff0f63cf0718`; first review of `git diff 443a79337...40bcb4a07`. HEAD remained the candidate at final source inspection. No source changed. Original task/spec were read through `git show 40bcb4a07:<path>`; uncommitted task/design changes are excluded.

Reviewed the shared navigation map and applicable root/docs/SDK rules; SPEC-positioning revision 4, TASK-001 including its four scenarios and locked animation/strip; architecture/wyrd-design.md client model, Observation identity, Verifier and Agents/tools/workspaces; relevant security permission and evidence provisions; maintainer style and TESTING. Reviewed source-to-README/mdsvex/homepage consumers, all three actual finance programs and declaration modules, API, focused journeys, slot/result public projections and existing alternatives, source highlighting and BeatLoop/component lifecycle. Cross-tier standards and ownership specialists independently cover their assigned boundaries. No other reviewer conclusions were consumed.

Revision 4 explicitly removes the live-provider walkthrough/release-availability gate (`f9bfb494b`); lack of live provider execution or a published 0.4.1 package is not proposed as a finding. The same revision retains PDF evidence and the no-public-API constraint. Candidate task claims of human completion do not themselves revise these approved obligations.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: exact README/homepage headline and question; proof before infrastructure | README.md:19–31; docs/src/routes/+page.svelte:49–75; Introduction:9–32 | Focused rendered homepage/README tests pass; source ordering inspected | PASS |
| REQ-001/007: Declare/Observe/Verify identity and live verdict before application decision | README.md:25–27; homepage hero, FinanceFlow, workflow strip | SSR source and existing browser record; no backend speed measurement implied | PASS |
| REQ-002: existing finance story, synthetic inputs, all three public SDKs and existing route | finance_service/finance_release in all three languages; Quickstart frontmatter/path retained | Real-server actual-program journeys exist and historical task evidence records runs | PASS for reuse/languages/route; purpose evidence fails BEH-005 |
| REQ-002/003, AC-003: selected uploaded PDF extraction actually read through Gateway | finance_service.py:40–76, .rs:40–76, .ts:16–47 now create Markdown and mount its source | All provider fixtures read `/inputs/policy.md`, not PDF extraction | FAIL: BEH-002 |
| REQ-003: one Service binds one Verifier with exactly three requirements, one call per release | finance_service.py:79–103; Rust:79–105; TS:49–66; loops call verify once | Rust graph inspection examples/tests/finance_release.rs:350–374; all-language journeys | PASS for topology |
| REQ-004/AC-001: policy-only prohibited report plus independent recipient/destination failures | Three assertion/Agent branches authored in all languages | Fixtures return bank-marker failure; all-language tests check overall failures, Rust verifies graph IDs; independent requirement outcomes not fully pinned | FAIL for required observed decisions/reasons and defining isolation proof: BEH-001 |
| REQ-004/AC-002, Scenario 2: every non-pass/error holds, reasons and stable errors visible, native distinctions preserved | Actual CLI programs only print verdicts; API branches on passed but errors propagate without hold response | Rust uncertain/missing test checks outcomes only; Python/TS overall uncertainty checks; API test only passed/failed/inconclusive | FAIL: BEH-001 |
| INV-003: missing slots follow native semantics | Target::project leaves omitted named slots absent; architecture requires inconclusive dependent checks | Existing base projection already allowed missing named slots. Candidate adds positional object projection | PASS for server missing-evidence contract; new public projection is BEH-003 |
| REQ-005/AC-004: unchanged requests reuse selected policy; changed exact policy/Verifier/Service selections preserve history | Artifact declaration and exact workspace refs reused; Rust policy test checks old/new mounted versions and Service bindings | Rust test :441–479; two provider read sources compared :457–460 | PASS for reuse/mount identity, FAIL for complete required follow-on and result inspection proof: BEH-004 |
| REQ-005/Scenario 3/AC-005: changed-policy result and public result/details attribution, independently reproducible three-language follow-on | Quickstart:130–132 describes update; result inspection uses stored-verdicts.sql selecting IDs/verdict only | Rust appends inert `Revision 2.` and requires unchanged output; Python checks immutable drift only; TS has no revision scenario | FAIL: BEH-004 |
| REQ-006/AC-005: provider setup, synthetic exposure and correct shipping release pin | Quickstart:21–45 gateway credential/deployment commands, exposure caution; Setup pins 0.4.1 | Existing setup/gateway configure helpers exercise commands; task execution evidence includes relevant journeys | PASS for documented prerequisites/exposure/pin; native API drift separately fails |
| REQ-006: coupled programs, setup-check, snippets, agent handoff and captured output agree | setup-check rename, setup-prompt, Quickstart embeds; highlight strips anchors; README copies source; generator consumes current files | README parity and homepage regressions pass; indexes source inspected | PASS for mechanical parity; output contract itself incomplete BEH-001/002 |
| REQ-007/AC-006: correct CTAs, language links, six-section and GitHub links, current published learning link | +page.svelte:30–38,57–61,95–99,144–149; README language links; Introduction Next | shell-links source and focused pass; task defers new foundation to TASK-002 | PASS |
| Locked finance visual: source-backed API, three requirements, Gateway only Agent, verdict before release, asynchronous evidence capture | +page.server.ts:44–51; FinanceFlow.svelte:31–125; API excerpt | Homepage pass/hold SSR tests pass; historical browser observations | PASS for composition/ordering; PDF label disagrees with actual example BEH-002 |
| Locked lower strip: four distinct workflows, desktop separators, tablet/phone reflow | WorkflowStrip.svelte:16–181 and responsive CSS :183–240 | Source shows step accumulation/queued submission, fan-out/fan-in, session rows; historical browser observations | PASS |
| Motion/static/accessibility: initial finished state, reduced motion, pause, offscreen/hidden suspension and cleanup | BeatLoop:16–18,43–68; components attach/tick effects and text alternatives | SSR finished pass/hold tests; recorded 390/800/1440 viewport, hidden/reduced-motion observations | PASS on source and recorded evidence; browser not rerun |
| REQ-008/AC-007: editorial owner/dispositions, bounded older packet handoff, coupled Author in code | PRODUCT, task inventory, older packet dispositions handoff; Author in code:175 now embeds finance_service run-config; retained service modules inspected | README/indexes proof; source confirms relevant embeds exist | PASS for bounded TASK-001 inventory/handoff |
| INV-001: no new public API/runtime/dependency boundary | New Rust InputSlot helpers/projection, TS schema inputs, all-language result predicates | New SDK tests prove APIs exist; they do not authorize them | FAIL: BEH-003 |
| INV-002: supplied facts and provider exposure honestly described; no transmission or universal guarantees | Quickstart requirements table and exposure caution; no send operation in examples | Source only synthetic data; no external release | PASS for facts/exposure/no transmission; purpose omission BEH-005 |
| INV-003: no new request wait/write or audit behavior | Target::project remains synchronous local value preparation; no server executor/control/SQL/Scribe changes in range | Full changed path review; project function body contains no I/O/await | PASS for sanctioned-wait boundary; no added/moved server waits found |
| INV-004/005/006: foundation, source-derived snippets, honest illustrative pace, no invented metrics or discovery platform | Existing CoverScene/tokens/tabs/highlighter reused; diagrams explicitly illustrative; generated indexes owner changed | Focused tests and historical rendering observations; no new deployment, analytics or indexing runtime | PASS within task scope; full shared metadata/discovery closure remains TASK-004 |

No full docs gate, browser, Postgres, Cargo, Python or TypeScript SDK suite was rerun by this reviewer. Relevant focused SSR/parity execution: `mise exec -- pnpm --dir docs exec vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts -t 'homepage shows embedded finance verification|holds the report when the policy check fails|README'`: 5 passed, 13 intentionally filtered out, exit 0. Historical task evidence concerns several earlier commits and cannot replace missing negative scenarios at final candidate.

## Proposed material findings

### BEH-001 — MISSING: the actual quickstarts do not make an application decision or expose requirement reasons/errors

**Obligation:** REQ-004, AC-001/002, TASK-001 Scenarios 1–2 explicitly require each actual Python/Rust/TS program to print reasons/results and an application decision, release only on passed, and hold all non-pass/errors with stable error information.

**Locations:** examples/finance-release/finance_release.py:11–19; finance_release.rs:25–46; finance_release.ts:11–20; supporting docs/src/content/docs/get-started/quickstart.md:93–108. API consumer examples/finance-release/api.py:40–46 has the pass/hold branch but no error handling.

**Reachable failure:** Run any documented program. It calls verify and prints only `verdict: report text`; no release/hold operation/decision is represented and no individual outcomes/reasons are read. Python/TS exceptions and Rust `?` leave the program before an explicit error/hold decision. A developer copying the example does not receive the very application-control workflow the task promises. The sole Python API separately shows pass/hold, but leaves request/execution failures unhandled and does not supply all-language quickstart behavior.

**Producer/consumer tracing:** The SDK already returns requirement outcomes and reason fields, exact result identity, and typed WyrdError. The examples discard them. Quickstart/Introduction/README embed those same files. Test stdout assertions explicitly lock the reduced two-line output (Rust tests:312–317; Python test_docsite_quickstart.py:109–112; TS finance-release.test.ts:63–68). The negative-flow tests inspect overall verdicts directly without executing an application release/hold decision. Existing API :44–46 is an adequate pattern for the example-level decision; the pre-change finance programs already iterated non-passing requirements.

**Materiality:** This omits the primary requested user outcome, not cosmetic wording. A silent non-transmission on process exit does not demonstrate explicit hold, native error distinction, or why the request failed.

**Smallest correction:** Keep decision and output in the existing example programs using existing verdict/requirement/error fields. Simulate release only for passed and print hold plus reason/code otherwise; retain typed errors rather than relabeling them. Apply corresponding output/parity updates to existing embeds and journeys. No SDK result wrapper, new helper layer or standalone app is needed. Extend the existing focused journeys to assert policy-only failure, independent recipient/destination outcomes, inconclusive and typed error decisions with no simulated release. This is bounded application code and fixture/output assertions.

### BEH-002 — VIOLATION: Markdown replacement removes the required PDF extraction workflow

**Obligation:** REQ-002/003/005, AC-003/004, original task and its stop conditions explicitly require the existing policy PDF, stored extraction, exact Artifact selection and use of that extraction.

**Locations:** finance_service.py:40–50,61,76; finance_service.rs:40–60,74–76; finance_service.ts:16–27,35,47; deletion of examples/finance-release/company-sharing-policy.pdf; Quickstart:93,132. FinanceFlow :26,38 still describes PDF.

**Reachable failure:** Following the guide creates a text file, uploads that file and mounts `/inputs/policy.md`. There is no PDF upload or extraction in any actual quickstart. The source is consumed correctly as Markdown, but it cannot prove the specified PDF workflow. Generated index and animation continue to say PDF.

**Existing alternative:** Base finance_release.py already used `Artifact.from_path(...company-sharing-policy.pdf).extract(True)` and mounted `extracted/company-sharing-policy.pdf.json`; Rust and authoring journeys already use that supported path. Restore/extend this existing complete workflow in the current declaration modules; no storage/extractor implementation changes are needed.

**Materiality/correction burden:** PDF upload/extraction and reuse is an explicit defining onboarding obligation, not a file-extension preference. Restore the asset and existing exact extracted-file selection, align copy and existing provider fixtures/tests around the selected stored extraction. Keeping Markdown instead requires approved spec revision; review cannot reinterpret it as equivalent.

### BEH-003 — DRIFT / VIOLATION: documentation task ships new public SDK contracts

**Obligation:** Approved spec Authority/change boundaries forbids SDK contract change; INV-001 forbids new public API; task Scenario 2 forbids SDK wrapper and stop condition requires returning to spec for new public behavior.

**Locations:** crates/shared/wyrd-client/src/authoring/verifier.rs:123–168 (`model`, `fields_of`, `fields`); verify/mod.rs:238–250,335–354 (positional object now fills multiple slots); crates/wyrd-spec/src/verification/execution.rs:575–595 (new result methods); Python src/verify/mod.rs:319–347 and stubs; TS src/definitions.ts:120–196, src/index.ts:293–323 (new schema inputs and result predicates).

**Reachable scenario:** The copied finance declarations depend on new Rust `InputSlot::fields_of::<Release>()`, TS `inputs: Release` schema behavior and the displayed API's `r.passed`. These are exported behaviors callers can now use; they are not a prose-only change. Inputs::One previously refused multi-slot positional values; candidate now interprets object keys as slot names. This change is distinct from native omitted-slot semantics, which already worked for named slots at base.

**Existing owners/alternative:** Existing named-slot authoring/InputSlot::json and named inputs already satisfy the full three-requirement task. Python `inputs=Model` and its schema decomposition pre-existed; moving its private logic is not itself a new Python authoring contract. Existing `.verdict` / `.outcome.verdict` comparisons suffice for every required decision. TS JsonSchemaSource pre-existed for TableConfig; it did not authorize adding Verifier input behavior.

**Materiality/correction:** Remove task-unearned public authoring/projection/result additions and use the existing named-slot/result fields in examples and current consumers; preserve pre-existing Python schema behavior and existing TableConfig schema-source behavior. That closes the task without deciding new APIs, tests or docs to maintain permanently. If retaining these APIs is desired, obtain an explicit specification decision; an implementation report or completion status cannot waive the approved non-goal. No change to server missing-evidence semantics is proposed.

### BEH-004 — MISSING: required changed-policy follow-on and attribution are not proved on taught surfaces

**Obligation:** REQ-005/006, AC-004/005, Scenario 3 requires changed-policy behavior, old exact attribution and public results/details identifying versions; task says extend Python/TS focused stories too. Revision 4 makes actual-program journeys the proof.

**Locations:** examples/tests/finance_release.rs:441–479, judge_policy :64–82; Python test_docsite_quickstart.py:167–181; TS finance-release.test.ts:63–94; docs Quickstart:110–132; examples/finance-release/stored-verdicts.sql:2.

**Failing proof scenario:** The Rust test appends inert `Revision 2.`, requires identical stdout, then checks registered mounted version pointers. The model fixture chooses verdict only from report text, so it cannot establish that a changed rule affected judgment or that the old declaration retains its original behavior. Python changes a file solely to test explicit-version drift refusal; TS does no policy update. The reader inspection query returns result/run/subject IDs and verdict, but neither exact Verifier/policy selection nor reasons; it does not provide the result-to-exact-policy trail required by the story.

**Materiality:** Green fixtures do not demonstrate the promised policy evolution/inspection journey, and this is the only evidence class retained by approved revision 4. This is a bounded missing original acceptance obligation, not a demand to assess live model accuracy.

**Smallest correction:** Extend existing actual-program tests/provider support with one meaningful synthetic policy change read from the selected extraction, verify the expected changed verdict, and execute/inspect the old exact declaration to prove preserved attribution. Use existing public Cards/result/details/Bifrost inspection owners, including exact selected policy file/ref, rather than invent a history API or version algorithm. Cover the brief update/inspection procedure in each taught language's focused journey and align the existing doc inspection step. No full suite or new harness is necessary.

### BEH-005 — MISSING: policy judgment receives no release-purpose evidence

**Obligation:** REQ-002 explicitly requires synthetic report, recipient, **purpose** and destination evidence; INV-002 requires evidence precision.

**Locations:** Release models finance_service.py:34–37, .rs:34–37, .ts:9–13; all three Agent prompts/inputs bind only report and recipient (Python:61–63,75; Rust:70–72,119–121; TS:35–37,46). The policy itself at Python:44 (and Rust/TS equivalent) permits external sharing only for external-audit purposes.

**Reachable missing state:** Every documented release omits a stated purpose, yet the policy judge is asked whether the policy permits it. Report text contains a financial total/bank marker and recipient contains email/authorized; neither supplies the purpose. Fixtures return passed based on absence of a bank marker, bypassing that required evidence. This review does not infer a particular live model response; the explicitly required input is absent from every taught path.

**Materiality and smallest correction:** Include the synthetic release purpose in the existing evidence/model and bind it into the existing policy Agent prompt. Use the existing named-slot authoring and input projection; do not add API machinery. Require the local fixture to observe the purpose with the selected policy when establishing the permitted case, and update actual-program output/consumer fixtures as needed. Adding a concrete field/binding is a small bounded correction; teaching a policy judgment that silently assumes a required fact undermines the defining example.

## Non-findings and limits

- No latency claim is inferred from illustrative animation timing; no backend hot-path implementation changes or added/moved waits were found. Target::project runs before server invocation and performs only local value projection.
- Existing missing named slots already preserve native missing-evidence semantics. Do not remediate by making them refused or by changing the server.
- Shared metadata/title/discovery closure and new foundation/FAQ are assigned to later tasks; their absence is not a TASK-001 finding. The currently published learning link is valid.
- The motion owner has local state, effect cleanup and per-component lifecycle; no complex shared server lifecycle or deployment/recovery ownership change was found requiring a separate invariant/system review.
- No browser or full journey rerun was performed. Existing records and passing SSR tests support presentation behavior but do not cure the source-proven missing acceptance paths above.
