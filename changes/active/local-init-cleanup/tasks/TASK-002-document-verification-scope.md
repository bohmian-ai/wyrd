---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-local-init-cleanup
spec_revision: 1
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
comparison, a source-backed engine/production/performance explanation, and a
prominent policy-artifact worked path. Connect existing entry points to these
pages and keep the first quickstart minimal. Reuse existing docs sections,
frontmatter and components; a new top-level section or navigation engine is
not required. Use existing tested source for copyable examples.

The hub gives each badge a concrete problem, evidence needed, suitable checks,
an existing worked path, and application responsibilities. Cover policy
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
2. Write the use-case hub and promote the existing policy-artifact workflow;
   provide the compact check-selection table and grouped/drift links.
3. Add the simple comparison and explain engine composition, production
   mechanisms and measured performance with their original context.
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

Badges, explanatory prose, comparison copy, prompt wording and benchmark
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
| REQ-004, INV-002 | Engine, durability, isolation, admission, gateway and Bifrost claims link to owners. Published benchmark numbers reproduce the specified records, units and workload; no unrelated/invalid measurement is relabelled. |
| REQ-005, INV-003 | Focused discovery regressions plus generated/rendered inspection establish every entry route, onboarding order and draft exclusion. The nonexistent next-step instruction is replaced by a working link. |
| REQ-006, INV-001 | The promoted policy path reuses tested fixtures/examples, explains exact selected files/tools/citations, and completes the existing public workflow. Group and drift routes and check-selection guidance are discoverable. |
| REQ-007 | All four reader problems meet their rubric through both entry paths in fresh contexts; recorded evidence supports comprehension without access to this packet or private source. |

## Implementation Reference

Reuse these inspected owners; paths guide the implementation, not a file
allowlist:

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

### Recorded performance evidence

Use the actual records at immutable benchmark-history commit `274cbfb83`,
measured source `92b62f9c0`, and the methodology in `benchmarks/README.md`:

| Workload | Recorded result approved for contextual publication |
| --- | --- |
| OTLP spans approximately 338 bytes, 64 clients | Approximately 267,603 spans/s; derived median 8,362.598 spans/s/vCPU × 32. |
| OTLP spans approximately 1,054 bytes, 64 clients | Approximately 130,298 spans/s; derived median 4,071.810 spans/s/vCPU × 32. |
| Published Parquet trace-ID lookup, one client | 4.551 ms server p99. |
| Published Parquet trace-ID lookup, 64 clients | 24.609 ms server p99; approximately 4,067 queries/s. |

[Write record](https://github.com/bohmian-ai/wyrd/blob/274cbfb83/bifrost/writes-parquet/20261031T125731Z-92b62f9c0.json)
and [query record](https://github.com/bohmian-ai/wyrd/blob/274cbfb83/bifrost/parquet/20261031T130839Z-92b62f9c0.json)
are VALID. Recheck exact fields and arithmetic before publishing. Context:
Linux, Ryzen 9950X, 32 logical CPUs, approximately 92 GiB RAM, local file
storage, three repetitions. Writes are short durable-ack windows, not sustained
publication throughput; trace lookup uses a settled 1.5-million-span fixture.
Retain payload, concurrency, server-latency scope, storage and source revision
near the figures or in an immediately linked methodology table.

Do not call the separate 2.206-million native rows/s result OTLP throughput;
do not call the 4.981-ms hot point lookup a trace lookup; do not promote the
driver-saturated INVALID direct-verification result as validated. No matched
vendor benchmark was found. Explain architectural distinctions and Wyrd's
measurements rather than inventing a numeric ranking. No rerun is required by
this documentation task.

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
stale vendor facts and metric labels locally when the approved intent remains
intact; do not weaken Wyrd's documented scope or fabricate comparative proof.

## Authority Links

[Shared spec revision 1 (draft)](../spec.md), [AGENTS.md](../../../../AGENTS.md),
[docs rules](../../../../docs/AGENTS.md),
[docs contributing](../../../../docs/CONTRIBUTING.md),
[docs product](../../../../docs/PRODUCT.md),
[protocol authority](../../../../architecture/wyrd-design.md),
[Bifrost](../../../../architecture/bifrost-design.md),
[security](../../../../architecture/wyrd-security-posture.md),
[agent surfaces](../../../../agent_docs/agent-surfaces.md),
[execution](../../../implementation-execution.md), and
[testing](../../../../TESTING.md).
