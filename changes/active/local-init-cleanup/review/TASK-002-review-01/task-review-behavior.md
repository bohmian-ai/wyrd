# TASK-002 behavior acceptance audit

Result: **FAIL** — two proposed material findings, BEH-001 and BEH-002. This is
the discovery review, subject to independent validation. No functional SDK
regression or requirement to measure real-model judgment quality is asserted.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/wyrd-main-2026-11-02`.
- Base: `5aaa38937d76fef1a690cd931e3001363be69ddc`.
- Candidate: `729255588540db8a46dba9f5d090bbfc9e59d2bb`.
- Original task/specification read from contract snapshot `e13bbb5fb`, before
  the implementation evidence appendix. Complete base-to-candidate diff
  inspected, including generated projections, specification narrowing, and
  task changes. No previous task acceptance carried forward.
- Revision-2 approval remains an orchestrator-owned pending question. Explicit
  user narrowing excludes benchmarks and duplicate architecture explanations.
  This report does not infer approval or turn draft metadata into a code defect.
- Read `AGENTS.md`, `docs/AGENTS.md`, `docs/CONTRIBUTING.md`, `docs/PRODUCT.md`,
  `changes/spec-driven-development.md`, maintainer style and agent-surfaces
  guides, architecture authority map, Verifier/evidence/citation/binding/group
  sections of `architecture/wyrd-design.md`, and Bifrost system boundary.
  No additional nested AGENTS files govern the changed directories. CodeGraph
  is absent; repository navigation used `rg` and source reads.
- No specialists were needed: the runtime owner is unchanged; guide changes
  alter strings and the generator change extends its existing static START
  list. No new security, lifecycle, storage, or concurrency implementation.

## Inspected coverage and navigation map

| Surface | Owner and consumers inspected | Assessment |
| --- | --- | --- |
| README | Opening, six badges/alt text/anchors, existing tested example, next-step routes | Six neutral navigation links; existing example retained |
| Hub and comparison | `get-started/what-can-i-build.md`, all six workflow sections, comparison rows, reference/date links | Thin routing owner; existing technical explanations linked |
| Existing recipes | Opening selection table, Model judge, Uploaded files, Grouped session, Drift; existing authorization reference | Canonical owner retained; new public runnable material has BEH-001 |
| Policy source | `fixtures/cards/uploaded_artifacts/handbook-{desk,review,judge}.yaml`; quickstart Service; support-desk example | Existing Service and mount ownership available; no need for parallel tutorial |
| SDK proof | Complete Rust uploaded-Artifact journey and scripted responder; corresponding Python journey; TypeScript sibling call/assertions located | Real server/tool/mount/citation behavior is covered by existing journeys; exact new published snippet is not their source |
| Shared prompt | `wyrd-spec/src/setup-prompt.md`, `mcp.rs` projection and invariant test; CLI init and server MCP consumers | One source retained; release heading/secrets contract unaffected |
| MCP guide | `GuideTopic::Start` and its surrounding instructions | Setup → quickstart → hub route; wire names/scopes unchanged |
| Human discovery | Introduction, quickstart, coding-agent page, landing route, frontmatter content/navigation owner | Existing route machinery reused; setup and quickstart remain first |
| Machine discovery | Generator START/filter/rendering, both generated indexes, page-text and CodeFromFile resolution, indexes tests including omitted/draft corpus cases | Existing owner extended; generated full text carries the new public material too |
| Recorded exercises | Candidate task lines 347–387 and complete change-packet file inventory | Eight exercise summaries exist; raw answers/exact complete prompts are not preserved in this packet (BEH-002) |

## Acceptance matrix

Verification references: `verification.md` records the orchestrator's independent
62-test focused docs run, one shared-prompt test, local link check, and range
whitespace check, all passing. Candidate task records the broader docs lane,
MCP discovery journey and uploaded-Artifact journey; those are supplied evidence,
not independently rerun by this reviewer.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: production verification across AI/ML and application workflows | README and introduction openings; hub policy/runtime/observability/drift/group routes and versioned Cards | Source and authority comparison; preserved existing worked example | PASS |
| REQ-002 / AC-001: six accessible neutral badges | README six grey shields, descriptive `Use case:` alt text and exact hub anchors | Static label/anchor inspection; supplied docs a11y evidence | PASS |
| REQ-003 / AC-002: six platforms, three columns, accurate overlap and MLflow coexistence | Hub comparison has exactly requested columns/platforms, external IDs supplied by application, no connector claim, Phoenix separated from AX | Primary documentation checked during this audit; see references below | PASS |
| REQ-004 / AC-003: concrete differentiators and canonical concept reuse | Hub short descriptions link verification, Bifrost and architecture owners; no new architecture section | Full diff inspection | PASS |
| No benchmarks, speedup rankings, new integrations, status taxonomy, OPA or expanded vendor matrix | Complete content/generated diff | Source inspection; no runtime connector implementation | PASS |
| REQ-005 / AC-004: all specified human and agent entry routes | README, landing CTA/frontmatter, introduction, quickstart Next, coding-agent Next, setup prompt step 7 and MCP Start | Focused docs tests and local links pass; prompt invariant passes | PASS |
| REQ-005 / INV-003: setup → quickstart → capability selection, drafts excluded | Existing generator START list adds third route; existing publication filters unchanged; coding-agent order becomes 5 | Both-index ordering assertions and omitted/draft fixtures retained/extended; 62-test focused run passes | PASS |
| Quickstart remains minimal and explicitly simplest | Only introductory/Next prose changes | Complete quickstart diff; source unchanged | PASS |
| REQ-006: policy version/mount/tools/authorization/cited-result semantics | Existing fixture embeds; exact extracted-file workspace selection; new fs.read grant note; existing authorization reference; citation loop; deterministic checks and verify/submit links | Rust journey verifies fresh marker from selected file, fs_read-only offered tool, excluded rota, attachment slot and Lines locator; matching Python journey inspects same shape | PASS |
| REQ-006 / AC-005: complete public runnable workflow using tested source, at documented release | New handwritten binding/Python fences in Recipes lines 432–452; no example-source or journey change | Source SDK journey exercises a different Service through its own setup; candidate admits no documented-release public workflow run | FAIL — BEH-001 |
| REQ-006: canonical check selection, grouped and scheduled drift routes | Existing selection table updated once; hub links existing Trace/Grouped session/Drift anchors | Source/authority comparison and local links | PASS |
| REQ-007 / AC-005: eight independent reader scenarios with recorded prompts, sources and answers | Task records model, docs revision, entry restrictions, four question texts, sources and eight answer summaries | Summaries suggest correct comprehension, but requested actual answers and complete exact prompts are missing | FAIL — BEH-002 |
| INV-001: application owns orchestration and enforcement; no formal proof or latency promise | Hub introduction, policy, long-workflow and runtime sections; recipe says application acts on verdict | Verifier authority confirms inline versus queued and grouped semantics; no new runtime behavior | PASS |
| INV-002: evidence-backed claims and bounded composition | Comparison focuses on product centers and positive Wyrd capabilities, not competitor inability; Code described as exact logic, never unrestricted Python | Authority/fixture/journey review and primary-source checks | PASS |
| INV-003: one discovery owner; no names/schemas/scopes/lifecycle changes | Shared source prompt, existing MCP guide, existing frontmatter and index pipeline | Complete diff and projection consumers inspected | PASS |
| Docs publication/source ownership and generated artifacts | Fixture embeds retained and indexes updated from same content; new recipe fences bypass tested-source rule | Structural/link tests cannot prove runnable snippet provenance | FAIL — BEH-001 |

## Proposed material findings

### BEH-001 — Published policy completion bypasses the tested-source/public-run gate

- **Classification:** VIOLATION (also leaves REQ-006 public-workflow proof missing).
- **Obligation:** `docs/AGENTS.md` Examples requires copyable code from a
  journey-tested source through CodeFromFile; `docs/CONTRIBUTING.md` Publishing
  requires running a guide's steps at the documented release. Original TASK-002
  requires tested source reuse and completing the public workflow with an
  observed cited verdict. These are explicit publication gates.
- **Exact locations:**
  `docs/src/content/docs/build-and-verify/verify/verifier-recipes.md:432–452`;
  `changes/active/local-init-cleanup/tasks/TASK-002-document-verification-scope.md:333`
  and `:385–387`. The same snippet reaches agents via generated `llms-full.txt`.
- **Source evidence:** The newly added binding and Python program are handwritten
  Markdown fences. The existing fixture Service is `handbook-desk.yaml`, whereas
  the new call declares `support-reply.yaml`. The actual Rust test at
  `sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs:1917` writes
  a handbook, configures a gateway, uploads the Artifact, declares/pulls
  `handbook-desk`, provisions an fs.read-scoped credential, then invokes its
  Verifiers using an injected test client. The Python sibling at
  `sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py:1105`
  likewise invokes its own fixture bundle/client. Neither consumes the new
  public Python program or its proposed Service. The task's sole cited proof
  is that source-tree journey and its limits expressly omit a documented-release
  public execution.
- **Reachable scenario/consequence:** A reader follows the newly promoted
  Uploaded files path from a README badge or machine onboarding, copies this
  new Service-binding/program combination, and expects the promised runnable
  public path. Maintainers have neither the mandatory shared tested source for
  this published code nor the release-path observation establishing that the
  documented setup/declaration/call returns the shown cited result. The source
  journey proves the underlying mount/tool/citation capability, but cannot
  catch drift or omissions in independently maintained public copy. This is a
  mandatory proof/provenance failure, not a claim that the Python call itself
  is type-invalid or that semantic model quality failed.
- **Sibling consumers/common cause:** Rendered Recipes, the hub link, README
  badge, and generated full-text index all consume the same Markdown owner.
  Correct that owner once; do not patch each consumer or add another tutorial.
- **Smallest safe correction/proof:** Reuse the existing uploaded-Artifact
  Cards/Service and a small public example for the invocation/citation
  inspection; embed the actual journey-exercised source with CodeFromFile.
  Make the existing public path concrete (file placement/Service identity,
  gateway and grant prerequisites through existing owners), and execute those
  exact published steps at the documented release, recording the citation and
  verdict. Reuse the existing scripted provider if appropriate; no real-model
  quality benchmark or new provider service/harness is required. Regenerate
  the indexes and run only focused docs/example proof. A prose-only citation
  to a separate source SDK test cannot close the explicit source/publication
  gate. Existing fixtures and journey infrastructure bound the correction;
  no runtime, schema, grant-model or vendor-integration work is justified.

### BEH-002 — Reader results are summarized instead of preserving required answers

- **Classification:** MISSING.
- **Obligation:** REQ-007 and original task Proof Strategy explicitly require
  recording exact prompts, model/version, entry point, docs revision, sources
  **and answers** for all four scenarios through both entry paths.
- **Exact location:**
  `changes/active/local-init-cleanup/tasks/TASK-002-document-verification-scope.md:347–379`,
  particularly the `Answer summary` column at lines 365–374.
- **Evidence:** The record includes a model/version, revision, date, four
  question strings and general input restrictions. Eight rows summarize the
  claimed answers, with some rows saying only “Same composition” or “Same”.
  Actual answers and complete exact reader prompts are absent. The packet
  inventory provides no linked transcript or answer artifact. These summaries
  cannot establish what qualifications the readers actually used or whether
  the original prompts supplied part of the intended answer. This report does
  not infer that the answers were wrong or that the exercises never occurred.
- **Reachable scenario/consequence:** Acceptance of AC-005 relies on the
  implementation author's rubric labels; an independent reviewer cannot check
  the eight answers against the no-invented-integrations/application-boundary
  rubric from the required evidence. This matters because reader comprehension
  is itself an explicit task deliverable, separate from runtime tests.
- **Smallest safe correction/proof:** Preserve the original complete prompts
  and eight actual answers, retaining their existing model/revision/entry/source
  metadata. Recover original results if available; rerun only missing exercises
  if they are unavailable. Evaluate each actual answer against its existing
  rubric and repair/rerun only a failed row. No evaluation framework, CI gate,
  broad content rewrite or additional reader scenarios are required.
- **Calibration:** Source-material simulation of MCP Start is disclosed and can
  supply bounded comprehension evidence when it faithfully represents the
  onboarding projection; the separate guide journey tests discovery. Lack of
  a live MCP connection alone is not this finding. The public workflow proof
  gap belongs to BEH-001, not this finding.

## Comparison verification and non-findings

Primary documentation read during this audit supports the actual high-level
focus descriptions: [MLflow GenAI](https://mlflow.org/docs/latest/genai/),
[Langfuse](https://langfuse.com/docs),
[LangSmith Observability](https://docs.langchain.com/langsmith/observability),
[Braintrust](https://docs.braintrust.dev/), and
[Phoenix](https://arize.com/docs/phoenix/). These are used for product focus,
not proof of competitor feature absence or performance. The candidate makes
no comparative benchmark claim requiring one. No material comparative defect
was found. No duplicated architecture owner, second navigation mechanism,
dependency, speculative extension or runtime change was found.

The scripted Artifact journey is meaningful positive evidence: the random
marker is unavailable in initial model input, the tool actually reads the
selected mount, the final reason contains it, the citation is checked, and the
unselected file stays out. It proves considerably more than a response existing.
It still does not execute the handwritten release-facing program. Real-model
judgment quality is outside the required proof and is not demanded here.

## Limits and result

**FAIL**, pending independent validation of BEH-001 and BEH-002. Remaining
acceptance is source-backed as mapped above; no optional improvement or wording
preference is proposed. Revision approval remains a separate root-owned gate.

No builds, tests, Cargo commands, provider calls, source edits or Git mutations
were performed by this reviewer. Root verification is cited above. Candidate
HEAD remained `729255588540db8a46dba9f5d090bbfc9e59d2bb`; only this assigned report
was written. Skipped broad reruns and live-model quality evaluation; the
material remaining risk is unproven public recipe publication and uninspectable
reader-answer evidence.
