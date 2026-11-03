# TASK-002 independent finding validation

## Subject, authority and limits

Base `5aaa38937d76fef1a690cd931e3001363be69ddc`; candidate
`729255588540db8a46dba9f5d090bbfc9e59d2bb`; original task and specification
read from contract snapshot `e13bbb5fb`. This is independent source and
materiality validation of BEH-001 and BEH-002, plus a bounded check of the
discovery report's remaining acceptance coverage. It is not another full
discovery pass. No prior TASK-002 acceptance is carried forward.

Revision 2 remains draft in the inspected source. Revision-1 approval and the
subsequent explicit user narrowing (no benchmarks; reuse architecture owners)
define the recorded intended scope. This report does not grant revision-2
approval; the orchestrator must resolve that separate authority question.
Neither finding depends on restoring the excluded benchmark work.

Read the task-review skill, supplied root rules, `docs/AGENTS.md`,
`docs/CONTRIBUTING.md`, the applicable spec-driven authority and proof rules,
maintainer style, agent-surfaces guide, architecture authority map, the
Verifier citation contract, and Bifrost system boundary. CodeGraph is absent.
Inspected the actual cumulative changed surfaces and the full relevant
uploaded-Artifact Rust/Python journey bodies, scripted responders, fixtures,
TypeScript sibling, and the existing Python docs-example execution pattern.
No new blocker was discovered requiring separate source validation.

No builds, tests, Cargo, provider calls, source/task/spec edits or Git mutations
were performed. `verification.md` supplies the orchestrator's independently
rerun 62 focused docs tests, one prompt-invariant test, local link check and
range whitespace check. Those establish their stated structural properties;
they do not execute the newly published program or recover reader answers.
The broader docs and SDK journey results in the task remain supplied evidence.

## Proposal decisions

| Discovery proposal | Decision | Retained ID | Reason |
| --- | --- | --- | --- |
| BEH-001 | REVISED | FIND-TASK-002-1 | Both explicit publication/source-proof gaps are supported; narrow correction to the exact public recipe and existing infrastructure. Do not demand real-provider judgment quality, a new runtime/harness, or an unrelated SDK matrix. |
| BEH-002 | CONFIRMED | FIND-TASK-002-2 | The original proof obligation explicitly asks for prompts and answers. The sole preserved record has question strings, general instructions and summaries, including “Same” rows; actual answers and complete prompts are absent. |

The findings have different owners: the published recipe/source proof and the
reader-exercise evidence record. They do not justify a content redesign.

## Validated ledger

### FIND-TASK-002-1 — Exact published policy recipe lacks mandatory source and execution proof

- **Discovery ID / status / classification:** BEH-001 / REVISED / VIOLATION,
  with the REQ-006 and AC-005 public-completion proof still missing.
- **Obligation:** `docs/AGENTS.md`, Examples: “Embed code with
  `<CodeFromFile file="examples/...">` from a file a journey test runs” and
  “Do not hand-write a snippet a reader will copy.” `docs/CONTRIBUTING.md`,
  Publishing: a guide's code comes from an example run against a real server,
  and its steps must be proved at the documented release. The original
  TASK-002 What Changes requires tested-source reuse; Proof Strategy also
  requires completing the public policy workflow and observing its cited
  verdict at that release.
- **Exact locations:** `docs/src/content/docs/build-and-verify/verify/verifier-recipes.md:432–452`
  (new binding and Python fences); candidate task acceptance row at line 333
  and limitation at lines 385–387. `docs/public/llms-full.txt` projects the
  same Markdown owner.
- **Producer-to-consumer proof:** The recipe introduces its `support-reply.yaml`
  convention at line 10, embeds the existing handbook Verifier/Agent, then
  independently handwrites the Service binding and public invocation. The
  binding is a fragment, not the tested `handbook-desk` Service. The Rust
  journey `uploaded_artifacts_are_reused_by_service_judges` at lines 1917–2029
  uploads a fresh marked file, configures the gateway, declares/pulls
  `handbook-desk.yaml`, grants `fs.read`, and invokes a state built from that
  bundle with an injected client. It asserts passed verdicts, the file-only
  marker in the reason, an attachment citation with Lines locator, only
  `fs_read` offered, and exclusion of the other file. The complete Python
  journey at lines 1105–1150 and TypeScript sibling at lines 998–1040 use the
  same fixture identity and equivalent result checks. None executes the
  new public program or consumes its Service fragment as its example source.
  The candidate's own evidence expressly supplies the scripted SDK journey
  instead of a documented-release public run.
- **Reachability and consequence:** README badges, hub policy link and machine
  onboarding promote this published recipe. Readers copy code maintained
  separately from the executed source. Its exact declaration, input wiring
  and citation-printing path have no required publication proof. A green
  underlying capability journey cannot detect drift in this independent copy.
  This is an explicit gate failure, not a claim that the Python API is invalid
  or that a reader could never construct a Service from the fragment.
- **Materiality:** The requested outcome is a completed, public worked path;
  its missing source/execution proof is part of that deliverable. Structural
  tests and understandable prose cannot substitute for the explicit gate.
- **Smallest correction boundary:** Correct Uploaded files once. Reuse the
  existing handbook fixture bundle and its exact Service identity, including
  the dependencies that Service references. Place the minimal public invocation
  and citation inspection in an example source that a journey executes, and
  embed it through CodeFromFile. Make file placement, upload, Service identity,
  gateway and grant prerequisites concrete by linking existing owners and
  showing only the local missing steps. Do not create another tutorial or
  change authorization, schemas, runtime behavior or the fixture capability.
  Regenerate machine projections from that owner.
- **Why smaller alternatives fail / burden:** Deleting the new completion
  would remove REQ-006 behavior. A prose link to the existing journey still
  leaves independently copied executable code untested. Merely moving the
  fence into an unexecuted file also fails. Existing example execution already
  solves the machinery: `test_docsite_quickstart.py` copies public files into
  an empty reader directory, selects the normal client connection through
  environment, runs the actual script and asserts its printed results. The
  existing handbook responder in Python `conftest.py` and Rust `judge_handbook`
  performs the real file-tool round trip. Reuse those established mechanisms;
  one bounded source/example correction and focused proof suffice.
- **Exact closure proof:** A focused journey must execute the exact embedded
  source and complete published file/declaration/upload/invocation sequence
  against the real server/Postgres through public client calls, obtaining a
  verdict and a valid citation to the uploaded selected file; check the
  reason/citation against the file's marker or equivalent known content.
  Record the command, source/release identity and observed result for the
  public workflow at documented release 0.4.1. If execution uses candidate
  sources, disclose that identity accurately and do not label it as a release
  run. An existing scripted provider is sufficient for composition/citation
  proof when disclosed; no paid provider, semantic-quality score, live MCP
  session or new provider/harness is required. Run only the focused example
  proof and docs generation/affected docs checks. Preserve the existing
  mount/tool/authorization regression evidence; no broad cross-SDK rerun is
  required solely to repair this Python documentation path.

### FIND-TASK-002-2 — Required reader answers and complete prompts are not preserved

- **Discovery ID / status / classification:** BEH-002 / CONFIRMED / MISSING.
- **Obligation:** REQ-007 and original TASK-002 Proof Strategy require
  model/version, exact prompts, entry point, docs revision, retrieved sources
  and answers for four problems through each of two entry paths. Actual
  answer content must support the specified composition and boundary rubrics.
- **Exact location:**
  `changes/active/local-init-cleanup/tasks/TASK-002-document-verification-scope.md:347–379`.
- **Source proof:** The record supplies `claude-sonnet-5-5`, revision
  `fb7db48c3`, entry material, date, restrictions, four question strings and
  eight rows under `Answer summary`. Several rows explicitly summarize by
  reference (“Same composition”, “Same”). They are not the eight returned
  answers. The general restrictions do not preserve the full exact instruction
  messages. The packet inventory contains no linked answer/transcript artifact.
  The record's assertion that every answer passed is therefore the producer's
  rubric conclusion, not the answer a reviewer can assess.
- **Reachability, consequence and materiality:** AC-005 explicitly depends on
  reader comprehension. A reviewer consuming this evidence cannot independently
  check omissions, qualifications, invented interoperability or prompt leakage
  against the rubric. This does not prove the readers answered incorrectly or
  that the exercises did not occur; it proves a required deliverable is absent.
- **Smallest correction boundary / burden:** Recover and preserve the original
  complete instruction/question messages and eight actual answers in this
  packet's implementation evidence (or a clearly linked evidence artifact),
  retaining the existing per-run model/revision/entry/source metadata. Rerun
  only missing records when original outputs cannot be recovered. Evaluate
  actual answers against the existing rubric and repair/rerun only failed rows.
  No framework, CI gate, added scenarios or content rewrite is warranted.
  Summaries may remain as navigation, but cannot replace the required evidence.
- **Exact closure proof:** Each of the eight records contains the complete
  supplied prompt, actual answer, identity/entry/source metadata and a
  source-checkable rubric assessment. Independent reading can determine whether
  the answer meets its existing row and application/integration boundaries.
  A faithful, disclosed source projection of MCP onboarding remains acceptable
  for this bounded comprehension exercise; a live MCP connection is not an
  additional closure requirement. Source discovery itself has its separate
  recorded MCP journey. Public runtime proof remains FIND-TASK-002-1.

## Remaining acceptance coverage

| Obligation | Independent bounded source check and available proof | Assessment |
| --- | --- | --- |
| REQ-001/002, AC-001 | README opening and six neutral grey badge labels, alt text, exact hub anchors; introduction retains tested quickstart example; hub covers AI/ML and application workflows | Covered; no retained finding |
| REQ-003, AC-002 | Hub has exactly the six requested rows and three columns; positive Wyrd capabilities, MLflow evidence supplied by application, Phoenix/AX distinction, dated official references. Discovery independently checked those primary sources | Covered by source plus discovery evidence; no invented connector or comparative performance claim found |
| REQ-004, AC-003 and user narrowing | Complete affected content diff adds links to existing verification/Bifrost/architecture explanations. No benchmark material, second architecture section, numerical rankings or parallel selection table | Covered; no unearned machinery |
| REQ-005, AC-004, INV-003 | Setup prompt step 7 and MCP Start strings route setup → simplest quickstart → hub; intro/quickstart/coding-agent Next and landing use existing routes. Generator only extends START; existing published-only filter and render_start select present routes. Complete index tests retain ordering, omitted/draft/empty corpus and publication closure proof | Covered; root 62-test run, prompt invariant and links support unchanged discovery owner |
| REQ-006, AC-005 | Existing fixtures and full Rust/Python/TS public-client journeys prove selected upload/extraction/workspace/tool/citation composition; changed recipe states grant and application-owned enforcement | Capability supported; exact public recipe proof fails FIND-TASK-002-1 |
| REQ-007, AC-005 | Eight summarized reader exercises, disclosed MCP simulation and specified metadata | Required preserved evidence fails FIND-TASK-002-2 |
| INV-001/002 and non-goals | Hub and recipe leave orchestration/enforcement with the application, link verify/submit, separate exact checks from model judgment, and promise neither formal proof nor fixed latency. Verifier authority lines 1545–1556 supports valid pinned citations and uncited-pass inconclusive behavior | Covered; no runtime/security/lifecycle changes requiring a specialist |
| Mandatory documentation provenance/publication rules | Existing fixture embeds are reused; newly copyable completion has no shared executed example source; root structural checks cannot establish this property | FIND-TASK-002-1 |

No proposal was rejected or promoted to optional remediation. The final
validated ledger contains exactly **FIND-TASK-002-1** and
**FIND-TASK-002-2**. No new product or expensive architecture decision is needed
to close them. Approval status remains an orchestrator-owned prerequisite to
the final task verdict.

Candidate HEAD was rechecked as
`729255588540db8a46dba9f5d090bbfc9e59d2bb`; only this assigned report was written.
Skipped test/build reruns and real-model quality evaluation. Remaining risks
are the unproved exact public workflow and uninspectable reader-answer record.
