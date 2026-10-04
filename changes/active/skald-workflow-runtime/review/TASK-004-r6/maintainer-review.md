# TASK-004 R6 Maintainer Review

## Immutable subject

- Base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Candidate: `5f3b521b5005c26277d53e7dfd2458c4f740e8be`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Task packet: `TASK-004` plus remediation tasks R1 through R5
- Review mode: source and recorded-evidence review only; no build, test, formatter,
  linter, generator, or production-source edit was run

The candidate remained the checked-out `HEAD` throughout this review. The
repository has no `.codegraph/` directory, so source, diff, and caller tracing
used direct repository reads. The fixed human decisions in the review request
were treated as authority, including the deleted Oracle polling/refusal model,
grant-stream-close release, the unchanged foreign-tenant harness, published
attempt-one semantics, the one-time prepared deadline bind, and Revision 14's
schema-versus-destination contract.

## Changed-surface coverage

| Changed surface | Owners, callers, and proof read | Maintainer assessment |
|---|---|---|
| Accepted server run lifecycle | `components/workflow/{host,runs,routes}.rs`, server state/configuration/boot and shutdown callers, native run DTOs, `pg_workflow_runs.rs` admission, replay, authority, lifecycle, retention, and bounds scenarios | Cohesive concrete ownership. `WorkflowRuns` owns process-local state and task tracking; `Preparation` and `AcceptedRun` express the ownership transfer without a second executor or durable controller. |
| Pinned graph and hydration | server Cards resolution, exact-reference reads, `WorkflowBodies`, `SkaldWorkflow::from_card_bodies`, registration/admission callers, shared-client Workflow hydration | The existing Cards and Skald owners remain visible. No duplicate client graph loader or compatibility hydration path entered the server. |
| Built-in tools and query collection | `components/workflow/tools.rs`, `query/collect.rs`, Cards and query service calls, `RunningQueryControls`, MCP collector consumers, prepared-run deadline binding, tool-path tests and Oracle journey | `RunTools` is a focused run-bound resolver and `BoundedQuery` is the shared collection owner. The one-time deadline bind uses ordinary one-time state and adds no clock service, option, or checker. |
| Skald execution and observable attempts | `skald-workflow` route, workflow, workflow-surface, ledger/task execution and tests; Agent tool-loop callers | Preparation, execution, transition publication, attempt reservation, cancellation, and terminal settlement remain discoverable through the Workflow owner. The fixed published-attempt decision is implemented at the transition owner rather than with a handshake or downstream repair. |
| Provider request, Prompt, and response contracts | `skald-spec` request/prompt/authoring/response owners; `skald-prompt` builders and Python wrappers; runtime dispatch; Agent loop/request builder; cache key; provider clients; gateway projections; local client gateway; route validation | Revision 14's single-variant wire-schema model is structurally simpler and consistently drives dispatch through `Prompt::provider()`. Two documentation defects remain; see `MAINT-R6-1` and `MAINT-R6-2`. |
| Gateway route ownership | Skald route resolution and external client, server in-process gateway projection, shared-client public gateway caller, provider-registry call sites, real-server route journey | Native, Wyrd-gateway, and external-gateway ownership remains explicit. No compatibility protocol, conversion layer, or extra response type was added. The unchanged local Vertex refusal was not re-litigated. |
| Oracle lifecycle and recovery | Vala Oracle admission, analytical supervisor, query stream, running/resource owners, server lifecycle controls and scheduling, peer-cluster Oracle workflow journey | The surviving lifecycle is followable as grant-stream close plus tracked settlement. Deleted idle polling, refusal, poisoning, and leader acknowledgement were not treated as missing mechanisms. |
| Rust/Python/generated contract parity | Rust wire types and Prompt hash, PyO3 request/response/Prompt projections, package exports, checked-in stubs and schemas, fixture migrations, parity tests named in the R5 evidence | Variant deletion and generated declaration shape are aligned. Python documentation already distinguishes request dialect default from Prompt dispatch destination, which exposes the stale Rust documentation in `MAINT-R6-1`. |
| Production-shaped journeys | `wyrd-server/tests/pg_workflow_runs.rs`, `wyrd-testing/tests/bifrost/oracle/workflow.rs`, Agent provider journeys, shared-client workflow transport tests, and the recorded focused/broader R5 results | The long server and Oracle files earn external-test placement: each brings up a real multi-crate server/topology and groups scenarios around shared expensive fixtures. Their scenario names, fixture methods, and outcome assertions keep the paths navigable; splitting them would add test binaries and duplicate setup without improving ownership. |
| Architecture, task, and configuration surface | Revision 14 spec, original task, R1-R5 remediation tasks, applicable Wyrd design/doctrine/Bifrost authority, `AGENTS.md`, agent rules, maintainer style, manifests/config diffs | No unsupported mechanism, repository check, dependency, setting, route alias, or public option was found. The new bounds and bindings are approved server behavior, not speculative configurability. |

## Material findings

### MAINT-R6-1 — Rust documentation still conflates response dialect with dispatch destination

- **Changed locations:**
  `crates/skald/skald-spec/src/response.rs:101-110` and
  `crates/skald/skald-agent/src/loop_runtime.rs:656-670`.
- **Governing rule/principle:** Revision 14 separates a request/response wire
  schema from its dispatch destination. `AGENTS.md` section 16 and
  `maintainer-style.md` require materially changed documentation to explain the
  non-obvious contract a maintainer must preserve, and public declarations in
  each language must describe the same operation.
- **Evidence:** `ProviderResponse::provider()` now necessarily returns
  `ProviderName::Google` for `GeminiGenerateContent`, including a response to a
  Vertex-targeted Prompt, because the response retains only the shared
  GenerateContent schema. Its rustdoc still says it returns the provider that
  produced the response. Separately, `invoke_agent_span` was changed from
  `prompt.request.provider()` to `prompt.provider()`, but its rustdoc still says
  the telemetry provider is derived from the typed request. The Python
  response/Prompt declarations already use the correct dialect-default versus
  destination wording.
- **Concrete maintenance cost:** A Rust caller or telemetry maintainer is told
  opposite things by adjacent source. Treating `ProviderResponse::provider()`
  as the actual destination misattributes Vertex GenerateContent responses;
  treating the Agent span value as request-derived can cause a future change to
  undo the approved custom-provider and Vertex attribution path.
- **Smallest testable correction:** Change only these two rustdoc blocks.
  Describe `ProviderResponse::provider()` as the response schema/dialect's
  default provider and explicitly state that a shared GenerateContent response
  does not retain whether Gemini or Vertex produced it. Describe the Agent span
  as recording the Prompt's effective dispatch target. Reuse the terminology
  already present on `ProviderRequest::provider()`, `Prompt::provider()`, and
  the Python declarations. Add no field, response wrapper, telemetry fallback,
  test harness, or repository check; the existing Vertex/custom-provider tests
  remain the behavioral proof.

### MAINT-R6-2 — The changed Prompt construction owner omits required failure and invariant documentation

- **Changed locations:** `crates/skald/skald-spec/src/prompt.rs:142-163` and
  `crates/skald/skald-prompt/src/builder.rs:240-244,273-324`.
- **Governing rule/principle:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` make substantive rustdoc mandatory for every
  new or materially modified Rust item, including private helpers, and require
  a `# Errors` section on every fallible function. The maintainer guide requires
  documentation to state the invariant or side effect that is not apparent
  from the signature.
- **Evidence:** `Prompt::new` was materially changed to establish the new
  `provider: None` state before normalization, but its fallible documentation
  has no `# Errors`. The Vertex builder's contract changed to build the shared
  `GeminiGenerateContent` schema with a Vertex destination, yet its one-line
  documentation still describes only a native Vertex prompt and omits errors.
  The materially changed private `google_prompt` and `finalize_prompt` helpers
  have no rustdoc or `# Errors`; critically, `finalize_prompt` must preserve and
  restore the optional destination across `Prompt::new`, which intentionally
  defaults it to `None`.
- **Concrete maintenance cost:** The save-and-restore at the single builder
  finalization seam looks redundant without its invariant. A routine cleanup
  can silently turn Vertex or custom-target Prompts back into dialect-default
  dispatch. Callers also cannot learn from the constructor/builder contract
  which model, settings, serialization, normalization, or placeholder failures
  are returned.
- **Smallest testable correction:** Add substantive rustdoc and exact
  `# Errors` sections to `Prompt::new`, `vertex`, `google_prompt`, and
  `finalize_prompt`. State that GenerateContent is the shared body schema, the
  optional provider is the destination, and finalization preserves that
  destination while performing existing normalization. Do not change the
  implementation, add a builder abstraction, introduce another constructor,
  or add a documentation checker. Existing Prompt round-trip and native
  dispatch tests remain sufficient behavioral proof; the normal Rust format
  and lint lanes are sufficient closure checks for the documentation-only
  correction.

## Calibration notes

- `pg_workflow_runs.rs` and the Oracle workflow journey are unusually large,
  but the repository explicitly permits external tests that wire multiple
  crates and real server/topology surfaces. Their shared fixture cost and
  scenario-level organization justify the file size; this is not a request to
  split them or add a fixture framework.
- `google_prompt(..., vertex_target: bool)` is a narrow private two-caller
  choice. An enum or strategy type would add vocabulary without improving the
  closed Gemini/Vertex decision, so it is not a material finding.
- Direct assignment to declarative `Prompt.provider` occurs at authoring
  boundaries and matches the approved public contract. A setter or destination
  wrapper would be an unearned abstraction, so none is requested.
- No DRIFT finding is reported. In particular, the candidate does not restore
  Oracle polling/acknowledgement machinery or add a deadline clock/service,
  compatibility layer, standalone check, option, or harness absent from
  established Wyrd and ordinary Rust practice.

## Verification limits

This review did not execute any code. It relied on the immutable cumulative
diff, current source and callers, checked-in generated declarations and
fixtures, and the focused/broader successful commands recorded in the R5
remediation evidence. Those results provide behavioral evidence but do not
waive the two source-documentation defects above.

## Overall result

**FAIL** — coverage is complete and the implementation shape is otherwise
maintainable, but `MAINT-R6-1` and `MAINT-R6-2` violate the repository's hard
documentation contract at Revision 14's central schema-versus-destination
boundary.
