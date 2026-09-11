---
id: SPEC-verified-change-contract
revision: 3
status: draft
---

# Verified Change contract

## Objective and user value

Let a human or agent state what a proposed code change intends to do and have
reusable Verifiers judge that intent against the actual change and its test
results.

The user model is deliberately small:

```text
Change Request
├── subjects       what code is changing
├── claims         what behavior the author says will change
│   └── verifiers  how each Claim is checked
└── verifiers      how the whole Change Request is checked
```

A Verifier is a reusable Card that a user, team, repository, or organization
can define once and apply across Change Requests. Wyrd gathers the Evidence a
Verifier declares, runs it, and records whether it passed, failed, or could not
reach a conclusion.

## Scope

- Change Request creation, revision, and closure.
- One or more immutable repository subjects per revision.
- Optional pull-request creation or linkage for each subject.
- Claim-scoped and Change Request-scoped Verifiers.
- Repository- and organization-enforced Verifiers.
- `Verifier` as a seventeenth registrable Card kind.
- YAML authoring and normal Wyrd Card loading for Verifiers.
- Built-in, LLM-judge, Workflow, Python, API, and MCP Verifier tasks.
- Immutable Evidence and Verification Results bound to exact revisions.
- Durable execution of synchronous and long-running Verifiers.
- Equivalent human and agent workflows through CLI, HTTP, and MCP.

## Non-goals

- Requiring a pull request to verify a change.
- Requiring users to author Change Requests as YAML.
- A separate Evidence manifest, Verifier binding resource, capability selector,
  or generic semantic-parameter bag in the user model.
- General-purpose arbitrary code execution inside Wyrd. Python Verifiers are
  limited to the approved Monty execution boundary.
- A Verifier marketplace, optimizer, or automatic Verifier selection system.
- Verifier DAGs. A Verifier may run one existing Wyrd Workflow, but it does not
  define another task or workflow mechanism.
- A general-purpose external workflow or integration language.
- Treating verification as approval, merge, deployment, or authorization.
- UI layout or component behavior.

## Definitions

- **Change Request**: a long-lived, provider-neutral record of an intended
  change.
- **Change Request revision**: an immutable snapshot of the Change Request's
  exact subjects, Claims, and Verifiers.
- **Subject**: one repository change identified by exact repository, base
  commit, and candidate commit, plus an optional pull-request reference.
- **Claim**: a plain-language statement of behavior asserted by the change
  author.
- **Claim Verifier**: a Verifier attached to one Claim. It receives that Claim
  and the Evidence declared by its Verifier Card.
- **Global Verifier**: a Verifier attached to the Change Request revision rather
  than one Claim. It receives the whole change and its declared Evidence.
- **Evidence**: immutable material Wyrd supplies to a Verifier. The initial
  typed Evidence classes are `code_diff` and `test_results`.
- **Runtime context**: an optional dynamic JSON object supplied when a custom
  Verifier is requested and digest-bound to that run.
- **Verifier Card**: the reusable, versioned declaration of Evidence required
  and the task that judges it.
- **Verifier task**: one closed, YAML-authored task variant: built-in,
  LLM-judge, Workflow, Python, API, or MCP.
- **Verification Result**: one Verifier's immutable result for one exact Change
  Request revision and optional Claim.
- **Verification run status**: `queued`, `running`, `completed`, `cancelled`,
  `timed_out`, or `errored`.
- **Verifier verdict**: `passed`, `failed`, or `inconclusive`; present only for
  a completed run.

## Required behavior

### Change Request authorship

- **REQ-020**: `wyrd change open` MUST be the one human-facing CLI workflow for
  opening a Change Request. It MUST support no pull request, linking an
  existing pull request, and creating then linking a pull request.
- **REQ-021**: Pull-request choice MUST affect subject acquisition, not create
  separate Change Request types or workflows. A pull request MUST remain
  optional.
- **REQ-022**: Every subject MUST resolve to an exact repository, base commit,
  and candidate commit before Wyrd opens the revision. A pull-request number,
  URL, branch, or mutable head MUST NOT replace those immutable identities.
- **REQ-023**: `wyrd change revise` MUST support the same pull-request choices
  as `open`. With no new pull-request choice, it MUST refresh the existing
  linked pull request and every subject to their current exact revisions.
- **REQ-024**: A revision MAY contain subjects from multiple repositories or
  multiple subjects from one repository.
- **REQ-025**: Human CLI authorship MUST collect Claims and registered
  Verifiers without requiring a YAML file. HTTP and MCP MUST accept the same
  typed Change Request contract for agents and automation.
- **REQ-026**: MCP writes MUST require explicit write scopes. CLI, HTTP, MCP,
  SDK, and future UI surfaces MUST delegate to the same server-owned Change
  Request operations.

### Revision semantics

- **REQ-027**: A Change Request lifecycle MUST be `draft`, `open`, or `closed`.
  Closing records either `completed` or `cancelled`; verification state MUST
  NOT become lifecycle state.
- **REQ-028**: Opening or revising MUST create an immutable revision containing
  its exact subjects, Claims, Claim Verifier references, and global Verifier
  references.
- **REQ-029**: Changing a subject, base or candidate commit, pull-request
  linkage, Claim statement, Claim Verifier, or global Verifier MUST create a
  new revision. Evidence arrival and rerunning an unchanged Verifier MUST NOT.
- **REQ-030**: Prior Verification Results MUST remain attached to their
  original revision and MUST NOT silently satisfy a changed revision.
- **REQ-031**: Every revision-changing operation MUST use optimistic
  concurrency and idempotency so concurrent authors cannot silently overwrite
  one another and retries cannot create duplicate revisions.

### Claims and Verifiers

- **REQ-032**: A Claim MUST contain a statement and one or more Verifier Card
  references. There is no separate requirement object between a Claim and its
  Verifiers.
- **REQ-033**: A revision MAY contain zero or more global Verifier Card
  references outside its Claims.
- **REQ-034**: Repository and organization policy MAY add global Verifiers when
  a revision is opened. The resulting immutable revision MUST expose the exact
  resolved Verifier Card references and which policy level enforced each one.
- **REQ-035**: Every Claim Verifier MUST pass before its Claim is satisfied.
  Every Claim and every global Verifier MUST pass before the revision is
  Verified.
- **REQ-036**: Missing Evidence, an unavailable Verifier, an execution error,
  or an inconclusive result MUST NOT count as a pass.
- **REQ-037**: Verification and authorization MUST remain separate. A Verified
  revision is not implicitly approved, merged, promoted, or deployed.

### Evidence

- **REQ-038**: A Verifier task contract MUST determine the typed Evidence it
  needs. Built-ins own a fixed typed signature; customer-defined tasks declare
  their Evidence classes. Change Request authors MUST NOT separately wire
  sources or generic parameters to that Verifier.
- **REQ-039**: The initial Evidence classes MUST cover the exact code change
  between a subject's base and candidate commits, including optional
  pull-request context, and CI/CD test results produced for that exact
  candidate.
- **REQ-066**: `code_diff` MUST contain the full commit diff across the
  revision's exact Subjects. It MUST become available from the recorded base
  and candidate commits when a revision opens or changes, without requiring a
  pull request, so code-diff-only Verifiers can run immediately.
- **REQ-067**: Wyrd clients MUST expose one shared parser that converts the
  supported JUnit XML family into canonical `test_results` before upload. The
  server MUST accept and validate the canonical contract rather than own
  framework-specific report parsing.
- **REQ-068**: Canonical `test_results` MUST preserve execution status,
  summaries, and every normalized test case for bounded inputs. Parsing MUST
  be verified against representative Python, Rust, JavaScript, JVM, Go, and
  .NET JUnit producers or adapters. Oversized reports MUST be rejected
  explicitly and MUST NOT be silently truncated.
- **REQ-069**: A custom Verifier MAY accept dynamic `context` as a JSON object.
  Context MUST be supplied when verification is requested, MUST NOT be a
  static Change Request or Verifier value, and MUST be digest-bound to the
  resulting run.
- **REQ-040**: Wyrd MUST obtain declared Evidence from the resolved subject,
  configured repository/provider integrations, or an authenticated Evidence
  submission operation. Evidence producers MUST NOT declare a passing verdict.
- **REQ-041**: Evidence used for a result MUST be immutable and bound to the
  exact tenant, Change Request revision, subject, producer, candidate commit,
  and content digest.
- **REQ-042**: A Verifier MUST remain pending until all Evidence it declares is
  available. Incompatible, stale, ambiguous, or inaccessible Evidence MUST
  fail closed and MUST NOT be substituted silently.
- **REQ-043**: Test Evidence MUST preserve CI execution status, individual test
  outcomes, and enough producer provenance to prove it belongs to the
  candidate. Wyrd MUST distinguish failed tests from a failed or incomplete CI
  execution.
- **REQ-044**: Evidence disclosure to an external API or MCP server MUST be
  explicit, least-privilege, bounded, and auditable. Large or sensitive
  Evidence MUST NOT be embedded indiscriminately into external requests.

### Verifier Card

- **REQ-045**: Wyrd MUST add `Verifier` as a seventeenth registrable Card kind.
  It MUST use the normal `apiVersion`, `kind`, `metadata`, and `spec` envelope,
  normal CardRef identity, composite registration, schema generation, and
  `wyrd apply` loader behavior.
- **REQ-046**: A Verifier MUST be fully authorable in YAML in the same style as
  an Eval Card: the Spec declares or selects its typed Evidence requirements
  and exactly one adjacently tagged task whose `kind` selects a closed variant
  with variant-specific fields. It MUST NOT use a generic parameter or
  configuration object where typed fields can express the contract.
- **REQ-047**: The initial task variants MUST be built-in, LLM-judge,
  Workflow, Python, API, and MCP. `custom` MUST NOT be a separate runtime
  variant.
- **REQ-048**: A built-in task MUST select a server-owned typed implementation.
  Initial built-ins MUST support evaluating canonical CI/CD test results and
  whole-change code review. Vendor adapters MAY be built-ins when a vendor does
  not implement Wyrd's portable API or MCP contract.
- **REQ-049**: An LLM-judge task MUST reuse the established Eval composition of
  a constrained Agent judge that resolves its Prompt. It receives the optional
  Claim plus the declared Evidence and MUST return structured output. It MUST
  NOT gain production mutation tools or unrestricted context.
- **REQ-070**: A Workflow task MUST execute one Wyrd `Workflow` Card as the
  Verifier's single task. `workflow_ref` MUST accept the same local-path,
  registered CardRef, and inline-Spec authoring forms as `judge_ref`. The
  Workflow MAY contain multiple agentic steps but MUST accept one Verification
  Input and return one structured judgment.
- **REQ-071**: Workflow steps and intermediate outputs MUST remain internal to
  the Workflow. Wyrd MUST normalize only its final `verdict`, `summary`, and
  `findings` into the enclosing Verification Result. Skald is the Workflow
  execution engine and MUST NOT introduce a second public workflow format.
- **REQ-050**: An API task MUST declaratively describe how Wyrd invokes a remote
  API, shapes the canonical verification context into the API's expected
  request, determines synchronous or asynchronous acceptance, retrieves a
  long-running result, and maps the terminal response into Wyrd's common
  Verification Result.
- **REQ-051**: An MCP task MUST reference a registered `Mcp` Card and
  declaratively describe how canonical verification context becomes MCP tool
  arguments and how one or more tool results become Wyrd's common Verification
  Result.
- **REQ-052**: API and MCP customization MUST operate only over the canonical
  context Wyrd supplies: exact Change Request revision, subjects, optional
  Claim, and the Evidence declared by the Verifier. A custom mapping MUST NOT
  grant undeclared data access.
- **REQ-053**: API and MCP task fields MUST be sufficient for customer-defined
  request shapes without requiring the remote implementation to understand
  Wyrd Cards. The mapping contract MUST remain declarative and bounded; it MUST
  NOT become arbitrary executable code.
- **REQ-054**: API and MCP execution MUST use bounded requests, responses,
  concurrency, retries, and deadlines. Server URL fetching MUST follow Wyrd's
  effective-address SSRF screening and address-pinning requirements.
- **REQ-055**: Authentication MUST use server-resolved secret references;
  secret material MUST NOT appear in a Verifier Card. Runtime health and secret
  values MUST remain server-managed state outside the Card.
- **REQ-056**: One Verifier Card declares one task. Users compose independent
  judgments by attaching multiple Verifier Cards. A task MAY delegate one
  multi-step execution to a Wyrd Workflow, but this change MUST NOT introduce
  a Verifier DAG or another workflow mechanism.

### Durable execution and results

- **REQ-057**: Claim-scoped execution MUST provide the exact Claim statement
  and the Verifier's declared Evidence. Global execution MUST provide the whole
  Change Request revision and declared Evidence without inventing a synthetic
  Claim.
- **REQ-058**: Starting a Verifier through CLI, HTTP, or MCP MUST return a
  durable Wyrd verification-run identity without waiting for the Verifier to
  finish. The run MUST continue independently of the initiating connection.
- **REQ-059**: A synchronous external call MAY complete within one bounded
  server-side attempt. A long-running API or MCP Verifier MUST use a durable
  asynchronous completion contract; Wyrd MUST NOT hold the initiating client
  request open for a 20–25 minute external execution.
- **REQ-060**: Asynchronous retrieval MUST survive server restarts and bounded
  transient failures without duplicating the external job. The exact external
  run identity and retrieval progress MUST be durable and auditable.
- **REQ-061**: Built-in, LLM-judge, Workflow, Python, API, and MCP tasks MUST
  produce one common Verification Result shape.
- **REQ-062**: Every result MUST bind the exact tenant, Change Request revision,
  optional Claim, subject set, Verifier Card identity and version, Evidence
  identities and digests, execution status, verdict, summary, and structured
  findings.
- **REQ-063**: Execution status and verdict MUST remain independent. Only a
  completed execution may produce `passed`, `failed`, or `inconclusive`.
  Cancelled, timed-out, and errored executions have no verdict.
- **REQ-064**: Invalid LLM output, missing context, exhausted retries, or a
  judge abstention MUST produce `inconclusive`, never `failed` or `passed`.
- **REQ-065**: Applying the same Verifier Card to the same Claim or revision
  with the same Evidence digests MAY reuse its completed result. Changed
  Evidence or a changed Verifier Card version MUST require a new run.

## Invariants

- **INV-001**: The public user model remains Subject, Claim, Evidence, and
  Verifier; transport or implementation machinery does not become an authored
  fifth concept.
- **INV-002**: Claim-scoped and global Verifiers use the same Verifier Card and
  result contracts; placement determines whether Claim context is present.
- **INV-003**: A pull request is optional context, never immutable subject
  identity and never a prerequisite for verification.
- **INV-004**: Missing, stale, unavailable, errored, cancelled, timed-out, or
  inconclusive verification cannot become a pass.
- **INV-005**: Evidence never contains a verifier-authored verdict, and a
  Verification Result never rewrites its Evidence.
- **INV-006**: Verifier Cards contain no secret material or mutable runtime
  health.
- **INV-007**: Tenant isolation and transactional audit apply to Change
  Requests, revisions, Evidence, runs, results, external invocations, and
  enforcement decisions.
- **INV-008**: External execution never depends on the continued availability
  of the client connection that started it.
- **INV-009**: Eval task DAGs remain evaluation check sets; Wyrd Workflow Cards
  remain one-input, one-output agentic orchestration. Verifier does not merge
  or duplicate either mechanism.

## Acceptance obligations

- **AC-001**: Human user journeys open and revise a Change Request with no pull
  request, an existing pull request, and a newly created pull request.
- **AC-002**: Agent journeys perform the equivalent open, inspect, revise, and
  close workflow through scoped MCP tools and the shared server contract.
- **AC-003**: Revision journeys cover refreshed linked pull requests,
  non-pull-request subjects, multiple subjects, concurrent revision conflict,
  idempotent retry, and stale-result rejection.
- **AC-004**: Card loader and generated-schema evidence cover one real YAML
  fixture for every Verifier task variant, CardRef/path resolution where the
  final YAML permits it, unknown task kinds, unknown fields, invalid refs, and
  secret rejection.
- **AC-005**: Claim evaluation covers multiple Claim Verifiers, global
  Verifiers, repository enforcement, organization enforcement, missing
  Evidence, failed Evidence production, pass, fail, inconclusive, timeout, and
  execution error.
- **AC-006**: Built-in test verification proves evaluation of candidate-bound
  canonical test results; LLM-judge verification proves structured
  pass/fail/inconclusive handling.
- **AC-010**: Evidence journeys prove immediate full-commit `code_diff`, shared
  client-side JUnit normalization across representative framework fixtures,
  complete bounded 1,000-plus-test reports, oversized-report rejection, and
  dynamic context binding.
- **AC-011**: A Workflow-backed Verifier fixture proves local-path, registered
  CardRef, and inline Workflow authoring; one Verification Input; multi-step
  execution; and one normalized final judgment without exposing intermediate
  steps as Verifier results.
- **AC-007**: API journeys cover a customer-defined request body, selected
  Evidence disclosure, synchronous completion, asynchronous acceptance,
  durable polling/retrieval, restart recovery, timeout, malformed results, and
  common-result mapping against a local mock API.
- **AC-008**: MCP journeys cover customer-defined tool arguments, tool schema
  validation, synchronous completion, long-running start/retrieval tool
  behavior, malformed results, and common-result mapping against a local mock
  MCP server.
- **AC-009**: Security evidence covers tenant isolation, scoped writes,
  transactional audit, secret containment, bounded external IO, SSRF defense,
  least-privilege Evidence disclosure, and fail-closed handling.

## Open material decisions

The revision remains draft until these contracts are resolved together with
real customer-authored YAML examples. They are product and wire decisions, not
implementation details.

1. **Common Verifier YAML:** exact Evidence declaration, task field name, and
   CardRef/path/inline behavior. The result must use the repository's real
   Card/Eval YAML style rather than a prose pseudo-schema.
2. **Built-in YAML:** closed built-in names and the typed fields each built-in
   owns, including how CI providers/checks are selected.
3. **LLM-judge YAML:** whether the judge uses the existing
   `InlineableRef<AgentSpec>` unchanged, what context is supplied, and whether
   Verifier-specific instructions belong on the Verifier, Agent, or Prompt.
4. **Canonical injected context:** exact typed shape of Change Request,
   subject, optional Claim, and Evidence exposed to API request templates and
   MCP argument templates.
5. **Evidence delivery:** which Evidence is embedded, referenced by a bounded
   signed URL, summarized, or withheld; media types and size limits; and how a
   Verifier explicitly selects the minimum material it receives.
6. **API request definition:** supported HTTP methods, URL and header
   templating, authentication references, JSON body templating, typed value
   insertion versus string interpolation, and validation of customer-defined
   request shapes.
7. **API completion and retrieval:** the exact synchronous response contract;
   asynchronous job-id extraction; poll URL, method, headers, and body;
   terminal-status mapping; polling interval, backoff, timeout, cancellation,
   and whether callbacks are supported or deferred.
8. **API result mapping:** extraction and normalization of execution status,
   verdict, summary, and findings from arbitrary customer API responses,
   including unknown vendor states and malformed payloads.
9. **MCP invocation definition:** MCP Card reference, start tool selection,
   discovered schema validation, customer-defined argument mapping, and
   Evidence delivery to tool arguments.
10. **MCP completion and retrieval:** whether v1 requires one synchronous tool,
    permits a start tool plus a status/result tool, or supports both; how
    external run identity and terminal states are extracted and persisted.
11. **Customer-authored examples:** complete loader-valid YAML for passing CI
    tests, LLM code-and-tests Claim review, built-in whole-change code review,
    a custom synchronous API, a long-running API, a synchronous MCP tool, and a
    long-running MCP verifier. These examples must be agreed before field names
    become authoritative.
12. **Service surfaces:** exact HTTP request/response types and MCP tool names
    for Change Request authorship, Evidence submission, run control, and result
    retrieval.
13. **Enforcement:** where repository and organization Verifiers are declared
    and how a revision displays their server-added origin without allowing the
    author to remove them.
14. **Scheduling and cost:** when ready Verifiers run automatically versus by
    explicit request and how duplicate paid external or LLM execution is
    suppressed.
15. **Trusted base:** behavior when a candidate changes its own Verifier YAML,
    repository enforcement, or organization enforcement inputs.
16. **Workflow execution seam:** the exact Workflow input/output schema and
    server execution surface required to run registered or inline Workflows as
    one durable Verifier task.

## Material authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`
- `architecture/references/doctrine/positioning-and-vocabulary.md`
- `architecture/references/domain/evaluation.md`
- `architecture/references/languages/agent-harness.md`
- `architecture/references/languages/spec-driven-development.md`
- `crates/wyrd-spec/src/vala/eval/task.rs`
- `crates/wyrd-spec/src/card/workflow.rs`
- `crates/skald/skald-workflow/src/run.rs`
- `crates/wyrd/wyrd-cli/tests/fixtures/loader/end_to_end/agent1/eval.yaml`

## Revision history

- **Revision 1 split draft (2026-09-03):** Split the durable Verified Change
  protocol from the UI foundation work. Proposed capability selectors,
  separate Verifier bindings, Evidence envelopes and manifests, semantic
  parameter schemas, and explicit run modes. It remained unapproved.
- **Revision 2 simplification draft (2026-09-09):** Replaced the prior model
  with Subject, Claim, Evidence, and Verifier. Locked `Verifier` as the
  seventeenth Card kind; Claim-scoped and global Verifiers; optional, linked,
  or newly created pull requests; CLI/HTTP/MCP authorship; ordinary YAML Card
  loading; one Eval-style built-in, LLM-judge, API, or MCP task per Verifier;
  and durable asynchronous execution. Removed the unapproved capability,
  binding, manifest, generic-parameter, advisory, and DAG concepts. Exact
  customer YAML, injected context, external request/retrieval mapping,
  enforcement, scheduling, and trusted-base contracts remain open for explicit
  human refinement before approval.
- **Revision 3 evidence and Workflow draft (2026-09-10):** Locked full-commit
  `code_diff`, shared client-side JUnit normalization into canonical
  `test_results`, dynamic run context, and Workflow-backed Verifiers. Preserved
  one task and one verdict per Verifier while allowing that task to reference
  or inline one existing Wyrd Workflow with multiple internal agentic steps.
