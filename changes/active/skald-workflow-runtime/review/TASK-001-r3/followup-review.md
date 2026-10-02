# Focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `afdd8cd716c4529bd8cbb7fbe175bef55ef6ee1f`
- Approved specification: Revision 11 at `9a621a28a`
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation inputs: `review/TASK-001-r2/TASK-001-R1-close-validated-runtime-gaps.md` and `TASK-001-R1-addendum-revision-11.md`

The candidate remained the stated commit throughout this pass. No
`.codegraph/` directory exists. I read all eight discovery reports in this
review directory and inspected only the disputed paths named below. This is a
discovery pass, not a verdict.

## Paths and authorities inspected

- `changes/active/skald-workflow-runtime/spec.md`: retry/deadline contract,
  `REQ-016`, `REQ-042`, and `REQ-053`.
- `AGENTS.md` sections 4, 5, and 16; `architecture/agent-rules.md`;
  `architecture/references/languages/maintainer-style.md`; and
  `architecture/references/domain/telemetry-observations.md`.
- `crates/skald/skald-workflow/src/workflow.rs`, `route.rs`, and `output.rs`;
  `crates/skald/skald-agent/src/agent.rs`, `loop_runtime.rs`,
  `request_builder.rs`, `journal.rs`, and `tests/agent_timeout.rs`;
  `crates/skald/skald-providers/src/clients/external.rs`; the involved
  manifests; and the direct source callers found from those owners.
- `docs/architecture/skald.md`, `CHANGELOG.md`, `examples/rust/README.md`, and
  the corresponding base-to-candidate diffs.
- Official current OpenTelemetry GenAI span conventions and attribute registry:
  [GenAI spans](https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/gen-ai-spans.md)
  and [GenAI attribute registry](https://opentelemetry.io/docs/specs/semconv/registry/attributes/gen-ai/).
- The pinned `tracing-opentelemetry` 0.32.1 visitor in the local Cargo source,
  which projects ordinary `tracing` fields as primitive OTLP values and has no
  general `tracing::Value` path for a string array.

## Conflict resolution

### 1. `SYS-R3-001` and `CONC-R3-001`: one deadline-owner defect

**Resolution: REVISED and consolidated.** Both reports reach the same source:
`StepTask::run` fixes `agent_deadline` at
`workflow.rs:434-435`, but its biased select at `workflow.rs:445-463` races only
cancellation, total deadline, step-attempt deadline, and the Agent future. The
absolute Agent deadline is merely passed to `StepTask::attempt`; only the
WyrdGateway adapter consumes that deadline. Native and ExtGateway execution
start a fresh relative `tokio::time::timeout` later in
`loop_runtime.rs:187-193`.

The terminal journal case at `loop_runtime.rs:195-199` is a concrete reachable
demonstration: after the relative Agent timer expires, an arbitrary `Journal`
can keep the Agent future pending outside that timer. It is not a second root
cause. It is one of all the post-timer operations that the missing absolute
Workflow-owned deadline arm fails to contain. The behavior/invariant PASS
reports did not exercise either delayed first polling or a pending terminal
journal append, so their green cases do not falsify this path.

The exact correction boundary is the existing `StepTask` owner. Race the fixed
`agent_deadline` in the same biased select, after cancellation, total deadline,
and step deadline but before the Agent future. On expiry, feed the existing
Agent-timeout outcome through `AttemptOutcome::from_agent` so retryability and
the stable Agent timeout code remain unchanged. This one arm bounds Native and
ExtGateway execution, response normalization, output validation, and terminal
journal settlement and preserves the specified cancellation > total > step >
Agent precedence. Do not add a route-specific timer or another runtime. A
paused-time test must cover delayed first Agent polling and a terminal journal
that remains pending; one Workflow-level proof can close both source reports.

The standalone Agent's current relative timer historically bounds its loop body,
not the subsequent best-effort terminal journal append. Revision 11 requires
retaining Agent timeout behavior but does not redefine that standalone journal
contract. Therefore the follow-up does not require independently moving the
terminal append inside the Agent timer.

### 2. `STD-R3-001` / `TEL-002`: OpenTelemetry requirements and bridge limits

**Resolution: REVISED.** The provider, model, and finish-reason portions
survive, with different requirement levels; the asserted mandatory Google
operation-name change does not.

- For an inference span, current OpenTelemetry marks
  `gen_ai.operation.name` and `gen_ai.provider.name` **Required**. When a
  well-known provider value applies it must be used. `google` and `vertex` at
  `loop_runtime.rs:799-800` are therefore invalid; the typed Google and Vertex
  requests identify `gcp.gemini` and `gcp.vertex_ai` respectively.
- `gen_ai.request.model` is **Conditionally Required if available**. The
  resolved Prompt retains the exact model, yet `request_model` returns `None`
  for Gemini and Vertex and the span records an empty string. The model is
  available and must be supplied to each model-call span from the existing
  Prompt/request owner. This requirement applies directly to the `chat`
  inference span; the discovery claim that it must also be populated on the
  higher-level `invoke_agent` span is not needed to establish the defect.
- `gen_ai.response.finish_reasons` is **Recommended**, not required, but its
  registered type is `string[]`. The candidate emits a scalar string. The
  pinned `tracing-opentelemetry` bridge records ordinary `tracing` fields as
  primitive values and exposes no general string-array field path. The smallest
  conforming correction is therefore to delete this optional attribute rather
  than add a direct OpenTelemetry instrumentation path, JSON-encode an array
  under a `string[]` key, or add a dependency. Provider, operation, model, and
  stable error fields still satisfy REQ-053's required payload-free model-call
  span.
- `generate_content` is a well-known operation value, but the current official
  Google GenAI reference scenario itself uses `chat` for a Gemini
  `generate_content` call. The generic authority does not establish that every
  request using Google's GenerateContent transport must use
  `generate_content` rather than `chat`. That subclaim is rejected absent a
  provider-specific rule adopted by Wyrd.

The consolidated proof should cover the built-in provider mappings and exact
model fields, assert the optional finish-reason key is absent, and retain the
payload-exclusion assertions. No array-capable telemetry abstraction is needed.

### 3. `TEL-001`: panic settlement and span/run disagreement

**Resolution: CONFIRMED, with a narrower root-cause correction.** A
caller-supplied `AgentTool::invoke` at `loop_runtime.rs:454` is awaited without
a panic boundary. `StepTask::run` owns `AttemptSpan` inside the spawned task.
Unwinding drops the guard first, so `AttemptSpan::drop` records `cancelled`
without `error.type`; only afterward does `WorkflowExecutor::settle` receive the
`JoinError` and write a failed `WorkflowRun` with
`WYRD_WORKFLOW_500_INTERNAL`. The same attempt is observably cancelled in the
trace and failed in the authoritative result, contrary to REQ-053.

The correction belongs around the existing Workflow attempt future, before
the attempt guard can drop, not only around `AgentTool::invoke`. Catching only
the named tool panic would leave sibling reachable panic sources with the same
span/result mismatch. Convert an attempt panic to the already-selected
non-retryable `WorkflowInternal` outcome inside `StepTask::run`, then let the
existing `span.failed(...)` and normal settlement paths record the matching
code and result. The already-installed `futures` support is sufficient; no new
panic service or telemetry path is warranted. Retain true cancellation as
`cancelled` and prove both cases.

### 4. `NET-R3-001`: successful response credential reflection

**Resolution: CONFIRMED, with the correction narrowed to retained decoded
content.** REQ-042 is explicit that a bound secret value must never enter a
returned result; it is not limited to non-success bodies. A successful external
gateway response is untrusted, is decoded at `external.rs:96-126`, and its
retained strings flow through `AgentRun.output` into the public Workflow result.
The prior remediation deliberately closed refusal-body reflection only and its
test cannot prove the separate successful path.

The proposed boundary is correct: `ExternalGatewayClient` is the only owner
that has both the bound sensitive headers and the external response before it
becomes a general provider value. The scan must apply only to decoded content
that the typed `ProviderResponse` would retain, not arbitrary unknown response
members that deserialization discards. Recursively checking every retained JSON
string and member name for an exact secret-value substring is sufficient for
the specified direct/reflected value path, including text and structured/tool
payloads. Refuse with one fixed safe non-retryable provider error and never
include the body or matched value. This is intentionally fail-closed; short
operator-supplied secret values can cause false-positive refusal, but weakening
the exact REQ-042 prohibition or defining a minimum secret entropy would require
a specification decision rather than silently permitting disclosure.

A raw-byte-only scan is insufficient because JSON escaping can hide the same
decoded value. Scanning after the response has entered `AgentRun` is too late
and duplicates the guard across consumers. The focused proof needs valid 2xx
responses with reflected canaries in ordinary assistant text and retained
structured content, plus a clean success control and complete error/result
canary exclusion.

### 5. `STD-R3-002` and `MNT-R3-002`: rustdoc applicability

**Resolution: CONFIRMED and combined.** `AGENTS.md` section 16 and
`architecture/agent-rules.md` explicitly cover every new or materially modified
Rust item regardless of visibility, including test helpers, fields, trait
methods, and tests; every fallible function requires `# Errors`, with relevant
async cancellation and side effects documented.

`agent_timeout.rs` is a wholly new replacement test. Its undocumented
`RecordingJournal`, `RecordingProvider`, `FixedTool`, `ControlledTool`, and
`SlowTool` items, their fields/helpers, and their trait methods are in scope.
The partially documented `FailingTool` demonstrates that this is not an
external-test exemption. `loop_runtime::run` and `run_prompt` were materially
changed to replace Observer lifecycle work with tracing, so their one-line docs
also require actual error, timeout/cancellation, and terminal-journal behavior.
`validate_prompt_loop_request` and `assistant_message` have materially changed
fallible bodies and lack `# Errors`. No behavior or new abstraction is needed;
document the existing items in place. Unchanged surrounding helpers are not
pulled into scope merely because their callees changed.

### 6. Remaining maintainer proposals

#### `MNT-R3-001` — struct-centered Agent orchestration

**Confirmed.** `loop_runtime::run` and `run_prompt` were materially changed
(`125` additions / `173` deletions in the module), accept `&Agent`, repeatedly
read its provider override, Prompt, run configuration, journal, session,
callbacks, and tools, and coordinate the complete lifecycle. `Agent::run*` in
`agent.rs:662-696` merely forwards to those free functions. This is the exact
shape prohibited by the repository's hard struct-centered rule; the unchanged
pre-candidate shape is implementation drift, not precedent. Move only these
two orchestration bodies into a private inherent `impl Agent` in the existing
module. Stateless request, span, journal, and conversion helpers remain free
functions. Existing tests are sufficient behavioral proof.

#### `MNT-R3-003` — library `anyhow`

**Confirmed.** `output.rs:28` introduces the crate's sole source use of
`anyhow`, and `skald-workflow/Cargo.toml` adds the direct dependency. This
violates the explicit `AGENTS.md` rule reserving `anyhow` for binaries.
`SchemaResolverError` accepts an ordinary standard error, so a fixed
`std::io::Error` preserves the no-network refusal and lets the dependency be
deleted. No new error type is needed.

#### `MNT-R3-004` — stale permanent documentation

**Confirmed.** These are changed lines or immediate changed context, not
unrelated old prose:

- `docs/architecture/skald.md:25-30` was edited to remove `Observer` but still
  attributes deleted `WorkflowDef`, `Task`/`TaskDef`, `Context`,
  `execute_task`, and `MessageConversion` handoff to `skald-workflow`.
- `CHANGELOG.md:5-13` was edited for the Observer deletion but still advertises
  deleted `execute_task` and `MessageConversion` behavior.
- `examples/rust/README.md:17-25` removed the Observer example entry while
  retaining `tracing_stdout.rs`, even though this candidate deletes both that
  file and its Cargo bin target.

The task changes those exact public/runtime surfaces and REQ-053 explicitly
requires removing Observer examples and docs, so correcting these entries is
in scope. Replace the obsolete Workflow description with the implemented
explicit Agent DAG/binding/route/bounded-run ownership and delete the stale
example entry; do not add compatibility notes or a replacement example.

## Proposed finding reconciliation

| Discovery proposal | Follow-up disposition | Shared source / narrowed correction |
|---|---|---|
| `SYS-R3-001`, `CONC-R3-001` | **REVISED / consolidate** | One missing absolute Agent-deadline arm on `StepTask`; terminal journal pending is a reachability case, not a second owner. |
| `STD-R3-001`, `TEL-002` | **REVISED / consolidate** | Fix required provider values and available model; delete optional wrongly typed finish reasons; reject the unproven mandatory `generate_content` subclaim. |
| `TEL-001` | **CONFIRMED** | Catch the whole attempt panic before `AttemptSpan` drops, then reuse normal failed settlement. |
| `NET-R3-001` | **CONFIRMED** | Scan retained decoded external response content at `ExternalGatewayClient`; fail closed with a fixed safe error. |
| `STD-R3-002`, `MNT-R3-002` | **CONFIRMED / combine** | Document the new fixtures and materially changed fallible production items in place. |
| `MNT-R3-001` | **CONFIRMED** | Put the two stateful Agent workflows on `Agent`; leave stateless helpers free. |
| `MNT-R3-003` | **CONFIRMED** | Use a standard error and remove the sole direct library `anyhow` dependency. |
| `MNT-R3-004` | **CONFIRMED** | Repair the three changed permanent-documentation surfaces only. |

No new independent finding was introduced by this follow-up.

## Final status

**RESOLVED.** The discovery conflicts can be reconciled from approved authority
and current source. The behavior/invariant PASS reports missed reachable
deadline, panic, successful-reflection, convention, and repository-rule paths;
the specialist proposals remain material after the consolidations and rejected
subclaims above.
