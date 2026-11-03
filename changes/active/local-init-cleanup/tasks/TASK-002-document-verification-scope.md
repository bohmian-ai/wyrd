---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-local-init-cleanup
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, INV-001, INV-002, INV-003]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
---

# Document Wyrd's full verification scope and make its workflows discoverable

## Outcome and Value

A person or coding agent can identify what Wyrd verifies, why to choose it
over adjacent tools, and how to build a custom verification layer. Today the
assertion quickstart can obscure richer capabilities already implemented and
tested. After this task, a payment-policy question leads directly to a
versioned policy Artifact mounted into an Agent check, with deterministic
checks and workflow evidence where appropriate.

This is one documentation outcome across README, docs, machine indexes and
agent onboarding. The recorded decisions are in [the shared draft spec](../spec.md); the
source evidence and exact verification commands below make it executable
without the author conversation.

## What Changes and What Must Stay True

Add the six approved README badges, one use-case hub, the simple six-platform
comparison, links to existing engine/production explanations, and a
prominent route to the completed existing policy-artifact recipe. Connect
existing entry points to these canonical destinations and keep the first
quickstart minimal. Reuse existing docs sections,
frontmatter and components; a new top-level section or navigation engine is
not required. Use existing tested source for copyable examples.

The hub gives each badge a short problem-to-workflow description and canonical
links. It does not reproduce check tables, technical explanations or tutorials.
Cover policy
judgments, agent evaluation, extended workflows, runtime decisions,
observability and scheduled drift. Include model/data/artifact/experiment
registration as part of Wyrd's broader substrate rather than reducing the
product to traces and scorers.

The comparison has only Platform / Main focus / Why choose Wyrd? columns.
Wyrd leads with production verification across AI/ML systems. MLflow covers
model/experiment lifecycle and GenAI; Langfuse, LangSmith, Braintrust and
Phoenix compete in the narrower LLM/agent tracing/evaluation slice. Explain
coexistence plainly, including externally supplied MLflow evidence. No OPA,
integration-status labels, feature-checkmark grid, new vendor integrations or
unsupported blanket speedup claims. Gateway-only and guardrail vendors are
deferred from the initial matrix.

Preserve versioned identity, tenancy, grants and tool limits. Describe
application-owned enforcement and orchestration accurately. A missing
capability or runtime contradiction discovered while writing is not permission
to implement it. Do not change runtime behavior, public schemas, MCP tool
names, the CLI lifecycle, or existing deployment expectations.

## Approach

1. Revalidate the source evidence below and current official vendor facts;
   record their checked date beside the comparison's supporting references.
2. Write thin use-case routing. Reuse/update and link the existing Recipes
   selection table; promote and complete Uploaded files in its current owner
   with Service declaration, invocation and cited-result inspection. Link the
   published grouped/drift sections rather than reproducing them.
3. Add the simple comparison, make Wyrd's concrete differentiators visible,
   and link existing verification/Bifrost/operational explanations. Do not add
   a new architecture section or publish benchmarks.
4. Add accessible README badges and connect the shared prompt, MCP start
   guide, coding-agent page, docs landing/navigation and index onboarding.
5. Prove changed discovery behavior, regenerate owned outputs, run the bounded
   reader exercises, and finish the focused checks below.

## Ordered Implementation Scenarios

### Scenario 1 — Richer workflows appear in human and machine discovery

**Behavior.** Published capability/comparison/policy pages are reachable from
the approved entry points and appear in search/navigation and both machine
indexes. Setup and quickstart remain the first machine onboarding steps, then
capability selection. Draft/withdrawn pages do not leak into discovery. The
shared prompt links an actual page instead of naming an absent one.

**RED.** Extend the existing navigation/index fixtures to demonstrate the
missing capability route and onboarding next step; retain draft exclusion and
existing ordering assertions. Run the focused docs command in Verification
and Evidence and confirm failure for the missing discovery behavior. If the
implementation uses only already-supported static frontmatter/configuration,
verify that behavior directly instead of manufacturing executable logic or a
test that merely pins prose.

**GREEN.** Publish the content through existing owners and update the static
onboarding configuration. Change executable selection/navigation logic only
if the current owner cannot express the required route, and then satisfy the
focused failing test without adding a second discovery system.

**REFACTOR.** Remove duplicated instructions and stale prose-only destinations;
keep the navigation and machine-index representations aligned using their
existing structure. Preserve passing regression checks.

## Proof Strategy for Content

Badges, explanatory prose, comparison copy, prompt wording and source
references are static obligations: no artificial RED or string-pinning tests.
Review rendered pages, links, source provenance and generated text. Existing
journeys establish runtime behavior; they do not establish real-model judgment
quality or reader comprehension.

Use four fresh-context reader prompts, without private architecture/source
access, once from rendered public docs and once from MCP onboarding:

| Reader problem | Required understanding |
| --- | --- |
| Payment events must follow a supplied policy document. | Upload/version policy evidence; mount selected files for an Agent judge; combine exact Code/Assert checks with semantic judgments; supply relevant facts and enforce the verdict in the application. |
| A coding/review workflow runs for hours. | Verify steps and grouped accumulated evidence; identify when evaluation runs; do not turn Wyrd into the workflow executor. |
| Models and experiment results already live in MLflow. | Keep that ownership if desired, supply selected evidence to Wyrd, and retain exact external identifiers; do not invent a built-in fetch/sync connector. |
| Tracing and gateway tools are already deployed. | Explain Wyrd's overlapping capabilities, broader verification role and possible coexistence without forcing replacement or promising automatic interoperability. |

Record model/version, exact prompts, entry point, docs revision, sources and
answers in the task's implementation evidence. Each answer must meet its row
and the applicable boundaries; repair omissions and rerun the affected prompt.
Use the existing ability to launch fresh readers; no new evaluation service,
automated LLM CI gate or synthetic load. For the policy guide, also complete
the existing public workflow and observe its cited verdict at the documented
release. Reuse the uploaded-Artifact journey below as regression evidence.

## Acceptance Criteria

| Obligations | Acceptance and proof |
| --- | --- |
| REQ-001, INV-001 | README and docs open with production verification across AI/ML and application workflows. Source review confirms substantive coverage beyond agents, while orchestration/enforcement responsibilities remain clear. |
| REQ-002 | All six named badges are legible, accessible links to published relevant content; visual/link inspection confirms they do not imply certification. |
| REQ-003, INV-002 | Six-platform, three-column comparison gives Wyrd full scope, accurately represents vendor focus and overlap, and contains no OPA/status taxonomy or invented integrations/parity. Official references are dated. |
| REQ-004, INV-002 | Concrete differentiators link to existing verification, Bifrost and operational explanations. No duplicated architecture section, benchmark tables, figures or numerical speedup claims are published. |
| REQ-005, INV-003 | Focused discovery regressions plus generated/rendered inspection establish every entry route, onboarding order and draft exclusion. The nonexistent next-step instruction is replaced by a working link. |
| REQ-006, INV-001 | The promoted policy path reuses tested fixtures/examples, explains exact selected files/tools/citations, and completes the existing public workflow. Group and drift routes and check-selection guidance are discoverable. |
| REQ-007 | All four reader problems meet their rubric through both entry paths in fresh contexts; recorded evidence supports comprehension without access to this packet or private source. |

## Implementation Reference

Reuse these inspected owners; paths guide the implementation, not a file
allowlist:

| Content | Canonical owner and minimum edit |
| --- | --- |
| Check selection | `build-and-verify/verify/verifier-recipes.md`, existing opening table: update only if needed, then link. No second selection table. |
| Policy Artifact workflow | The same page's Uploaded files section: retain upload/version/mount guidance; add the missing Service declaration → invocation → cited-result public steps from existing tested material. No parallel policy tutorial. |
| Check semantics, workspaces, versions, verdicts and inline/queued behavior | `understand/how-verification-works.md` and `reference/verifier-card.md`: link existing explanations. |
| Grouped evidence and drift | Published Grouped session and Drift sections in Recipes: link them. Existing draft guides are not prerequisites and must not be published merely to create badge destinations. |
| Architecture and Bifrost | `understand/architecture.md` and `understand/bifrost.md`: link existing explanations; add no new architecture/performance section. |
| Use cases and vendor comparison | Thin routing descriptions and one maintained three-column comparison, which may share an existing page or one thin new page. README, prompts and other entry points link to those owners rather than copying them. |

Paths in this table are relative to `docs/src/content/docs/`. The new content
is navigation and the requested comparison, plus completion of the missing
public steps in the existing Artifact recipe. Shared generated projections
remain generated from these sources.

- **README:** `README.md` already mentions documents/long workflows but its
  worked example emphasizes an assertion. Add badges and richer routing;
  preserve its tested snippets and existing product identity.
- **Human discovery:** `docs/src/lib/content.ts` owns sections/frontmatter;
  `derived-nav.ts` consumes them; `docs/src/routes/+page.svelte` owns landing
  discovery. Existing content/navigation/index/shell-link/README tests cover
  these seams. Extend them only for material discovery changes.
- **Machine discovery:** `docs/scripts/generate_llms_txt.py` owns `START`,
  section ordering and public indexes. `page-text.js` expands the same embedded
  source into full-text output. Regenerate `docs/public/llms*.txt` through the
  existing task. Do not create a second capability schema or generated catalog.
- **Agent entry points:** `crates/wyrd-spec/src/setup-prompt.md` is embedded by
  `mcp::setup_prompt` and projected by CLI/MCP. Edit once, preserving release
  heading and no-secrets invariants. Extend `GuideTopic::Start` in
  `crates/wyrd/wyrd-server/src/mcp/guide.rs` and
  `docs/src/content/docs/get-started/use-with-your-coding-agent.md`; do not add
  another prompt/tool or change the guide's wire contract.
- **Policy example:** `verifier-recipes.md`'s Uploaded files section,
  `fixtures/cards/uploaded_artifacts/{handbook-review,handbook-judge}.yaml`,
  existing support-desk examples and the uploaded-Artifact SDK journeys already
  cover file selection, `fs.read` and citations. Reuse rather than maintain a
  second set of Cards or fabricate copyable SDK snippets. Follow `CodeFromFile`
  and the docs rules for runnable examples.
- **Full engine:** `architecture/wyrd-design.md` Verifier, Agents/tools/
  workspaces and Drift sections; `VerificationEngine::run` in
  `crates/vala/vala-verify/src/blocks/engine.rs`; structured-output and group
  journeys establish more than scalar scoring.
- **Production mechanisms:** `verification/runner.rs` documents durable claims,
  fencing and settlement; security posture owns tenant isolation; Bifrost
  authority owns durable storage, resource budgets, admission and scaled roles.
  Cite those mechanisms without turning user guides into internal inventories.

### Recorded comparison evidence and decisions

Official sources checked on 2026-11-03; refresh before publication:

- [MLflow GenAI](https://mlflow.org/docs/latest/genai/),
  [classic evaluation](https://mlflow.org/docs/latest/ml/evaluation),
  [backend architecture](https://mlflow.org/docs/latest/self-hosting/architecture/backend-store/),
  and [REST API](https://mlflow.org/docs/latest/api_reference/rest-api.html).
  MLflow is broader than a registry/tracker; Wyrd's strength is its integrated
  verification/evidence system. No native MLflow connector was found locally.
- [Langfuse overview](https://langfuse.com/docs),
  [managed code evaluators](https://langfuse.com/docs/evaluation/evaluation-methods/code-evaluators),
  and [LLM judges](https://langfuse.com/docs/evaluation/evaluation-methods/llm-as-a-judge).
  Documented managed-code boundaries include observation/experiment context,
  standard libraries, no network and two seconds. Judge rules use the latest
  evaluator version. Its document inputs are real overlap; Wyrd's complete
  exact-version composition is the stronger distinction. Do not imply Wyrd
  Code checks themselves are unrestricted.
- [LangSmith focus](https://docs.langchain.com/langsmith/observability),
  [managed evaluators](https://docs.langchain.com/langsmith/code-evaluator-ui),
  [SDK evaluators](https://docs.langchain.com/langsmith/code-evaluator-sdk),
  [Braintrust](https://docs.braintrust.dev/), and
  [Phoenix](https://arize.com/docs/phoenix/).
  These support the narrower product-center comparison, not claims that
  custom scoring is absent. [Arize AX](https://arize.com/products/ml-cv-observability/)
  includes conventional ML monitoring and must not be conflated with Phoenix.

### Recorded decision: reuse architecture docs; do not publish benchmarks

The maintainer explicitly rejected including OTLP benchmark material in this
documentation change. Existing docs already explain verification and Bifrost;
link those pages rather than introducing another architecture explanation.
Keep the simple comparison and use cases focused on what readers can build
and why to choose Wyrd. Mention concrete differentiators briefly and direct
readers to existing detail.

Do not publish OTLP measurements, benchmark tables, throughput/latency
figures, methodology sections or numerical vendor rankings. Internal benchmark
records remain untouched; they are not a deliverable or an acceptance target
for this task. Do not replace the removed material with vague unsupported
superiority claims.

## Verification and Evidence

Run commands sequentially when they invoke Cargo. Planning does not execute
these implementation checks. Every additional specifically named test added
to the evidence must include its confirmed exact selector and setup.

1. **Focused discovery regression**, while iterating:

   ```sh
   mise exec -- pnpm --dir docs exec vitest run src/lib/derived-nav.test.ts src/lib/content.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts src/lib/readme.test.ts
   ```

   Confirms existing ordering, publication, index and shell contracts plus the
   new route. Install locked docs dependencies through `mise run docs:install`
   if needed. Avoid new checks that merely assert marketing wording.

2. **Shared prompt invariant**, after changing the source prompt:

   ```sh
   mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=mcp::tests::setup_prompt_names_its_release_and_holds_no_secret)'
   ```

3. **Existing live guide discovery**, using its owning migration lifecycle:

   ```sh
   mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples)"'
   ```

4. **Existing policy-artifact regression**, using the owning SDK journey setup:

   ```sh
   mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verifier_building_blocks::uploaded_artifacts_are_reused_by_service_judges)"'
   ```

   Proves the composition still works against the real server/Postgres with a
   scripted model, not real-model reasoning quality. Reuse cross-SDK evidence
   unless executable SDK/example changes require their owning exact journeys.

5. **Generation and final docs lane:** run `mise run docs:generate` from the
   owners, retain generated changes, then `mise run docs:check`. This covers
   generation drift, existing docs tests, commands, links, build and
   accessibility. Its generated-output cleanliness check must remain intact;
   Git actions remain caller-owned. Do not bypass a red gate or claim it passed
   when uncommitted generated output prevents completion. Run owner format/
   lint checks only for executable files actually changed.

6. **Public reader/visual evidence:** complete the policy workflow and eight
   fresh-context exercises above; inspect README badge readability, rendered
   comparison and all destinations. Verify the setup-prompt link separately:
   the docs content link checker does not cover that external Markdown source.

## Material Stop Conditions

Stop for a proposed runtime, wire, permissions, persisted-data or integration
change; a necessary capability that is only a contract rather than implemented
behavior; or contradictory Operator dispatch semantics not resolvable from
the owning contract and runtime. Keep discovered runtime work separate. Correct
stale vendor facts locally when the recorded intent remains
intact; do not weaken Wyrd's documented scope or fabricate comparative proof.

## Authority Links

[Shared spec revision 2 (draft)](../spec.md), [AGENTS.md](../../../../AGENTS.md),
[docs rules](../../../../docs/AGENTS.md),
[docs contributing](../../../../docs/CONTRIBUTING.md),
[docs product](../../../../docs/PRODUCT.md),
[protocol authority](../../../../architecture/wyrd-design.md),
[Bifrost](../../../../architecture/bifrost-design.md),
[security](../../../../architecture/wyrd-security-posture.md),
[agent surfaces](../../../../agent_docs/agent-surfaces.md),
[execution](../../../implementation-execution.md), and
[testing](../../../../TESTING.md).

## Implementation evidence

Spec revision 2 was still `draft` when this was implemented at the maintainer's
direct request. It only narrows revision 1 by removing benchmark publication.
Spec revision 3 (draft) moves the comparison to the README; where this task's
body describes the six-platform hub comparison, revision 3 governs.

### Reuse and gap

| Need | Existing owner reused | Gap closed |
| --- | --- | --- |
| Use-case hub | `get-started/` section, frontmatter-derived nav, landing `startPages` | One thin page `get-started/what-can-i-build.md` (order 4; coding-agent page moved to 5) |
| Machine onboarding | `generate_llms_txt.py` `START` list, published-only filter | Third `START` step; no new discovery logic |
| Agent onboarding | `setup-prompt.md` step 7, `GuideTopic::Start` docs step | Real hub link replaces "Choose your path"; guide names the hub after quickstart; wire contract unchanged |
| Policy workflow | Recipes Uploaded files, `fixtures/cards/uploaded_artifacts/handbook-*.yaml` | Added binding, Python call, citation inspection, `fs.read` grant, deterministic-rule and verify/submit guidance |
| Check selection | Recipes opening table | One row reworded to name policy decisions; no second table |
| Architecture / Bifrost | `understand/how-verification-works`, `bifrost`, `architecture` | Linked only; no new architecture section, no benchmarks |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, INV-001 | `README.md` opening, `get-started/introduction.md`, hub intro: production verification across AI/ML and workflows; app owns execution/enforcement | Source review; readers 1–4 state Wyrd does not orchestrate/enforce | PASS |
| REQ-002 | Six grey (`555555`) shields badges with `Use case: …` alt text, each linking a hub anchor | Shields SVG titles fetched (`Policy verification`, `Long-horizon workflows`); `docs:check` link and a11y pass | PASS |
| REQ-003, INV-002 | Spec revision 3: README `## Why Wyrd` (You get / So you don't have to, before Install) and `## Already have tools?` (You use / Keep it for / Add Wyrd for, seven category rows, after Example); hub comparison removed | Maintainer review of `comparison-table-review.md`; `docs:check` pass; readers 3–4 rerun, below | PASS |
| REQ-004, INV-002 | Why Wyrd rows name composed primitives, versioned checks run in CI and production, one data plane, whole-service versioning, gateway identity, self-hosted OTLP ingest; links to how-verification-works, bifrost, architecture; no figures | Source review: no benchmark or throughput content | PASS |
| REQ-005, INV-003 | `START` step 3, landing CTA, quickstart "simplest" + Next, coding-agent Next, intro Next, MCP guide, setup prompt link | RED: `indexes.test.ts` start-route and omission tests failed before the page existed; GREEN: focused vitest 62/62; `docs:check` 100/100 incl. draft exclusion; prompt URL `https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/` is the published route | PASS |
| REQ-006, INV-001 | Recipes Uploaded files completion reusing tested fixtures | `uploaded_artifacts_are_reused_by_service_judges` journey PASS (scripted model) | PASS |
| REQ-007 | Eight fresh-context readers, below; scenarios 3–4 rerun for revision 3 | Rerun: MCP 3 FAIL, MCP 4 and public 3 PARTIAL | FAIL |

### Commands

| Command | Result |
| --- | --- |
| `mise exec -- pnpm --dir docs exec vitest run src/lib/derived-nav.test.ts src/lib/content.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts src/lib/readme.test.ts` | RED 2 failed before page; then 62 passed |
| `mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=mcp::tests::setup_prompt_names_its_release_and_holds_no_secret)'` | 1 passed |
| `... wyrd-mcp --test mcp -P journey ... test(=discovery::pg_tests::agent_discovers_mcp_guide_and_sql_examples)` | 1 passed |
| `... wyrd-ai --test integration -P journey ... test(=verifier_building_blocks::uploaded_artifacts_are_reused_by_service_judges)` | 1 passed |
| `mise run docs:generate && mise run docs:check` | exit 0 after committing generated output (100 tests, links, build, a11y 31 pages) |
| `cargo fmt --check -p wyrd-server`, `git diff --check` | clean |

### Reader exercises

Model `claude-sonnet-5-5` via fresh `general-purpose` subagents, docs revision
`fb7db48c3`, 2026-11-03. Entry points: **public** = rendered `docs/build` HTML
only (no llms files), start at `index.html`; **MCP** = `wyrd.guide`
`start` text extracted verbatim from `GuideTopic::Start`, the setup prompt,
`llms.txt`, `llms-full.txt`. Each prompt restricted reads to that directory,
no web, no Wyrd execution, and asked for ≤300 words plus files read.

Questions: (1) "Our payment service approves refunds and payouts. Every payment
event must follow a policy document our compliance team maintains. How do we use
Wyrd to verify each payment decision against that policy?" (2) "We run an AI
coding and code-review workflow that runs for several hours across many steps.
How can Wyrd verify it?" (3) "Our models and experiment results already live in
MLflow. Can we use Wyrd, and how would it fit with MLflow?" (4) "We already run a
tracing/evaluation tool (Langfuse) and an LLM gateway in production. Should we
use Wyrd, and how would it fit with what we have?"

| Problem | Entry | Sources read | Answer summary | Rubric |
| --- | --- | --- | --- | --- |
| 1 Payment policy | public | index → what-can-i-build → verifier-recipes → how-verification-works → runtime limits | Upload policy Artifact `--extract`; Agent check mounts pinned version, `fs.read`, cites line; `assert`/`code` for limits; send decision facts; app approves/holds/escalates; `tool:invoke` grant; Wyrd does not enforce | PASS |
| 1 Payment policy | MCP | guide start → llms-full What can I build?, Recipes Uploaded files, How verification works | Same composition; `decide` combines; inconclusive ≠ approved; new Verifier version for new policy | PASS |
| 2 Long workflow | public | index → what-can-i-build → limits → verifier-card → recipes → record-a-run → bind-a-verifier → how-verification-works | Per-step verify with one `group_id`, grouped-session `group_end`, trace order check; verify/submit/scheduled; code/framework runs workflow. Missed that `after:` sets the quiet window | PASS |
| 2 Long workflow | MCP | guide start → llms-full hub, Recipes Grouped session/Trace, How verification works | Same, including `after: 10m`; Wyrd records and judges, does not orchestrate | PASS |
| 3 MLflow | public | grep → what-can-i-build | Keep MLflow as record; send chosen evidence with MLflow ID; no built-in connector, Wyrd reads nothing from MLflow | PASS |
| 3 MLflow | MCP | guide start → llms-full hub | Same; suggests an ID column in a custom table (labelled as its own suggestion) | PASS |
| 4 Existing tools | MCP | guide start → llms-full hub, LLM gateway, ASGI | Overlap in tracing/eval; broader verification role; nothing replaced; OTLP ingest; no Langfuse/gateway connector, no trace import | PASS |
| 4 Existing tools | public | index → introduction → set-up-the-llm-gateway → architecture → bifrost → runs-and-evidence → integrate-asgi → what-can-i-build | Overlap in tracing/eval and gateway; broader verification role; nothing replaced; OTLP ingest; no Langfuse/gateway connector; dual export or carrying trace IDs labelled undocumented/untested | PASS |

All eight answers meet their rubric; none invented a connector or made Wyrd the
workflow executor or enforcer. Only one omission (2/public: missed that `after:`
sets the quiet window) remained, and the recipe already states it, so no repair
was needed.

### Revision 3 reader rerun

Scenarios 3 and 4 were rerun after the comparison moved to the README. Model
`claude-sonnet-5-5`, docs revision `8b2ae1e69`, same prompts and restrictions.
The public entry was README plus the rendered build. The MCP entry was
unchanged: the README is not in it.

| Problem | Entry | Sources read | Answer summary | Rubric |
| --- | --- | --- | --- | --- |
| 3 MLflow | public | README → index → grep | Keep MLflow for training, experiments, registry; add Wyrd for runtime monitoring and verification of the deployed version; no connector invented; could not say how to link an MLflow model to a Model Card | PARTIAL |
| 3 MLflow | MCP | guide start → llms → setup prompt → llms-full (no MLflow match) | Inferred Wyrd's verification role and guessed a run-ID column, labelled unconfirmed | FAIL |
| 4 Existing tools | public | README → index → llms-full | Keep Langfuse and gateway; add Wyrd for composed verification; OTLP ingest, `telemetry=False`, gateway optional; no connector invented | PASS |
| 4 Existing tools | MCP | guide start → llms → llms-full → setup prompt | Same fit, by inference; found no comparison | PARTIAL |

REQ-007 fails for MCP onboarding. Agents read `llms.txt`, `llms-full.txt` and the
docs, not the README, so the comparison is invisible to them. The README row also
dropped the hub's "send evidence with the MLflow run ID" guidance.

### Limits and non-goals

- The MCP entry was simulated from the exact `wyrd.guide start` source and the
  generated indexes, because the session's Wyrd MCP server failed to connect.
- The policy workflow is proven by the scripted-model SDK journey, not by a run
  against a real provider at the documented release (no provider key was
  available). Real-model judgment quality is not established.
- Vendor facts rely on the task's 2026-11-03 official-source check (same day);
  they were not re-fetched. Revision 3 publishes no vendor citations.
- Excluded, as required: runtime, schema, MCP tool, and CLI changes; benchmarks;
  new architecture sections; vendor integrations; draft-page publication. No
  unrelated files changed. `review/TASK-001-review-02/` belongs to TASK-001 and
  is left untouched.
