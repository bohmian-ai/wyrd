---
id: SPEC-local-init-cleanup
revision: 3
status: draft
---

# Local setup and verification documentation cleanup

## Intent and status

Wyrd's public introduction overemphasizes its smallest assertion example.
Readers and coding agents can discover tracing and scoring while missing the
broader, implemented verification system. A reader investigating a payment
workflow, for example, should discover versioned policy Artifacts, Agent
judges, deterministic checks, and accumulated execution evidence without
reconstructing that composition from architecture documents.

Local setup also sends agents to a retired docs origin, logs expected startup
readiness polls as ERROR, and describes gateway setup as skipped even when
existing deployments are configured. The startup checklist must explain what
is actually ready, configured and saved.

On 2026-11-03 the maintainer approved the documentation direction, then asked
to keep both tasks in the existing change packet and create one shared spec.
This revision consolidates those recorded decisions and the existing setup task.
The maintainer explicitly approved spec revision 1 when requesting TASK-001
review on 2026-11-03. Revision 2 records the subsequent direction to omit
public benchmark material and reuse existing architecture explanations.
Revision 3 moves the comparison into the README. Both remain draft. TASK-001's revision-1 obligations are unchanged. Only the two
bounded tasks below are in scope:

- [TASK-001 — local setup cleanup](tasks/TASK-001-local-setup-cleanup.md).
- [TASK-002 — verification documentation](tasks/TASK-002-document-verification-scope.md).

## Recorded decisions and required outcomes

### Lead with production verification infrastructure

**REQ-001.** Present Wyrd as production verification and assurance
infrastructure across AI/ML components and application workflows. Explain
agent tracing and evaluation as one competitive use case within that scope.
Cover policy/document judgments, structured application behavior, classical
ML/data drift, trace verification, and long-running workflows. Give the
composable verification engine, exact-version evidence, registry/artifacts,
gateway, and analytical data plane appropriate prominence. Do not frame Wyrd
as an agent-evaluation product with ancillary features.

### Make use cases visible in the README

**REQ-002.** Add six accessible, clickable, neutrally styled use-case badges
near the opening description: **Policy verification**, **Agent evaluation**,
**Long-horizon workflows**, **Runtime verification**, **Observability**, and
**Drift monitoring**. Each must lead to a relevant published explanation.
Introduce them with a concise statement covering outputs, policy decisions,
tool calls, and complete workflows. They are navigation, not certification or
passing-test indicators.

### Compare in the README, by reader job

**REQ-003.** Put the comparison in the README as two sections, one per reader
job. **Why Wyrd**, before Install, serves a reader choosing between tools: a
two-column table, **You get** and **So you don't have to**, that names Wyrd's
capabilities and no other product. **Already have tools?**, after the Example,
serves a reader fitting Wyrd into an existing stack: a three-column table,
**You use**, **Keep it for**, and **Add Wyrd for**, with one row per tool
category and its common products: experiment tracking (MLflow, Weights &
Biases), LLM observability (Langfuse, Arize Phoenix, LangSmith), eval
frameworks (promptfoo, DeepEval, Ragas), ML monitoring (Evidently, Fiddler),
AI gateways (LiteLLM, Portkey), observability platforms (Datadog, Grafana,
Honeycomb), and AI governance (Credo AI, IBM watsonx.governance). Every row
keeps the existing tool. The docs introduction repeats the Already have
tools? section, so coding agents reading the docs and `llms-full.txt` find it.

The use-case hub carries no comparison. No OPA row, integration-status labels,
feature-checkmark grid, or claim about another product's depth, performance,
or reliability is included. Each claim about another product matches that
product's public documentation at publication; citations are not published.

**REQ-004.** Make Wyrd's concrete differentiators easy to find from the
comparison and use cases: heterogeneous evidence, composed checks, exact
versions, cited structured verdicts, and individual/grouped evidence. Link to
the existing verification, Bifrost and operational explanations for engine and
architecture detail. Do not add another architecture section or duplicate
those explanations. No OTLP figures, benchmark tables, throughput/latency
numbers, benchmark methodology sections or vendor speedup rankings are to be
published as part of this documentation change.

### Make the richer workflows discoverable

**REQ-005.** Publish a concise use-case hub, titled **What can I build?** or an
equivalent concrete title. Connect it from README badges, docs navigation and
landing/onboarding paths, the shared setup prompt, the coding-agent guide,
the existing MCP start guide, and the generated `llms.txt` onboarding route.
Keep setup then quickstart as the first two machine onboarding steps; make
capability selection the explicit next step. Replace the nonexistent
"Choose your path" instruction with a real link. The assertion quickstart
remains small and explicitly identifies itself as the simplest example.
The hub is short problem-to-workflow routing, whether added to an existing
page or published as one thin page; it does not repeat technical explanations.

**REQ-006.** Promote the existing policy Artifact → exact selected workspace
files → Agent check → Service-bound verification workflow into an obvious
worked path by completing its existing Uploaded files recipe with Service
declaration, invocation with evidence, and inspection of the cited result.
Reuse the recipe's upload/version/workspace/tool explanations and link existing
concepts for check semantics and inline versus queued evaluation. Reuse/update
and link the existing Recipes selection table for Assert/Code, Agent, Trace,
grouped evidence, and scheduled Drift. Link the published grouped-session and
drift sections. No second policy tutorial or selection table is required;
reuse tested examples and fixtures rather than inventing parallel material.

**REQ-007.** Verify reader comprehension with four scenarios: a payment
workflow governed by a policy document; a multi-hour coding/review workflow;
MLflow-owned model/experiment evidence; and an application keeping its current
tracing/gateway tools. Exercise each through rendered public docs and through
MCP onboarding in fresh contexts. Record prompts, model/version, entry point,
documentation revision, retrieved sources and answers. Answers must select the
right composition, distinguish deterministic checks from Agent judgments,
explain application responsibilities, and avoid invented integrations. This
is review evidence, not a new model-dependent CI gate or evaluation platform.

### Correct local setup guidance and startup reporting

**REQ-008.** Active onboarding guidance must use
`https://docs.bohmian.ai/wyrd/`, preserving the docs base and machine-index
paths. Replace the retired GitHub Pages origin at its current source owner;
historical records are not rewritten and generated files are regenerated.

**REQ-009.** Expected startup `/readyz` polling while dependencies initialize
must not emit ERROR per response. Keep 503 until the existing readiness
conditions are satisfied. Preserve real dependency-failure diagnostics,
unexpected HTTP failures, readiness loss after startup, and the CLI's timeout
and premature-exit errors. Routine waiting may remain visible at DEBUG.

**REQ-010.** A restart that does not apply gateway configuration must report
existing tenant-scoped deployments instead of implying the gateway is absent.
An empty successful read means not configured; a failed read must fail or
report explicit uncertainty, never claim absence. Explicit gateway-file
application and first-run provider selection keep their current behavior.
Stored configuration is not a claim that a provider key works.

**REQ-011.** The readiness summary must provide a concise labelled checklist
understandable to people and agents: HTTP, gRPC and UI readiness; configured
storage and Bifrost paths; saved client configuration and credential-file
location; and actual gateway configuration. Preserve MCP host outcomes,
diagnostic log location and shutdown guidance. Existing JSON readiness fields
and types remain intact, in one readiness document, with a gateway summary
consistent with text. Checklist wording and markers remain implementation-owned.

## Invariants and boundaries

- **INV-001 — One product contract.** The Rust server owns durable behavior;
  clients project it. User code owns application execution and enforcement.
  Long-horizon verification does not make Wyrd a workflow orchestrator, and
  inline Agent judgments do not imply a fixed low-latency SLA or formal proof.
  Policy-document judgments do not assert an unverified general Policy Card
  enforcement runtime.
- **INV-002 — Evidence-backed claims.** Lead positively with implemented
  capabilities and source-backed differentiators. Do not invent native MLflow/other
  vendor connectors, arbitrary external reads, experiment-tracker feature
  parity, adoption, or head-to-head speedups. A competitor's product focus
  does not prove it cannot run custom code. Wyrd's bounded Code checks are not
  unrestricted Python; Agent preparation, tools and workspace composition are
  the broader capability.
- **INV-003 — One discovery path.** Extend existing navigation, shared prompt,
  MCP guide and index owners. Keep existing CLI/MCP names, schemas, scopes,
  prompt release heading, secret protections and generated-output ownership.
  Regenerate indexes; never hand-edit generated documents.
- **INV-004 — Honest readiness and diagnostics.** Quiet startup polls do not
  weaken readiness, hide real faults or globally lower HTTP failure logging.
  No second health system or startup state machine is introduced.
- **INV-005 — Preserve configuration and authority.** Reporting uses existing
  authenticated tenant-scoped clients; it does not replace credentials,
  reconfigure deployments, call a provider to validate keys, change stored
  settings or expose secrets. Configuration files are named, never printed.

Both tasks belong to this packet. Their shared MCP guide edit must converge
on one current URL and coherent capability guidance; there is no execution
order dependency. Card-registration logging, new adapters, benchmark runs,
new verification/runtime capabilities, a docs redesign and a new evaluation
harness are outside this change. The only executable server/CLI behavior
changes authorized by the proposed scope are startup logging and truthful
setup reporting.

## Acceptance and cost

| Acceptance | Obligations | Expected observation and evidence |
| --- | --- | --- |
| AC-001 | REQ-001, REQ-002, INV-001 | README presents Wyrd's full scope and six accessible use-case links; rendered review confirms relevant published destinations. |
| AC-002 | REQ-003, INV-002 | README Why Wyrd and Already have tools? match REQ-003, and the introduction repeats Already have tools?; product claims match public documentation; the hub has no comparison; OPA, status labels and unsupported claims are absent. |
| AC-003 | REQ-004, INV-002 | Differentiators match source owners and link to existing explanations; no new architecture exposition or benchmark publication is added. |
| AC-004 | REQ-005, INV-003 | Public and machine discovery reach the richer workflows, preserve setup/quickstart order and draft exclusion, and contain no nonexistent next-step link. |
| AC-005 | REQ-006, REQ-007, INV-001 | Existing policy workflow completes with cited evidence; all four reader scenarios succeed through both public docs and MCP onboarding. |
| AC-006 | REQ-008, INV-003 | Static inspection and generated-output checks find the current docs origin throughout active onboarding. No new URL-string test is required. |
| AC-007 | REQ-009, INV-004 | A startup sequence retains 503 without ERROR spam; ordinary failed requests and genuine readiness failures retain diagnostics. Focused regression covers both cases. |
| AC-008 | REQ-010, REQ-011, INV-005 | Restart with deployments reports them; empty and failed reads remain distinguishable. Text checklist and existing JSON agree, secrets remain hidden, and retained configuration still works. Existing init journeys plus narrow negative checks prove the behavior. |

The material costs are small startup/reporting corrections and their focused
regressions, documentation/source maintenance, refreshing vendor facts before
publication, and bounded reader exercises. No new dependency, persistent
state, public schema or vendor connector is authorized.
New navigation or index logic requires focused TDD; prose and static content
do not require manufactured tests.

No material product decisions remain open. If the Operator dispatch guidance
conflicts with the owning architecture, correct documentation to the
established contract only after checking the implementation; unresolved
runtime drift becomes a separate follow-up rather than a new claim here.

## Authority and revision history

Authorities: [repository rules](../../../AGENTS.md),
[protocol doctrine and Verifier contract](../../../architecture/wyrd-design.md),
[Bifrost](../../../architecture/bifrost-design.md),
[security posture](../../../architecture/wyrd-security-posture.md),
[docs product](../../../docs/PRODUCT.md), and
[docs rules](../../../docs/AGENTS.md).

- Revision 1, draft, 2026-11-03: consolidated the existing local-init cleanup
  and documentation tasks into one change specification at the maintainer's
  request. Recorded the previously agreed documentation direction and the
  bounded startup/reporting outcomes. Combined revision awaits human approval.
- Revision 1 approved, 2026-11-03: explicit maintainer approval recorded with
  the TASK-001 review request; no requirement or acceptance behavior changed.

- Revision 2, draft, 2026-11-03: recorded the maintainer's rejection of public
  benchmark material and clarification that existing architecture documentation
  should be linked rather than duplicated. TASK-001 runtime scope is unchanged.
- Revision 3, draft, 2026-11-03: moved the comparison from the hub to the README
  as Why Wyrd and Already have tools?, per the maintainer's review of the
  comparison table; replaced the six-platform list with tool categories and
  dropped published, dated vendor citations. TASK-001 scope is unchanged.
