# TASK-004 r6 repository standards review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Remediations: TASK-004 R1 through R5 in the preceding review directories
- Review mode: static and strictly read-only. No build, test, Cargo, `mise`,
  formatter, linter, code-generation, or executable runtime command was run.

The candidate remained the named commit when this report was written. The
repository has no `.codegraph/` index, so navigation used the committed
cumulative diff, current source, callers, manifests, generated projections,
and repository-native text search.

The fixed human decisions were treated as authority: Oracle graph-drain polling
and supervisor idle refusal remain deleted; follower grant-stream close is the
release and the leader awaits no acknowledgement; the foreign-tenant harness is
unchanged; a published `Running` step reserves attempt one and an interrupted
published step settles `Cancelled`; query tools consume the one-time-bound
prepared-run deadline and a shorter `deadline_ms` wins; and Revision 14 uses one
request variant per wire schema while preserving the wyrd-client local Vertex
refusal. The standing DRIFT direction was applied to every new mechanism,
check, file, setting, and option.

## Authority coverage

| Changed surface | Applicable authority read and applied | Coverage result |
|---|---|---|
| Active spec/task/remediation packet and recorded evidence | `AGENTS.md` §§1, 11-16; `architecture/agent-rules.md`; `architecture/references/README.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md`; `languages/maintainer-style.md` | **FAIL** — the approved spec is Revision 14 and explicitly assigns it to TASK-004, but the current task and R5 remediation metadata still identify Revision 13; see `STANDARDS-R6-001` |
| Shared-client Workflow loading and local gateway projection | `AGENTS.md` §§2-6, 9; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `doctrine/architecture-constraints.md`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | PASS — the shared client remains the SDK-facing client owner and its pre-existing local Vertex refusal is preserved without a compatibility route or second transport |
| Skald Prompt, provider request/response, native dispatch, provider clients, and Workflow routing | `AGENTS.md` §§2-6, 9-10, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/patterns.md`; `languages/rust-core.md`; `languages/errors.md` | PASS — `ProviderRequest` now has one variant per wire schema, `Prompt.provider` is the optional dispatch target, native dispatch resolves it once, and the existing provider clients remain the protocol owners |
| Python-visible Prompt accessors, generated stubs, and Python tests | `AGENTS.md` §§7-8, 11-12; `languages/pyo3-boundaries.md`; `languages/python-api-and-stubs.md`; `languages/testing-workflows.md` | PASS — the change narrows an existing approved owner-crate PyO3 migration surface, public stubs follow the source shape, and recorded codegen/typecheck/Python evidence covers the projection |
| Wyrd Prompt Card identity and generated JSON schemas | `AGENTS.md` §§2-4, 8-9, 11-12; `architecture/wyrd-design.md`; `architecture/patterns.md`; `languages/agent-harness.md`; `languages/testing-workflows.md` | PASS — the dispatch target participates in the Rust-owned Prompt content hash, generated schemas project the Rust source, and no generated artifact becomes an independent contract owner |
| Server Workflow host, accepted authority, Cards, tools, query collector, gateway adapter, configuration, retention, and shutdown | `AGENTS.md` §§2-6, 9, 11-12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md`; `architecture/patterns.md`; `languages/agent-harness.md`; `languages/errors.md`; `languages/testing-workflows.md` | PASS — the cohesive server owners, verified caller context, canonical audit decisions, bounded run state, stable errors, typed routes, and prepared-run deadline seam remain intact |
| External gateway bindings and outbound provider calls | `AGENTS.md` §§3-6, 9-10, 16; `architecture/agent-rules.md` SSRF rule; `architecture/wyrd-security-posture.md`; `architecture/patterns.md`; `languages/rust-core.md` | PASS — Revision 14 removes a redundant request variant rather than adding a compatibility layer; external dispatch reuses the existing typed protocol, screened endpoint policy, secret headers, retries, timeouts, redaction, and response limits |
| Oracle admission, cancellation, graph settlement, resources, and forwarded Workflow query journey | `AGENTS.md` §§3, 6, 10-12, 16; `architecture/bifrost-design.md`; `architecture/references/domain/vala-architecture.md`; `domain/olap-serving.md`; `domain/analytical-operations-reliability.md`; `languages/rust-core.md` | PASS — no production Oracle mechanism changed after R4; the R5 proof extends the existing journey and preserves grant-stream-close settlement, original-deadline ownership, cancellation, and sibling serviceability |
| Rust/server/Postgres/multi-pod/Python tests and nextest configuration | `AGENTS.md` §§4, 11-12, 16; `architecture/agent-rules.md`; `languages/testing-workflows.md`; `languages/spec-driven-development.md`; `languages/implementation-execution.md`; `languages/rust-core.md` | PASS — runtime-specific tests remain in their owning runtimes, production-shaped Postgres/server and Oracle journeys remain in earned external targets, and no new test binary, harness, checker, clock service, or custom coordination mechanism was introduced |

## Applicable rule results

| Repository rule | Exact source evidence | Result |
|---|---|---|
| Every active task identifies the approved specification revision from which it is derived | `architecture/references/languages/spec-driven-development.md:132-150`; approved Revision 14 at `changes/active/skald-workflow-runtime/spec.md:1-54,2760-2764`; stale task metadata/link at `tasks/TASK-004-accepted-server-jobs.md:1-7,433-439`; stale R5 task metadata/current-authority statement at `review/TASK-004-r5/TASK-004-R5-close-compatible-route-and-deadline-proof-gaps.md:1-20` despite its Revision 14 evidence at lines 169-205 | **FAIL — `STANDARDS-R6-001`** |
| Durable behavior stays with its established owner | Skald request and Prompt contracts remain under `skald-spec`; dispatch stays under `skald-runtime`; provider wire calls stay under `skald-providers`; server run lifecycle stays under `components/workflow`; Prompt identity stays in `wyrd-spec` | PASS |
| Stateful workflows use cohesive concrete structs and inherent methods | Existing `WorkflowExecutor`, `WorkflowExecutionDependencies`, `WorkflowRunHost`, `WorkflowRuns`, `RunTools`, `BoundedQuery`, and provider clients retain their state/dependency ownership; Revision 14 adds no manager, graph owner, cache, loader, or orchestration layer | PASS |
| Async is limited to IO or intentional async composition | Revision 14 planning and provider selection are synchronous; provider dispatch remains async only where it awaits transport or composes the Agent/Workflow IO path | PASS |
| Interface types use module-scope imports and bare names | Changed Skald and server modules keep their dependencies in top-level `use` blocks; no new function-local import or fully qualified interface signature was introduced | PASS |
| New and materially changed Rust items have substantive rustdoc | `skald-spec/src/request.rs:17-51,83-98`, `skald-spec/src/prompt.rs:28-54,163-174`, `skald-runtime/src/dispatch.rs:14-35`, `skald-providers/src/clients/vertex.rs:105-107`, and `wyrd-server/src/components/gateway/workflow.rs:177-190` document dialect/target ownership and errors | PASS |
| Public contracts remain typed, generated, and owned by Rust source | `ProviderRequest` is an adjacent-tagged Rust enum; `Prompt.provider` is typed as `Option<ProviderName>`; Prompt hashing includes it; schemas and Python declarations project those sources; recorded `codegen:check` is green | PASS (recorded evidence) |
| Python remains a projection rather than a second implementation | Existing `skald-prompt` migration wrappers remove obsolete Vertex request/response accessors and expose the Rust-native dialect/target distinction; no Python validation, dispatch, transport, or durable state owner was added | PASS |
| Provider-specific wire behavior stays behind provider owners | `VertexClient` accepts the Google GenerateContent schema at the Vertex endpoint; the server gateway selects Vertex ingress from the resolved model; external gateway route matching accepts the same Google body for Gemini and Vertex protocols | PASS |
| Verified identity, tenant isolation, authorization, audit, and secret handling remain owner-controlled | Revision 14 changes no caller/tenant derivation, permission, audit writer, SQL boundary, bearer retention, credential resolution, or secret-bearing type; the cumulative accepted-run source continues to delegate those decisions to existing owners | PASS |
| One prepared deadline owns the run and each query honors a shorter explicit deadline | `WorkflowExecutor` retains the sampled deadline, `PreparedWorkflowRun` exposes it, `RunTools` binds it once, and `QueryTool` applies the requested minimum; R5 extends the existing forwarded Oracle journey with omitted/longer/shorter inputs | PASS (source plus recorded journey evidence) |
| No gate is weakened or bypassed | No production `#[allow]`, ignored required test, boundary-glob widening, compatibility reader, duplicate audit path, or deleted assertion entered the Revision 14/R5 correction range | PASS |
| New mechanisms/checks/files/settings/options are established or broadly standard | R5 proof reuses the existing journey and Revision 14 deletes redundant wrappers/variants; no new dependency, setting, option, checker, timer task, channel, registry, adapter, protocol, fixture system, or compatibility path was added | PASS |
| User-facing behavior has production-shaped journey coverage | `pg_workflow_runs.rs::server_routes_keep_gateway_and_external_ownership` crosses stored Prompt hydration and direct external dispatch; `wyrd-testing`'s existing forwarded Workflow journey crosses `QueryTool` and Oracle for the deadline precedence matrix | PASS (recorded execution evidence) |
| Human-fixed boundaries remain excluded | Current source does not restore graph-drain polling, idle refusal, or follower release acknowledgement; it does not change the foreign-tenant harness, step-attempt semantics, prepared deadline owner, or local Vertex refusal | PASS |

## Material findings

### STANDARDS-R6-001 — active TASK-004 authority metadata still points at superseded Revision 13

- **Violated rule:** `architecture/references/languages/spec-driven-development.md`
  requires every task to identify the approved spec revision that authorizes it.
  The approved specification itself states that Revision 14 is carried by
  TASK-004.
- **Location:**
  `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md:6,435`
  and
  `changes/active/skald-workflow-runtime/review/TASK-004-r5/TASK-004-R5-close-compatible-route-and-deadline-proof-gaps.md:6,17-18`.
- **Evidence:** `spec.md:3,21-54,2760-2764` is approved Revision 14 and assigns
  its one-schema request/Prompt dispatch contract to TASK-004. The candidate
  implements that contract and appends a dedicated `Revision 14 evidence`
  section to R5 at lines 169-205, but the original task still declares
  `spec_revision: 13` and links “Approved Revision 13”; R5 also declares
  Revision 13 as its current task authority.
- **Consequence:** the active packet does not unambiguously establish which
  approved contract authorized the candidate's public request, Prompt,
  generated schema, and Python surface changes. A later implementer or reviewer
  following the task metadata can evaluate the Revision 14 code against the
  superseded Revision 13 contract, defeating the repository's immutable
  requirement-to-evidence chain. This is the same repository invariant that
  R2 previously repaired when Revision 13 superseded Revision 12.
- **Smallest testable correction:** update TASK-004's `spec_revision` and
  authority link to Revision 14. Update R5's front matter/current-authority
  statement to Revision 14 while preserving its historically accurate
  immutable R5 review input and candidate; add the same concise “later revision
  extended this remediation” distinction used by the established R1/R2
  precedent. Do not rewrite immutable review reports, earlier remediation
  history, or add a metadata checker, schema, setting, option, or other
  enforcement mechanism. Source review of the active packet is sufficient
  proof.

## Open questions

None.

## Verification notes

- Per the strict review constraint, this reviewer executed no build, test,
  Cargo, `mise`, formatter, linter, code-generation, or runtime command.
- Recorded final-tree evidence in the R5 remediation reports successful `fmt`,
  `lints`, `codegen:check`, client-tier and PyO3 boundary checks, Skald/shared/
  Wyrd/gateway/Oracle lanes, and Python and TypeScript unit/integration lanes.
- The exact R5 journeys are recorded green for the external OpenAI-compatible
  route and omitted/longer/shorter Workflow-query deadlines. Generated-artifact
  and runtime claims in this report rely on that evidence plus static source
  inspection, not independent execution.
- Static range inspection reports only three trailing-blank-line warnings in
  historical r2/r4 review Markdown; they do not alter executable behavior or an
  applicable repository contract and are not elevated to a material finding.

## Overall result

**FAIL**
