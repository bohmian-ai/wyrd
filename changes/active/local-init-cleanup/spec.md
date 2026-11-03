---
id: SPEC-local-init-cleanup
revision: 1
status: approved
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
review on 2026-11-03. Only the two bounded tasks below are in scope:

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

### Keep the comparison simple and give Wyrd full credit

**REQ-003.** Publish a three-column comparison: **Platform**, **Main focus**,
and **Why choose Wyrd?** Include Wyrd, MLflow, Langfuse, LangSmith, Braintrust,
and Arize Phoenix. State that the latter four compete with Wyrd in the
LLM/agent observability/evaluation portion of its scope. Explain Wyrd's
broader integrated verification model using concrete capabilities, not a
generic list of evaluator features. MLflow covers conventional ML lifecycle
and GenAI tooling; explain both the overlap and keeping MLflow authoritative
for models/experiments while Wyrd verifies supplied evidence.

No OPA row, integration-status taxonomy, exhaustive feature-checkmark grid,
or larger vendor catalogue is included. LiteLLM and Guardrails AI are deferred
from this initial matrix. Do not equate Phoenix with the entire Arize AX
offering. Separate competitor feature documentation from evidence about
relative implementation depth, performance, or production reliability.

**REQ-004.** Follow the matrix with a clear explanation of why Wyrd's engine
differs: heterogeneous evidence; extraction/Agent/Code preparation; five check
kinds; dependencies, conditions, parallelism, retries and timeouts; structured
verdict data and citations; exact versions; individual and grouped evidence.
Explain relevant durable execution, isolation, admission and analytical
storage mechanisms, and publish the measured tracing results with context.
Keep implementation details where they help a reader choose Wyrd.

### Make the richer workflows discoverable

**REQ-005.** Publish a concise use-case hub, titled **What can I build?** or an
equivalent concrete title. Connect it from README badges, docs navigation and
landing/onboarding paths, the shared setup prompt, the coding-agent guide,
the existing MCP start guide, and the generated `llms.txt` onboarding route.
Keep setup then quickstart as the first two machine onboarding steps; make
capability selection the explicit next step. Replace the nonexistent
"Choose your path" instruction with a real link. The assertion quickstart
remains small and explicitly identifies itself as the simplest example.

**REQ-006.** Promote the existing policy Artifact → exact selected workspace
files → Agent check → Service-bound verification workflow into an obvious
worked path. Explain upload/extraction, policy versions, declared tools and
grants, supplied transaction/execution evidence, citations, and inline versus
queued evaluation. Link the existing grouped-session and drift workflows.
Add a compact selection table for Assert/Code, Agent, Trace, grouped evidence,
and scheduled Drift. Reuse existing tested examples and fixtures rather than
inventing a parallel tutorial implementation.

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
  capabilities and recorded measurements. Do not invent native MLflow/other
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
| AC-002 | REQ-003, INV-002 | Simple six-platform comparison gives Wyrd full credit, dates its primary evidence, and omits OPA, status labels and unsupported claims. |
| AC-003 | REQ-004, INV-002 | Engine/production claims match source owners; published metrics reproduce the correct records, units and workloads without invented vendor speedups. |
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
