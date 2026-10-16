# Proposal: Verifier Building Blocks

Status: direction and five simplifying rules approved in review on 2026-10-15.
Written specification: [spec.md](spec.md), revision 6, approved.
REQ-008 governs the built-in Agent tool registry, automatic runtime resolution,
and independent tool-invoke/resource authorization under the original subject
or valid delegation; older sketches below are subordinate.
Revision 2 retains batching and makes replay/conflict decisions asynchronous
through the existing execution status/wait surface; it does not put Postgres
writes back on the request path.
The specification incorporates the later requirements for Rust, Python, and
TypeScript examples and documentation, the working TypeScript demo, and
architecture updates. This proposal remains supporting design material.

## Summary

Make the Verifier one generic interface built from a small set of composable
building blocks, so users can verify any kind of work — agent output, uploaded
files, documents against a policy, code changes, model drift — by stitching
blocks together instead of choosing among fixed Verifier kinds.

A Verifier is:

- **inputs** — named, typed evidence slots;
- **steps** — an optional graph of work that produces data;
- **requirements** — named statements, each judged by one check, each producing
  one verdict; attached to a step or to the Verifier as a whole;
- **decide** — how requirement verdicts roll up to one overall verdict.

One rule makes the blocks predictable:

> Anything under `steps` produces data. Anything under `check` produces a verdict.

References into one scope document (`inputs.*`, `steps.<id>.output`,
`requirement.statement`) connect the blocks; the "Authoring model" section is
the contract authors reason with. An agent step runs in a small harness: the
existing tool loop, a per-call workspace (/inputs read-only, /work read-write), and four built-in tools.

The change consolidates existing machinery rather than rewriting it. The
`vala-eval` stage executor becomes the one generic orchestrator; the Workflow
Card, `skald-workflow`, and the `Drift` / `Eval` / `Task` implementation split
are removed.

## Decisions agreed

Agreed in review on 2026-10-15. Sections below carry the detail.

1. **One Verifier interface.** inputs + steps + requirements + decide. No
   `Drift` / `Eval` / `Task` implementation kinds.
2. **Actions** (produce data): `extract`, `agent`, `code`. **Checks** (produce
   a verdict): `assert`, `trace`, `agent`, `code`, `drift`.
3. **`code` is the action and check; `python` (monty-pool) is its runtime**
   (`CodeCall.runtime`, default `python`).
4. **Agents** are referenced inline, by file, or by registered Card
   (`InlineableRef<AgentSpec>`); the Agent owns its Prompt and
   `execution_route` (deployment-addressing TASK-001). Execution goes through
   `AgentInvoker` (today's `SkaldJudgeInvoker`), extended in this change to use
   `Agent::run_prompt`; deployment-addressing supplies route selection, not
   this tool-loop integration.
5. **Drift is a check** that keeps today's `DriftSpec` whole: PSI, SPC, Custom.
6. **`MediaFormat` enum** in `skald-spec`: short tokens (`pdf`, `pptx`, `ppt`,
   …), aliases, MIME parsing, byte sniffing; `ppt` ≠ `pptx`.
7. **One correct way per timing:** `verify` = real time; `submit` = all
   background work.
8. **`submit` routes on check type** through `CheckKind::evidence()`, an
   exhaustive match (`CheckEvidence::{Run, Window}`), never a boolean.
   `Run` → queued run picked up by `VerifierRunner`; `Window` (drift) →
   today's drift observation path, judged by the `schedule` Trigger.
   `verify` refuses `Window`. Registration refuses Verifiers that mix the two.
9. **Deleted:** `observe.drift`, `observe.eval` and the Eval observation flow
   (`observations_ready` activation, writer-Card binding fan-out, Eval
   observations table), `DirectVerificationInput`, immediate drift execution,
   the Workflow Card and `skald-workflow`. `ObservationEnqueue` /
   `ObservationRunOutbox` / `ObservationRunSink` are retargeted, not kept as
   is: they become `ExecutionInputsAck` / `ExecutionRequestOutbox` / `ExecutionRequestSink`.
10. **Inputs are named by the Verifier's slots** (`**inputs` / map), validated
    locally against the hydrated Verifier; the slot type picks the projection.
11. **Files go to object storage, never Postgres.** `wyrd.File` (path or bytes,
    optional `format=`) is a lazy local handle. `verify`/`submit` PUT missing
    digests to `evidence/sha256/{hex}` with a presigned `UploadPlan::SinglePut`
    through `WyrdStorageClient`; no upload row. Uploads are not registered as
    Artifact Cards; `wyrd.ArtifactRef` passes a file that is one.
12. **No Postgres write on the request path.** Admission writes nothing.
    `verify` executes in the request and stages inputs + outcome on
    `ScribeOutbox`. `submit` stages inputs on `ScribeOutbox` and returns
    `202 { execution_id }`; on Scribe's acknowledgement the batched
    `ExecutionRequestOutbox` inserts `verifier_executions` rows, which `VerifierRunner`
    claims. Only `submit` input records activate this queue; `verify` records
    evidence without requesting execution. Both use the same engine.
13. **Operators** fire on failed verdicts of background runs, never on `verify`.
14. **Two ids.** `run_id` (`RunId`) is the caller's invocation, minted by
    `state.run()`. `execution_id` (`ExecutionId`, replacing
    `VerificationRunId`) is one Verifier execution, minted by the server.
    Routes, table (`wyrd.verifier_executions`), and SDK handle
    (`VerificationExecution`) use `execution`.
15. **Authoring model.** One scope document; one dotted reference syntax with
    three roots (`inputs.`, `steps.`, `requirement.`); no `$.` and no bare
    `output`. Step requirements check one step's result right after it runs;
    Verifier requirements check the whole after all steps. Requirements never
    feed each other; `decide` counts every verdict once. Step verdicts are
    readable by `when` and action inputs, never by checks.
16. **Agent harness.** Agent blocks have two author-declared channels:
    `inputs:` (injected through Prompt placeholders) and `workspace:` (files
    under `/inputs`, read-only, fetched by Wyrd into a per-call scratch
    directory and mounted into monty; `/work` is a read-write mount over the
    same directory; all deleted when the call ends). Built-in tools assigned by the Agent Card's
    `tool_names`: `fs.list`, `fs.read`, `fs.search`, `code.run`. No network,
    no fetch tool. Check-position Agents get no tools and no workspace.
17. **Prompt shape follows deployment-addressing.** Agent output schemas are
    derived from the Prompt's `response_format` (or native output field), and
    injectable names from its `{{name}}` / `${media:name}` placeholders. No
    authored `variables`, `media_variables`, or `response_type`.
18. **Examples** live in [example.md](example.md).
19. **Monty.** `code` and `code.run` run on `monty-pool` 1.1 (with
    `monty-types`): Python executes only in pooled worker processes; mounts
    are passed per `Checkout::feed` and serviced on the parent by `monty-fs`
    through `cap_std` directory handles. The pool holds no files. `fs.*`
    tools are server Rust over the same per-call directory handle. Details
    under "Agent harness".
20. **Limits and threat model** are fixed under "Guardrails": concrete
    deployment defaults, per-threat defences, and the one residual risk.
21. **Task structure** is outlined under "Task structure": five tasks in this
    one change, including matched Rust/Python/TypeScript examples, the working
    TypeScript demo, docsite and architecture updates; replaced code is deleted when its
    last consumer moves. The demo in
    `wyrd-example-ts` shows `verify`, `submit`, an assertion, a file, an
    agent step reading that file, and an agent judge.

## Motivation

Wyrd verifies work against declared expectations. Today that is only
expressible for a narrow set of shapes, and each shape has its own contract.

Target use cases:

1. **PowerPoint agent.** A user's agent service writes decks. They want to
   verify each deck meets specifications: some checks are mechanical (slide
   titles, paragraph text, table contents), some need a model to read the deck
   (narrative quality, brand compliance).
2. **Upload-and-verify service.** A company runs a service where its clients
   upload work and the service verifies it, returning a breakdown.
3. **Document policy.** A company processes documents and must verify each one
   meets a policy, with a per-clause breakdown and evidence for every finding.
4. **Code review.** A multi-step review: extract scope, run several specialist
   reviewers, check requirements on each reviewer's output, consolidate a
   report.
5. **Drift.** Statistical checks over observation windows (existing behavior).
6. **Agentic evaluation.** Checks over agent traces and observations (existing
   behavior).

Longer term the same shape covers verification of non-AI work such as
underwriting: a file of evidence, a set of requirements, verifiers that judge
each one.

## Current state

| Concern | Today | Location |
|---|---|---|
| Verifier contract | `VerifierSpec { implementation: Drift(DriftSpec) \| Eval(EvalSpec) \| Task(TaskVerifierSpec) }` | `crates/wyrd-spec/src/card/verifier.rs` |
| Single real-time check | `Task` = exactly one `Assertion` or `LlmJudge` over JSON context + media | same |
| Multi-step checks | `EvalSpec.tasks` DAG of `EvalTask::{Assertion, LlmJudge, TraceAssertion, AgentAssertion}` with `depends_on`, `condition`, `pass_gate` | `crates/wyrd-spec/src/vala/eval/` |
| Second graph language | `WorkflowSpec.steps: Vec<WorkflowStep { id, action: Agent, depends_on, inputs: WorkflowBinding, timeout, retry }>` | `crates/wyrd-spec/src/card/workflow.rs`, `crates/skald/skald-workflow` |
| Executor | Stage loop with concurrent fan-out, growing `ContextSnapshot`, `SkipReason::{ConditionFalse, DependencySkipped}`, per-kind `Executors` | `crates/vala/vala-eval/src/executor.rs` |
| Drift | `method: Psi \| Spc \| Custom`, `signal`, `condition` (only `Statistical` accepted), `profile`; fitted state in `vala-drift` | `crates/wyrd-spec/src/card/drift.rs`, `crates/vala/vala-drift` |
| Direct input | `DirectVerificationInput::{EvalRecord, TaskContext, DriftSamples}` | `crates/wyrd-spec/src/verification.rs` |
| Result | `Judgment { verdict, summary, counts, detail: Drift \| Eval \| Task }`; per-check `AssertionResult { passed: bool, actual, expected, ... }` | same; `vala/eval/result.rs` |
| Multimodal | `MediaRef` → server `TenantMedia` → native provider image/document blocks; png/jpeg/gif/webp/pdf/text, 20 MiB, tenant prefix only | `crates/wyrd/wyrd-server/src/verification/eval.rs` |
| Triggers | `Schedule { cron, tz }` (fixed cron windows), `ObservationsReady {}` | `crates/wyrd-spec/src/card/trigger.rs` |
| Agent | `AgentSpec { prompt: InlineableRef<Prompt>, tool_names, run_config, verified_by }`; Prompt owns model, `variables`, `media_variables`, `response_type` (deployment-addressing derives these from the authored `response_format` and placeholders) | `crates/wyrd-spec/src/card/agent.rs`, `crates/skald/skald-spec/src/prompt.rs` |
| Agent route (in flight) | deployment-addressing TASK-001 adds required `execution_route: native \| wyrd_gateway { deployment }`, adds route selection to `SkaldJudgeInvoker`, relocates the gateway caller, and removes the Workflow product | `changes/active/deployment-addressing` |
| Media types | Free-form MIME strings; server allow-list is a string match in `TenantMedia::supported` | `crates/wyrd/wyrd-server/src/verification/eval.rs` |

Problems:

- Users must pick a kind before they can describe what must be true.
- Assertion and judge logic is duplicated across `Task` and `Eval` with
  different constraints.
- Two step-graph languages (Eval DAG, Workflow steps) with different binding
  and result models.
- No per-requirement breakdown with statements, citations, or evidence pinning.
- No way to do work (extract, run an agent, run code) before checking, except
  by leaving Wyrd.
- `agent_assertion` vs `llm_judge` vs Workflow `Agent` action: three ways to
  involve an LLM, each with different rules.

## Design principles

1. **One interface.** Every execution mode — direct call, bound Trigger,
   traces, files, drift windows, multi-step graphs, custom code — fits behind
   one `VerifierSpec`.
2. **Building blocks, not kinds.** A small closed set of blocks composes into
   any verifier. Users extend through blocks that accept their own Agents and
   code, not through new Verifier kinds.
3. **Position decides contract.** The same block (`agent`, `code`) can do
   work or judge; where it is placed decides whether it returns data or a
   verdict.
4. **Reuse existing Cards.** Agents are Agent Cards. Baselines are Data Cards.
   Nothing duplicates what a Card already owns.
5. **Consolidate, don't rewrite.** Keep the `vala-eval` executor, assertion,
   judge, trace, and media code; `vala-drift` engines; the `verifier_executions`
   claim loop; Triggers; Scribe outbox; Operators; SDK `verify` path.
6. **Verifiers compute and read; Operators write.** Steps never cause external
   side effects. Restarting a run is always safe.

## Building blocks

### 1. Inputs

Named, typed evidence slots the caller or a binding fills.

| Type | Holds | Resolution |
|---|---|---|
| `json` | Structured context, optional JSON Schema | Inline |
| `media` | Files (`formats: [pdf, pptx]`, `max_bytes`); see `MediaFormat` | Uploaded to tenant storage, pinned by sha256, read via `TenantMedia` |
| `trace` | A trace reference | Server fetches spans within the tenant |
| `samples` | Feature columns or a metric series | Inline (direct) or the Trigger's observation window |

Slots are `required: true` by default. A missing required slot never yields a
pass: requirements that depend on it are `inconclusive`.

### 2. Steps

Work that produces data.

```
Step { id, action, depends_on, when, requirements, timeout_seconds, retry }
```

- Each action declares what it reads with references (see "Authoring
  model"): `extract.from`, agent `inputs:` / `workspace:`, `code` `inputs:` /
  `workspace:`.
- Dependencies are derived from `steps.<id>` references; `depends_on` adds
  ordering without data.
- `when` is a skip gate with existing `EvalCondition` semantics over one
  reference.
- `retry` and `timeout_seconds` move here from `WorkflowStep`.

### 3. References

One dotted syntax reads the scope document everywhere: `inputs.<slot>…`,
`steps.<id>.output…`, `steps.<id>.status`,
`steps.<id>.requirements.<rid>.outcome`, and `requirement.statement`. The
full rules are under "Authoring model".

### 4. Actions (step position — produce data)

| Action | Does | Reuses |
|---|---|---|
| `extract` | Parses pptx/docx/xlsx/pdf into structure, text, counts | New; Rust parsers (`zip`, `quick-xml`, `lopdf`) |
| `agent` | Runs an Agent (inline, file, or registered Card) on its own `execution_route`; output shaped by the schema derived from its Prompt; may open files through a `workspace:` (see "Agent harness") | `skald_agent::Agent::run_prompt` via `AgentInvoker` (today's `SkaldJudgeInvoker`), `MediaResolver` |
| `code` | Sandboxed user code, JSON in → JSON out; `runtime: python` (the only runtime in v1); may read a `workspace:` | monty-pool |

### 5. Requirements

```
Requirement { id, statement, check }
```

- Attached to a step: checks that step's result right after the step
  succeeds, before any dependent step starts.
- Attached to the Verifier: checks the work as a whole, once, after every step
  has finished.
- A step with no requirements is pure work (extraction, preparation, a report).
- Requirements never feed each other; every verdict goes to `decide` once.

### 6. Checks (check position — produce a verdict)

| Check | Judges by | Reuses |
|---|---|---|
| `assert` | `path` or `each`, `operator`, `expected` | `AssertionTask` executor, `ComparisonOperator` |
| `trace` | Span existence, order, attributes over a `trace` slot | `TraceAssertionTask` executor |
| `agent` | An Agent returning a Verdict | `AgentInvoker` + `validate_judge_agent` constraints |
| `code` | User code returning a Verdict | Same runtime as the action |
| `drift` | PSI, SPC, or Custom over a `samples` slot; today's `DriftSpec` unchanged | `vala-drift` engines, baseline fitter, threshold semantics |

Drift is a check, not an action: each method already owns its decision rule
(PSI threshold, SPC control limits, Custom `alert_threshold`), so it produces a
verdict directly. Its full drift report is kept as the requirement's detail.

Checks read evidence only through references (see "Authoring model").

### 7. Decide

- Every requirement, at either level, counts exactly once.
- Exclude only `not_applicable` requirements: their step was intentionally
  skipped by `when`, or depends on an intentionally skipped step.
- If no requirements apply, the verdict is `inconclusive`.
- `all` (default): any `failed` → failed; else any `inconclusive` →
  inconclusive; else passed.
- `pass_rate { threshold }`: passed requirements divided by all applicable
  requirements, including inconclusive ones. At or above the threshold →
  passed; otherwise any inconclusive requirement → inconclusive; otherwise
  failed. For example, one pass and nine inconclusive results is 10%, not 100%.
- Requirements blocked by an upstream failure or timeout are `inconclusive`;
  they never leave the denominator.

## Authoring model

This section is the contract a Verifier author (human or agent) reasons with.
It is written to become the docsite page on building Verifiers; worked
examples are in [example.md](example.md).

### The scope document

Everything a Verifier reads comes from one JSON document that grows as the
execution runs:

```yaml
inputs:                         # filled by the caller (verify / submit) or a schedule window
  diff: { files: [...] }
  repo: <file>
steps:                          # one record per finished step, added stage by stage
  scope:
    status: succeeded           # succeeded | failed | skipped        (the step)
    output: { files: [...], changed_lines: 412 }   # what its action returned
    requirements:               # verdicts of its step requirements   (the step)
      reviewable-size: { outcome: passed, reason: "412 <= 800" }
requirement:                    # only while a check runs
  id: tested
  statement: Every behavior change has a covering test
```

### Names

Every step ID and requirement ID is unique across the whole Verifier,
including requirements attached to different steps. Step IDs and requirement
IDs share one namespace, so an execution-route override always names exactly
one block. Registration refuses duplicates. Results, overrides, and fitted
drift state use these same IDs; no qualified-ID syntax is needed.

### References

A reference is a dotted path into the scope document. There is one syntax and
it reads the same in every position.

| Reference | Value |
|---|---|
| `inputs.<slot>…` | What the caller sent for that slot |
| `steps.<id>.output…` | What that step's action returned |
| `steps.<id>.status` | `succeeded`, `failed`, or `skipped` |
| `steps.<id>.requirements.<rid>.outcome` / `.reason` | A step requirement's verdict |
| `requirement.statement` / `requirement.id` | The requirement being checked |

- `[n]` indexes an array. `[*]` is allowed only in an `assert` check's `each`.
- There is no `$.` prefix and no bare `output`. A step requirement on step
  `structure` writes `steps.structure.output.slides[0].title`.
- A step has exactly one action, so the action's output and the step's output
  are the same value: `steps.<id>.output`. The step's own state is
  `steps.<id>.status` and `steps.<id>.requirements`.

Where each reference is allowed, checked at registration:

| Position | May read |
|---|---|
| `extract.from`, agent `inputs:` / `workspace:`, `code` `inputs:` / `workspace:` on a step | `inputs`, and `steps.<id>` of any step it then depends on (output, status, and verdicts) |
| `when` | Same as the step's action |
| A step requirement's check | `inputs`, its own step, and that step's upstream steps; never step verdicts |
| A Verifier requirement's check | `inputs` and every step's `output` / `status`; never step verdicts |
| `requirement.*` | Checks only |

Checks never read verdicts. Verdicts reach the final result only through
`decide`, so no verdict is counted twice and there is one way to roll up.

### What `steps.<id>.output` contains

| Action | Output | Shape known at registration |
|---|---|---|
| `extract` | The locked `ExtractOutput` union in [spec.md, REQ-004](spec.md#locked-extraction-output-types). PDF: `{ format: "pdf", pages: [{ number, text }] }`. PPTX: `{ format: "pptx", slides: [{ number, title, paragraphs, tables, notes }] }`. DOCX, XLSX, CSV, JSON, text, Markdown, and HTML have concrete types there. Counts use array lengths; no extracted-image helpers. | Yes |
| `agent` | The Agent's final response, validated against the output schema derived from its Prompt (`response_format` in portable form, the native output field in native form). A string when no structured output is declared. | Yes, from the derived schema |
| `code` | The JSON the entrypoint returned | No |

When the shape is known, registration refuses a reference to a field that
does not exist. When it is not, a reference that finds nothing at run time
makes the requirement that used it `inconclusive`.

### How the graph runs

1. **Dependencies come from references.** A step that references
   `steps.X` (in its action or its `when`) depends on `X`. `depends_on` adds
   ordering without data. Cycles are refused.
2. **Stages.** Steps whose dependencies are all finished form a stage and run
   concurrently.
3. **Per step:** evaluate `when` → run the action (with its timeout and retry)
   → if it succeeded, run the step's requirements concurrently → write
   `status`, `output`, and `requirements` into the scope document.
4. The next stage starts only after the current stage's step requirements
   finish, so a later `when` can read an earlier verdict.
5. After the last stage, run the Verifier requirements concurrently.
6. `decide` combines every verdict once.

### Step requirements and Verifier requirements

Both are the same `Requirement { id, statement, check }`. Placement decides
when the requirement runs and what it can see.

- **A step requirement checks one step's result right after that step runs.**
  Use it for a claim about that step's product ("at most 15 slides" on the
  extract output, "no critical findings" on the security reviewer's output).
  Because it runs early, a later step can react to it with `when`.
- **A Verifier requirement checks the work as a whole, once, after every step
  has run.** Use it for a claim that spans steps or needs the raw input ("every
  behavior change has a covering test" needs the diff and the reviewer's
  findings).
- If the claim is about one step's result, attach it to that step. Otherwise
  attach it to the Verifier.

### How verdicts combine

Requirements never feed each other. Every requirement, at either level,
produces one verdict, and `decide` combines all of them once.

Use the rules in "Decide" above: exclude intentional skips, keep
inconclusive requirements in the pass-rate denominator, and return
`inconclusive` when none apply. `all` requires every applicable requirement
to pass. `pass_rate` compares passed / applicable against the threshold.

Data moves forward through `steps.<id>.output`. Verdicts move up through
`decide`. Those are the only two channels.

### When something is missing

| Situation | Effect |
|---|---|
| A step's `when` is false | The step is `skipped`; its requirements are `not_applicable` (left out of `decide`); steps that depend on it are skipped the same way |
| A step's action fails after its retries | The step is `failed`; its requirements are `inconclusive`; steps that depend on it are skipped and their requirements are `inconclusive` |
| A Verifier requirement needs output from a step intentionally skipped by `when`, including its skipped dependents | `not_applicable` |
| A Verifier requirement needs output from a failed or timed-out step, or a dependent skipped because of that failure | `inconclusive`; the check is not called |
| A required value is absent from an otherwise successful step | `inconclusive`; the check is not called |
| An optional input was not sent | The reference resolves to `null`; a `when` can test for it. A check that needs it is `inconclusive` |

The engine preserves why a step was skipped. If a requirement needs both
intentionally skipped evidence and evidence blocked by failure, failure wins:
the requirement is `inconclusive`. Reading a step's status alone does not need
its output and may still be checked.

Absence of evidence never passes.


## The agent rule

There is no separate judge concept. There is one `agent` block, and it uses
the existing Agent Card.

| Placed in | Returns | Server enforces |
|---|---|---|
| `steps[].action` | Data shaped by the output schema derived from its Prompt (`response_format`, or the native output field); a string when none is declared | Built-in tools named in the Agent Card's `tool_names`; may declare `workspace:` (see "Agent harness") |
| `requirements[].check` | A Verdict (`outcome`, `reason`, `citations`) | No tools and no `workspace:`; the derived output schema must equal the Verdict schema; a pass without valid citations becomes `inconclusive` |

The block names which Agent, what is injected (`inputs:`), and which files
it may open (`workspace:`). The Agent Card owns its Prompt (instructions,
model, output schema), tools, run limits, and `execution_route`.

- `inputs` keys must equal the placeholders derived from the Prompt
  (`{{name}}` for text, `${media:name}` for image and document blocks), in
  both directions; validated at registration. Per deployment-addressing,
  Prompts no longer author `variables`, `media_variables`, or
  `response_type`; shared compilation derives them.
- Media flows to `${media:name}` blocks through `MediaResolver`, and only in
  provider-native formats (pdf, png, jpeg, gif, webp, text).
- The requirement statement is passed explicitly via the
  `requirement.statement` binding — never injected implicitly.

`code` follows the same rule: data in a step, a Verdict in a check.

### Agent and Prompt references

Both use the existing `InlineableRef<T>` (`crates/wyrd-spec/src/reference.rs`),
which already has every form a user needs:

| Form | YAML | Resolved by |
|---|---|---|
| Inline | `agent: { prompt: {...}, execution_route: {...} }` | Embedded in the Verifier |
| File | `agent: ./agents/brand-reviewer.yaml` | Loader at registration (`InlineableRef::Path`) |
| Registered Card | `agent: { kind: Agent, name: brand-reviewer, version: 1.0.0 }` | `AgentCardResolver` (`InlineableRef::Ref`) |
| Sibling in the same submission | loader-projected | `InlineableRef::Sibling` |

The Agent's own `prompt` is an `InlineableRef<Prompt>` with the same four
forms, in portable or native authored form.

### Execution route

The Agent owns its route through the required `execution_route` introduced by
deployment-addressing (`changes/active/deployment-addressing`, TASK-001):

```yaml
execution_route: { kind: native }
execution_route: { kind: wyrd_gateway, deployment: openai-primary }
```

The Verifier does not add its own route field. Host rules:

- **Server-hosted runs** (direct `verify`, bound Triggers) require
  `wyrd_gateway`. The server holds no caller-local provider credentials, and
  the gateway supplies tenant credentials, governance, and audit. A `native`
  Agent is refused at registration of a binding and at direct execution, with
  a remediation naming the Agent and the deployment field to set.
- **Offline runs** (vala-eval independent of `wyrd-server`) accept both.
- **Per-run override.** The execute request may carry
  `execution_route_overrides: { <agent-step-or-requirement-id>: <route> }`,
  replacing the whole route for that one invocation, authorized and recorded,
  following deployment-addressing's override rule. The stored Card, hash, and
  version are unchanged.

### `AgentInvoker`: build on `Agent::run_prompt`, do not call it bare

deployment-addressing gives `SkaldJudgeInvoker` route selection and shared
Prompt compilation. This change renames it `AgentInvoker`, connects it to
`skald_agent::Agent::run_prompt`, and serves both step and check positions
through it. The tool-loop integration belongs here.

Calling `Agent::run_prompt` directly from the executor would scatter
server responsibilities across callers. The invoker is the one owner of:

1. Resolving `InlineableRef<AgentSpec>` and its `InlineableRef<Prompt>`
   (existing `AgentCardResolver`, `PromptCardResolver`).
2. Binding `inputs:` to the Prompt's derived placeholders (media through
   `MediaResolver` / `TenantMedia`), and materializing `workspace:` (see
   "Agent harness").
3. Choosing the effective route (Agent route, host rule, authorized override)
   and calling through the relocated gateway caller.
4. Applying the position policy: step position resolves the Agent's
   `tool_names` against the server's built-in tools; check position allows no
   tools, no workspace, no delegation, no session state
   (`validate_judge_agent`), requires a Verdict-shaped derived output schema,
   and validates citations against pinned evidence.
5. Executing `Agent::run_prompt` with the per-call deadline.
6. Recording provenance: Agent and Prompt identity and version, effective
   route, resolved deployment target.

```rust
/// Runs Agents for Verifier steps and agent checks on top of `skald_agent::Agent`.
pub struct AgentInvoker {
    agents: Arc<dyn AgentCardResolver>,
    prompts: Arc<dyn PromptCardResolver>,
    media: Arc<dyn MediaResolver>,
    gateway: Arc<dyn WyrdGatewayCaller>,
    tools: Arc<BuiltinTools>,            // fs.*, code.run, cards.get, bifrost.query
    workspaces: Workspaces,             // per-call scratch directories + monty mounts
    /// Resolved Agents keyed by exact identity; replaces today's single-entry cache.
    cache: Mutex<HashMap<ResolvedAgentKey, Arc<Agent>>>,
    call_deadline: Duration,
}

pub enum AgentPosition { Step, Check }

impl AgentInvoker {
    /// Resolve, bind, route, constrain by position, run, and record provenance.
    pub async fn invoke(
        &self,
        agent: &InlineableRef<AgentSpec>,
        inputs: &ResolvedInputs,
        position: AgentPosition,
        route_override: Option<&ExecutionRoute>,
        cx: &RunCx,
    ) -> Result<AgentInvocation, StepError>;
}

pub struct AgentInvocation {
    pub output: serde_json::Value,
    pub provenance: AgentProvenance,
}
```

Today's invoker caches a single Agent per run (`cached_agent`); a graph with
several reviewer Agents needs the keyed cache above.

## Agent harness

An agent step is a small, contained harness: the existing `skald-agent` tool
loop, a per-call workspace of files, and a fixed set of built-in tools. The
model never downloads anything and never touches the host.

### Two channels, both declared by the author

```yaml
action:
  kind: agent
  agent: { kind: Agent, name: security-reviewer, version: 1.0.0 }
  inputs:                                  # INJECTED into the model request
    diff: inputs.diff                      #   key = a placeholder derived from the Prompt
  workspace:                               # FILES the Agent's tools can open
    /inputs/repo: inputs.repo              #   key = virtual path, value = reference
    /inputs/scope.json: steps.scope.output
```

- **`inputs:`** keys equal the Prompt's derived placeholders, in both
  directions: `{{name}}` for text, `${media:name}` for image and document
  blocks. Media can be injected only in provider-native formats (pdf, png,
  jpeg, gif, webp, text) and within the attach limit.
- **`workspace:`** mounts each reference at a virtual path under `/inputs/`.
  Nothing in it reaches the model unless the Agent reads it with a tool.
- The same reference may appear in both channels. Which channel a value uses is
  whatever the author wrote; slot names carry no meaning to the system.
- The `code` action accepts the same `workspace:` block, so user code can read
  the same files.

### How a reference becomes a file

| Referenced value | File |
|---|---|
| A `media` input | Its pinned bytes at that path |
| A `media` input in an archive format (`tar_gz`, `zip`) | Unpacked as a directory under extraction limits (decompressed bytes, entry count, no links out) |
| A `json` input or a step output | A UTF-8 JSON file |

### Where the bytes land

Evidence lives in object storage; Monty mounts a local directory but does not
create or fill one. The scratch directory is where the evidence lands so that
Monty, `fs.*`, and `extract` all read the same local copy, fetched once.

1. Before the Agent loop starts, the runner fetches every workspace reference
   from tenant storage (digests pinned and authorized at admission) into
   `<scratch>/inputs/`, a **per-call scratch directory** on the server. The
   directory belongs to that one agent call, is size-capped, and is deleted
   when the call ends, whether it succeeds, fails, or times out. Fetching is
   eager, so the file set is fixed and recorded before the model acts.
   `<scratch>/work/` starts empty.
2. The server opens `<scratch>` as a `cap_std` directory handle. Every file
   access for the call goes through a handle, so the operating system refuses
   `..`, absolute paths, and symlinks that leave the directory.
3. `fs.*` tools are server Rust over that handle. `/inputs` is read-only to
   them; there is no write tool.
4. Each `code.run` (and each `code` action) checks out a worker from the
   shared pool and feeds the code with two `MountSpec`s: `/inputs` →
   `<scratch>/inputs` (`ReadOnly`) and `/work` → `<scratch>/work`
   (`ReadWrite`). Mounts belong to that one feed, not the pool. The worker
   never opens a file: a file operation suspends as `TurnEvent::OsCall`, and
   the parent services it with `Checkout::resume_from_mounts()` (`monty-fs`
   over `cap_std`). Any `OsCall` not covered by a mount, every
   `FunctionCall`, and every `NameLookup` is answered with an error. The
   checkout ends with `finish()` and the worker returns to the pool empty.
5. `/work` is `ReadWrite`, not Monty's `Overlay`: an overlay discards its
   writes at the end of each feed, so files would not survive between
   `code.run` calls. Files in `/work` are untrusted output: the server never
   executes or imports them, and `/work` is never on `sys.path`.
6. The model sees only virtual paths. Nothing in the workspace persists. The
   step's output is the Agent's final response.

Monty runs a Python subset: no class inheritance, generators, `match`, or
third-party packages; its standard library includes `json`, `re`, `os`,
`pathlib`, `datetime`, `hashlib`, `math`, and `collections`. Code parses text
formats (JSON, CSV, markdown, source, diffs) directly. Binary documents (pdf,
pptx, docx, xlsx) are parsed by `extract` first and mounted as JSON
(`/inputs/deck.json: steps.structure.output`).

The sandbox has no network and there is no fetch tool. External content
(a URL, a Google Drive document) enters as evidence: the caller passes
`wyrd.File(...)`, or later a `Source`-backed input.

### Built-in tools

The Agent Card assigns tools by name in its existing `tool_names`:

```yaml
kind: Agent
spec:
  execution_route: { kind: wyrd_gateway, deployment: anthropic-primary }
  tool_names: [fs.list, fs.read, fs.search, code.run]
  run_config: { max_iterations: 25, timeout_ms: 300000 }
  prompt: ./prompts/security.yaml
```

| Tool | Does | Reads |
|---|---|---|
| `fs.list(path)` | Lists entries | `/inputs`, `/work` |
| `fs.read(path, offset?, limit?)` | Returns text. For pdf, pptx, docx, and xlsx it returns the `extract` parser's text with page or slide markers, never raw bytes | `/inputs`, `/work` |
| `fs.search(pattern, path?)` | Regex over text files and extracted text; returns path, line or page, and the matching text | `/inputs`, `/work` |
| `code.run(source)` | Runs Python in a pooled Monty worker with `/inputs` (read-only) and `/work` (read-write) mounted; returns stdout and the last expression's value. Writes go only to `/work` | Same workspace |

- These join today's read-only built-ins (`cards.get`, `bifrost.query`). Their
  resolver moves from the deleted Workflow host
  (`wyrd-server/src/components/workflow/tools.rs`) into `AgentInvoker`.
- Tool output is truncated to a per-call byte cap.
- `fs.read` returns text only. When the model must see a page or an image, the
  author injects it through `inputs:`.

### Rules

- `workspace:` requires the Agent to declare at least one `fs.*` or
  `code.run` tool, and an `fs.*` tool requires a `workspace:`. Both are refused
  at registration otherwise.
- Check-position Agents may declare neither tools nor `workspace:`. A judge
  sees only injected values; large evidence is reduced by a step first.
- Nothing is injected implicitly, including no workspace manifest. The Prompt
  names the paths, or the Agent calls `fs.list("/inputs")`.
- Limits: `max_iterations` and `timeout_ms` from the Agent's `run_config`;
  everything else from deployment configuration with the defaults under
  "Guardrails".
- `/work` budget is per agent call. `MountSpec.write_bytes_limit` caps one
  feed, so before each `code.run` the server measures `<scratch>/work`, sets
  the limit to the remaining budget, and refuses the call when none is left.
- Host time spent servicing `OsCall`s is not covered by Monty's execution
  limits; the server enforces its own per-call `code.run` deadline on top of
  the pool's `request_timeout`.
- Provenance: every tool call (name, arguments, output digest, bytes returned)
  is recorded on the step result. Citations point to original input slots,
  not temporary workspace paths. The workspace manifest maps `/inputs` paths
  back to those slots. Derived output may carry the same citations forward;
  a downstream judge does not have to read the original file again.
- An Agent step that returns findings with citations declares them in its
  output schema using `Citation`. Authors bind the findings and citations to
  downstream Prompt placeholders explicitly through `inputs:`. This uses
  ordinary step output, with no new evidence wrapper or retrieval tool.
- The server validates citations against the pinned original input and limits
  them to source slots reachable through that call's input/workspace bindings.
  A valid locator proves the source exists, not that the finding is correct.
  A check-position Agent can reuse citations supplied with its evidence.

### Implementation

New workspace dependencies, used only by the server harness module:
`monty-pool` and `monty-types` 1.1 (`monty-pool` brings `monty-fs` and
`cap-std`; `cap-std` is also a direct dependency for the `fs.*` tools).
`tempfile` is already a `wyrd-server` dependency.

| monty-pool API | Use |
|---|---|
| `Pool` with `PoolConfig::subprocess` | One server-wide pool of prewarmed worker processes; a crashed or timed-out worker is killed and replaced |
| `Pool::checkout()` → `Checkout` | One fresh checkout per `code.run` / `code` action; `finish()` afterwards |
| `Checkout::feed(code, inputs, mounts, skip_type_check, on_print)` | Inputs become sandbox globals (`NamedValues` from JSON); printed output is collected |
| `MountSpec { mode: ReadOnly \| ReadWrite, write_bytes_limit, memory_usage_limit }` | `/inputs` and `/work` per feed |
| `TurnEvent::{OsCall, FunctionCall, NameLookup, Complete}` | `OsCall` → `resume_from_mounts()`; anything uncovered → error |
| `ResourceLimits` (`max_memory`, `max_feed_duration`, `max_turn_duration`, `max_suspensions`) and `request_timeout` | From deployment configuration |

Two facts task 3 must establish before relying on them: that `finish()`
leaves no interpreter state for the next checkout (another tenant), and
whether `PoolConfig::subprocess` lets the server set the worker's OS user,
environment, and network (see "Guardrails").

```rust
// crates/wyrd/wyrd-server/src/verification/workspace.rs
/// Fetches a step's `workspace:` references into per-call scratch directories.
pub struct Workspaces {
    media: Arc<dyn MediaResolver>,
    limits: WorkspaceLimits,
}

impl Workspaces {
    /// Resolve every reference against the scope document, fetch pinned bytes
    /// within the tenant, unpack archives under limits, and write JSON values.
    pub async fn materialize(&self, workspace: &Workspace, scope: &ContextSnapshot, cx: &RunCx)
        -> Result<MaterializedWorkspace, StepError>;
}

/// One call's files. Dropping it deletes the scratch directory.
pub struct MaterializedWorkspace {
    dir: tempfile::TempDir,
    manifest: Vec<MountedFile>,     // virtual path, digest, bytes: recorded as provenance
}

impl MaterializedWorkspace {
    /// `/inputs` read-only over `dir/inputs`, `/work` read-write over
    /// `dir/work` with `write_bytes_limit` set to the remaining call budget.
    pub fn mounts(&self) -> Result<[MountSpec; 2], StepError>;
}

/// One recorded tool call on a step result.
pub struct ToolCallRecord {
    pub tool: ToolName,
    pub arguments: serde_json::Value,
    pub output_sha256: Sha256Digest,
    pub output_bytes: u64,
}
```

`AgentInvoker` materializes the workspace, builds the `fs.*` tools over its
directory handle and `code.run` over the pool with its mounts for that one
call, runs `Agent::run_prompt`, and drops the workspace. The `code` action uses
the same `Workspaces` and pool.


## Contract proposal

### `crates/wyrd-spec/src/card/verifier.rs`

```rust
/// A Verifier: declared inputs, an optional step graph, requirements, and a decision rule.
pub struct VerifierSpec {
    pub description: Option<String>,
    pub inputs: BTreeMap<SlotName, InputSlot>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Judged after every step completes, over all inputs and step outputs.
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub decide: Decide,
    /// Replaces EvalSpec.context_capture: redact `actual` values in results.
    #[serde(default)]
    pub capture: CaptureMode,
}

pub struct InputSlot {
    #[serde(flatten)]
    pub kind: SlotKind,
    #[serde(default = "default_true")]
    pub required: bool,
    pub description: Option<String>,
}

#[serde(tag = "type", rename_all = "snake_case")]
pub enum SlotKind {
    Json { schema: Option<serde_json::Value> },
    Media { formats: Vec<MediaFormat>, max_bytes: Option<u64> },
    Trace,
    Samples { features: Vec<FeatureName> }, // measurement names; also used for metric samples
}

pub struct Step {
    pub id: StepId,
    pub action: Action,
    #[serde(default)]
    pub depends_on: Vec<StepId>,
    pub when: Option<Condition>,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    pub timeout_seconds: Option<u64>,
    pub retry: Option<RetryPolicy>,
}

/// Step position: produces data.
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Extract { from: Reference },
    Agent(AgentCall),
    Code(CodeCall),
}

/// One Agent invocation, in either position.
pub struct AgentCall {
    pub agent: InlineableRef<AgentSpec>,
    /// Injected: keys equal the placeholders derived from the Agent's Prompt.
    #[serde(default)]
    pub inputs: BTreeMap<VariableName, Reference>,
    /// Files for the Agent's tools. Step position only.
    #[serde(default)]
    pub workspace: Workspace,
}

pub struct Requirement {
    pub id: RequirementId,
    pub statement: String,
    pub check: Check,
}

/// Check position: produces a verdict.
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    Assert {
        path: Option<Reference>,
        /// The only position that accepts `[*]`.
        each: Option<Reference>,
        operator: ComparisonOperator,
        expected: serde_json::Value,
    },
    Trace(TraceCheck),
    /// No tools and no workspace; the derived output schema is the Verdict schema.
    Agent(AgentCall),
    Code(CodeCall),
    /// Today's DriftSpec unchanged: method Psi | Spc | Custom, signal, condition, profile.
    Drift {
        on: Reference,
        #[serde(flatten)]
        spec: DriftSpec,
    },
}

pub struct CodeCall {
    /// Execution runtime. `python` (monty-pool) is the only v1 runtime; the
    /// field exists so a second runtime is additive, not a contract change.
    #[serde(default)]
    pub runtime: CodeRuntime,
    /// Immutable source: an Artifact Card reference or inline source, never a server path.
    pub source: CodeSource,
    pub entrypoint: String,
    /// Values passed to the entrypoint in either position.
    #[serde(default)]
    pub inputs: BTreeMap<String, Reference>,
    /// Files mounted read-only at `/inputs`.
    #[serde(default)]
    pub workspace: Workspace,
}

#[serde(rename_all = "snake_case")]
pub enum CodeRuntime {
    #[default]
    Python,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Decide {
    #[default]
    All,
    PassRate { threshold: f64 },
}

/// A dotted path into the scope document: `inputs.<slot>…`,
/// `steps.<id>.output…`, `steps.<id>.status`,
/// `steps.<id>.requirements.<rid>.outcome`, or `requirement.statement`.
/// Parsed, and checked against its position, at registration.
pub struct Reference { root: ReferenceRoot, segments: Vec<Segment> }

/// Skip gate: the step runs only when the referenced value compares true.
pub struct Condition {
    pub path: Reference,
    pub operator: ComparisonOperator,
    pub expected: serde_json::Value,
}

/// Virtual path under `/inputs/` → the value mounted there.
#[serde(transparent)]
pub struct Workspace(pub BTreeMap<WorkspacePath, Reference>);
```

`Check::Drift` reuses today's `DriftSpec` (`crates/wyrd-spec/src/card/drift.rs`)
and its validation unchanged, so every current method keeps working:

| Method | Signal | Decision | Fitted state |
|---|---|---|---|
| `Psi` | `Distribution { baseline_ref, features }` | Per-feature PSI against `PsiThreshold` (`ChiSquare`, `Normal`, `Fixed`); any feature over threshold fails | Binned baseline in `vala-drift` |
| `Spc` | `Distribution` or `Metric` | Control limits from at least 20 complete baseline subgroups; insufficient target groups are inconclusive | Control limits in `vala-drift` |
| `Custom` | `Metric { name }` | Window mean absolute deviation from `baseline_value` against `alert_threshold` | None |

The only change is where samples come from: `on` binds a `samples` input slot,
filled only by a `schedule` binding's window of rows recorded through `submit`. The drift report
(per-feature scores, thresholds, flags, window) is kept as the requirement's
detail. `DriftCondition` variants that validation already refuses
(`Above`, `Below`, `Outside`) may be deleted as dead vocabulary; that is
independent cleanup, not part of this change.

### `skald-spec/src/media.rs`: `MediaFormat`

One shared enum replaces free-form media type strings in Verifier slots,
`MediaRef`, Prompt media binding, and the server's `TenantMedia::supported`
string match. Users write short tokens; full MIME strings are also accepted.
It lives in `skald-spec` next to `MediaKind` because Prompt media binding owns
media handling and `wyrd-spec` already depends on `skald-spec`.

```rust
/// A file format users name in slots and media references.
///
/// Deserializes from a short token (`"pdf"`, `"pptx"`, `"jpg"`) or a full MIME
/// type (`"application/pdf"`); always serializes as the short token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MediaFormat {
    Png,
    Jpeg,       // aliases: jpg
    Gif,
    Webp,
    Pdf,
    Text,       // aliases: txt
    Markdown,   // aliases: md
    Csv,
    Json,
    Html,
    Docx,
    Xlsx,
    Pptx,
    Doc,        // legacy binary Word
    Xls,        // legacy binary Excel
    Ppt,        // legacy binary PowerPoint
    TarGz,      // aliases: tgz; archive, unpacked by a workspace mount
    Zip,        // archive, unpacked by a workspace mount
}

impl MediaFormat {
    /// Canonical IANA media type.
    pub const fn mime(self) -> &'static str;
    /// Image or Document, for provider content-block selection.
    pub const fn kind(self) -> MediaKind;
    /// Accepted natively by provider multimodal input (png, jpeg, gif, webp, pdf, text).
    pub const fn provider_native(self) -> bool;
    /// Parsed by the built-in `extract` action (pdf, docx, xlsx, pptx, text, markdown, csv, json, html).
    pub const fn extractable(self) -> bool;
    /// Parse a token, alias, or MIME type.
    pub fn parse(value: &str) -> Option<Self>;
    /// Map a file extension.
    pub fn from_extension(ext: &str) -> Option<Self>;
    /// Detect from leading bytes; OOXML zips are distinguished by `[Content_Types].xml`.
    pub fn sniff(bytes: &[u8]) -> Option<Self>;
}
```

Rules:

- `ppt` and `pptx` are different formats. `ppt` is the legacy binary format;
  it can be declared and stored but is not `extractable` and not
  `provider_native`. Registration refuses a Verifier that feeds a
  `ppt`/`doc`/`xls`-only slot into `extract` or an Agent media variable, with a
  remediation to accept the OOXML format or supply a PDF rendition.
- `tar_gz` and `zip` are archives: neither `extractable` nor
  `provider_native`. They are usable only through a `workspace:` mount, which
  unpacks them under extraction limits. OOXML files are zips too; sniffing
  tells them apart by `[Content_Types].xml`.
- The server sniffs uploaded bytes and refuses a file whose detected format is
  not in the slot's `formats`. The declared type is never trusted.
- `provider_native` replaces the hard-coded list in
  `crates/wyrd/wyrd-server/src/verification/eval.rs::TenantMedia::supported`.

### `crates/wyrd-spec/src/verification.rs`

```rust
pub struct VerificationOutcome {
    pub verdict: VerificationVerdict,          // passed | failed | inconclusive
    pub requirements: Vec<RequirementResult>,  // the breakdown
    pub steps: Vec<StepResult>,
    pub evidence: Vec<EvidenceDigest>,         // slot, sha256, media type, size
}

pub struct RequirementResult {
    pub id: RequirementId,
    pub step: Option<StepId>,                  // None = verifier-level
    pub statement: String,
    pub outcome: RequirementOutcome,
    pub reason: Option<String>,
    pub actual: Option<serde_json::Value>,     // redacted under capture: redact
    pub citations: Vec<Citation>,
    pub checked_by: CheckProvenance,           // check kind + pinned Agent/Prompt version or code hash
}

pub enum RequirementOutcome { Passed, Failed, Inconclusive, NotApplicable }

/// Always names original evidence, even when carried through a step output.
pub struct Citation { pub slot: SlotName, pub locator: Locator }

pub enum Locator {
    Page(u32),
    Slide(u32),
    Lines { start: u32, end: u32 },
    JsonPointer(String),
    Region { page: u32, x: f32, y: f32, w: f32, h: f32 },
}

pub struct StepResult {
    pub id: StepId,
    pub status: StepStatus,                    // succeeded | failed | skipped | timed_out
    pub output: Option<serde_json::Value>,
    pub attempts: u32,
    pub duration_ms: u64,
    pub error: Option<StepError>,
    /// Harness provenance for agent steps; empty otherwise.
    pub tool_calls: Vec<ToolCallRecord>,
}
```

Execution status (queued, running, completed, errored, cancelled, timed out)
stays separate from the verdict, as today. A completed status may temporarily
have no stored result; completion does not acknowledge the Scribe write.
`wait(timeout)` returns only when the result is available, returns an execution
error for errored/cancelled/timed-out work, or returns a timeout if the result
never becomes available. It never invents an outcome from status alone.

### Running a Verifier: one pipeline, two entry points

#### Decision: two methods, not a `wait` flag

```python
result = run.observe.verify("deck-spec", wyrd.File("q3.pptx"))   # executes now  -> VerificationResult
job    = run.observe.submit("deck-spec", wyrd.File("q3.pptx"))   # queued        -> VerificationExecution
result = job.wait(timeout=600)                                     # same VerificationResult
```

These examples use Run evidence. For Window evidence, `submit` records
samples and returns `Recorded`; there is no individual execution to wait for.
Rust exposes this distinction as `Submission::{Recorded, Queued(...)}`;
Python and TypeScript project the same distinction.

- A `wait` flag changes the return type. Python needs `Literal` overloads,
  TypeScript needs overloads, and Rust cannot express it without returning an
  enum every caller must unwrap. Two methods give each a precise type in all
  three SDKs.
- This is the Temporal split: `execute_workflow` = `start_workflow` +
  `result()`. Here `verify` executes server-side in the request; `submit`
  returns a handle whose `wait()` polls.
- Both share every step except the last call. There is no second mechanism.

#### Decision: `wyrd.File` is a lazy local handle; it does not register a Card

`wyrd.File("q3.pptx")` does no IO. It names local bytes. `verify`/`submit`
stage it. Two reasons:

1. **Upload needs the Verifier.** The client validates the file against the
   slot's formats and size limit, and uploads only digests the server does not
   already hold. Constructing a `File` knows neither the slot nor the store.
2. **An uploaded deck is evidence, not a component.** Cards are versioned AI
   system components. An upload service verifying 10k customer files a day
   would register 10k Artifact Cards with invented names and versions, flood
   the registry, and tie customer-file retention to Card lifecycle. The run
   already carries the identity, principal, Verifier version, and lineage the
   Card would have supplied.

What is reused is the presigned `UploadPlan::SinglePut` and
`WyrdStorageClient`'s executor for it. Evidence is content-addressed
(`evidence/sha256/{hex}`), so dedupe is a storage HEAD and no upload row is
written; integrity and format are checked when the engine reads the bytes.
Bytes go client → object storage directly, never through the server or
Postgres.

A file that *is* a component (a policy PDF, a golden reference deck) is
registered as an Artifact Card the normal way and passed as
`wyrd.ArtifactRef("compliance/claims-policy@3.0.0")`.

#### What each step does

No Postgres write on the request path of either method. Run inputs and
results go through `ScribeOutbox` (batched Arrow into Bifrost); queued run
rows are created by a batched run-request outbox after Scribe acknowledges
the inputs, the same pattern today's Eval observation flow uses. Input records
carry a server-set `queue_execution` flag: false for `verify`, true for
`submit`. Only true records activate the queue. Recording evidence by itself
never requests execution.

```
client                         server (request path)                     background
──────                         ─────────────────────                     ──────────
1 resolve bound Verifier,
  match evidence (local)
2 project inputs by slot;
  hash files (local)
3 files only:
  POST /v1/verification/evidence ► presign SinglePut per missing digest
                                  (storage HEAD, no PG row)
  PUT bytes → evidence/sha256/{hex} (direct to object storage)

4 verify:
  POST /v1/verification/execute ► admit (token authz, cached Verifier,
                                  slot validation)
                                  VerificationEngine::run (≤ 60 s;
                                  reads files, checks sha256 + format)
                                  stage inputs + outcome on ScribeOutbox
  ◄ 200 { execution_id, run_id, outcome }                                               Scribe → vala.verification.results

4 submit (Run evidence):
  POST /v1/verification/enqueue ► admit (same)
                                  mint/reuse execution_id (replay key)
                                  stage inputs on ScribeOutbox
  ◄ 202 { execution_id }                                                        Scribe ack → ExecutionRequestOutbox
                                                                          → batched INSERT verifier_executions
                                                                          → VerifierRunner claims
                                                                          → engine.run → settle

4 submit (Window evidence, drift):
  rows → today's drift observation path (client Bifrost queue); the schedule Trigger judges the window

5 GET /v1/verification/executions/{execution_id} ► row status; outcome from results when completed
```

1. **Resolve.** `state.bound_verifier(subject, name)` and
   `spec.evidence()`. Refusals are local.
2. **Project.** Slot type picks the projection; files are stream-hashed.
3. **Evidence.** Only when a file is not already stored under its digest.
   The server presigns a single PUT to `evidence/sha256/{hex}` with the
   declared length; no upload row. Media is capped (20 MiB today), so
   multipart is unnecessary here.
4. **Run.** Admission is the same for both and writes nothing. `verify`
   executes in the request and stages inputs and outcome on `ScribeOutbox`.
   `submit` stages inputs and returns `202`; the run row is created
   asynchronously in batches. The engine checks each file's sha256 and sniffed
   format when it reads the bytes, so a wrong upload fails the run, never a
   silent pass.
5. **Status.** For queued submissions, `GET /executions/{execution_id}`
   reads the tenant-scoped row and, when completed, its stored result. A
   missing row returns not found; an SDK handle returned by `submit` treats
   that as not yet visible and keeps polling until its timeout. A UUID's age
   does not prove acceptance. Direct `verify` returns its result in the
   response and creates no queued-status handle.

`202` means accepted, not durable: as with every outbox in Wyrd, staged work
lost to abrupt process death is counted, never silently retried. The SDK's
`wait(timeout)` surfaces it as a timeout. The same applies when a run is
settled but its result never reaches Scribe. This change adds no new
persistence or result-recovery system.

Keep the existing accepted-authority and replay behavior when retargeting the
queue. Admission freezes the caller's effective authority and delegation
without credentials; queued reads and model calls use that snapshot, never
new authority from SYSTEM or the binding owner. Scheduled work keeps the
existing binding-owner acceptance rule. Carry this context through the input
record and queue handoff so it survives claim, retry, and restart.

For `/enqueue`, reuse the existing manual-run idempotency key and request-hash
rules, scoped by tenant, subject, and actor. The SDK retains the same key on
transport retries. A matching replay returns the same execution ID. As clarified by approved
spec revision 6, changed content under the same key is refused asynchronously
by the batched queue writer and surfaces as a terminal execution error, not
a synchronous enqueue refusal. The original execution is unchanged. Queue insertion deduplication remains
in place too; it handles internal retries, not HTTP replay. Preserve this
behavior with the existing queue/outbox machinery rather than adding another
job or durability system.

#### Wire contract (`crates/wyrd-spec/src/verification.rs`)

```rust
/// Body of `POST /v1/verification/execute` and `POST /v1/verification/enqueue`.
pub struct VerificationRequest {
    pub verifier_uid: CardUid,
    pub subject_card_uid: CardUid,
    /// The caller's invocation, for correlation with its observations.
    pub run_id: Option<RunId>,
    pub inputs: BTreeMap<SlotName, InputDeclaration>,
    #[serde(default)]
    pub execution_route_overrides: BTreeMap<StepOrRequirementId, ExecutionRoute>,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputDeclaration {
    /// Inline JSON for a `json` or `trace` slot, bounded by the request size limit.
    Json { value: serde_json::Value },
    /// A file already PUT to `evidence/sha256/{hex}`.
    File { sha256: Sha256Digest, size_bytes: u64, format: MediaFormat, filename: Option<String> },
    /// A registered Artifact Card's file.
    Artifact { artifact: CardRef, path: Option<String> },
}

/// `POST /v1/verification/evidence`: which declared files still need bytes.
pub struct EvidencePlanRequest { pub files: Vec<FileDeclaration> }
pub struct EvidencePlan { pub uploads: BTreeMap<Sha256Digest, UploadPlan> }   // SinglePut only

/// `execute` response; also embedded in a completed status.
pub struct VerificationResult {
    pub execution_id: ExecutionId,
    /// The caller's invocation, when the request carried one.
    pub run_id: Option<RunId>,
    pub outcome: VerificationOutcome,
}
```

| Route | Purpose | Postgres on request path |
|---|---|---|
| `POST /v1/verification/evidence` | Presign PUTs for missing digests | No |
| `POST /v1/verification/execute` | `verify`: run now, 200 `VerificationResult` | No |
| `POST /v1/verification/enqueue` | `submit`: 202 `{ execution_id }` | No |
| `GET /v1/verification/executions/{execution_id}` | Status; outcome when completed | Read only |

`DirectVerificationInput` and today's `ExecuteVerificationRequest` are
deleted.

Two ids, never interchangeable:

| Id | Type | Minted by | Means |
|---|---|---|---|
| `run_id` | `RunId` (existing, `wyrd-spec/src/vala/ids.rs`) | Client, `state.run()`, no network | One invocation of the caller's application; correlates its observations and verdicts |
| `execution_id` | `ExecutionId` (replaces `VerificationRunId`) | Server: admission for `verify` / `submit`, the scheduler for windows | One execution of one exact Verifier version |

One `run_id` may own many executions; a scheduled drift execution has none.

#### Server (`crates/wyrd/wyrd-server/src/verification/`)

```rust
/// Caller-facing verification: admission, real-time execution, and queued
/// submission. Holds no Postgres writer.
pub struct VerificationExecutions {
    admission: RunAdmission,       // token authz, cached Verifier resolution, slot validation
    engine: VerificationEngine,    // orchestrator + Executors
    scribe: Arc<ScribeOutbox>,     // run inputs + results, batched
}

impl VerificationExecutions {
    /// `verify`: executes in the request.
    pub async fn execute(&self, caller: &Caller, request: VerificationRequest)
        -> Result<VerificationResult, VerificationError> {
        let admitted = self.admission.admit(caller, request)?;            // sync: no IO beyond caches
        let outcome = tokio::time::timeout(EXECUTION_DEADLINE, self.engine.run(&admitted)).await;
        let result = admitted.settle(outcome);                           // timed_out on Elapsed
        self.scribe.stage(caller.tenant(), result.records());            // inputs (queue_execution=false) + outcome
        Ok(result.into_response())
    }

    /// `submit`: stages inputs; the run row follows Scribe's acknowledgement.
    pub fn enqueue(&self, caller: &Caller, request: VerificationRequest)
        -> Result<ExecutionId, VerificationError> {
        let admitted = self.admission.admit(caller, request)?;
        let id = admitted.execution_id(); // same ID on an accepted replay
        self.scribe.stage(caller.tenant(), admitted.queued_inputs_record(&id)); // queue_execution=true; accepted authority + replay context
        Ok(id)
    }
}

/// Gate acknowledgement hook for `vala.verification.execution_inputs` frames
/// (today's `ObservationEnqueue`, retargeted): queue only records whose
/// server-set `queue_execution` is true; recording `verify` never activates it.
pub struct ExecutionInputsAck { runs: Arc<ExecutionRequestOutbox> }

/// Today's `ObservationRunOutbox` / `ObservationRunSink`, retargeted: the
/// generic `Outbox` batches per tenant and inserts `verifier_executions` rows keyed
/// by (tenant, execution_id) with `ON CONFLICT DO NOTHING`.
pub type ExecutionRequestOutbox = Outbox<ExecutionRequestSink>;

impl VerifierRunner {
    async fn tick(&self) -> Result<(), VerificationError> {
        let Some(claim) = self.queue.claim_next().await? else { return Ok(()) };
        let inputs = self.inputs.load(&claim).await?;                     // execution_inputs by execution_id, or schedule window
        let outcome = self.engine.run(&inputs).await;                     // lease renewed alongside
        self.settlement.settle(claim, outcome).await                      // ScribeOutbox + row settle (existing)
    }
}
```

Schema changes:

- `wyrd.verifier_runs` is renamed `wyrd.verifier_executions`; `binding_id` nullable for submitted runs; unique
  `(tenant, execution_id)`. No input columns.
- New Bifrost fixed table `vala.verification.execution_inputs` (execution_id, run_id, verifier,
  subject, accepted authority and replay context, `queue_execution`, JSON
  values, file refs), replacing the Eval observations table. `verify` sets
  `queue_execution=false`; `submit` sets it true. This flag is server-owned,
  never supplied by the caller.
- `wyrd.storage_multipart_uploads` is untouched; evidence uploads create no
  row.

#### Client (`crates/shared/wyrd-client/src/observe/mod.rs`)

```rust
impl Observe<'_> {
    /// Real time. Window (drift) Verifiers are refused.
    pub async fn verify(&self, verifier: &str, inputs: impl Into<VerifyInputs>)
        -> Result<VerificationResult, WyrdError> {
        let bound = self.run.state.bound_verifier(&self.run.subject, verifier)?;
        match bound.spec.evidence() {
            CheckEvidence::Window => Err(drift_not_real_time(&bound)),
            CheckEvidence::Run => {
                let declared = self.upload_evidence(&bound, inputs.into()).await?;
                verify::execute(self.run.state.client()?, &declared.into_request()).await
            }
        }
    }

    /// Background. Window → drift observation path; Run → queued run.
    pub async fn submit(&self, verifier: &str, inputs: impl Into<VerifyInputs>)
        -> Result<Submission, WyrdError> {
        let bound = self.run.state.bound_verifier(&self.run.subject, verifier)?;
        match bound.spec.evidence() {
            CheckEvidence::Window => {
                self.record_samples(&bound, inputs.into())?;              // today's drift_value body
                Ok(Submission::Recorded)
            }
            CheckEvidence::Run => {
                let declared = self.upload_evidence(&bound, inputs.into()).await?;
                let id = verify::enqueue(self.run.state.client()?, &declared.into_request()).await?;
                Ok(Submission::Queued(VerificationExecution::new(self.run.state.client()?, id)))
            }
        }
    }

    /// Project by slot type, then PUT only files the server does not hold.
    async fn upload_evidence(&self, bound: &BoundVerifier, inputs: VerifyInputs)
        -> Result<DeclaredInputs, WyrdError> {
        let declared = inputs.declare(bound, &self.run.run_id).await?;
        if declared.has_files() {
            let client = self.run.state.client()?;
            let plan = verify::plan_evidence(client, &declared.files()).await?;
            let storage = WyrdStorageClient::new(client);
            for (digest, upload) in plan.uploads {
                storage.put(&upload, declared.source(&digest)?).await?;
            }
        }
        Ok(declared)
    }
}

pub enum Submission { Recorded, Queued(VerificationExecution) }

/// Handle for a queued run; rehydratable from its id in another process.
pub struct VerificationExecution { client: WyrdClient, id: ExecutionId }

impl VerificationExecution {
    pub fn id(&self) -> &ExecutionId;
    pub async fn status(&self) -> Result<ExecutionStatus, WyrdError>;
    /// Polls with capped backoff until the result is available, execution
    /// fails, or `timeout`; a completed row alone is not a result.
    pub async fn wait(&self, timeout: Duration) -> Result<VerificationResult, WyrdError>;
}
```

Python and TypeScript project the same three operations:

```python
result = run.observe.verify("deck-spec", wyrd.File("q3.pptx"))
result = run.observe.verify("deck-spec", deck=wyrd.File("q3.pptx"), rendered=wyrd.File("q3.pdf"))
result = run.observe.verify("claims-policy", document=wyrd.ArtifactRef("compliance/sample-claim@1.0.0"))

job = run.observe.submit("code-review", diff=wyrd.File("pr.diff", format="text"), pr={"number": 412})
job.id                      # persist it against your upload
job.status()                # ExecutionStatus
result = job.wait(timeout=900)
wyrd.VerificationExecution(client, job_id).wait()   # resume in another process
```

#### Input values: the slot type decides the projection

The client has the bound Verifier's spec in the hydrated graph, so it knows
each slot's type and projects the caller's value locally. No wrapper is needed
for JSON or samples.

| Slot type | Caller passes | Client projection (existing code) | Wire | Upload |
|---|---|---|---|---|
| `json` | dict / object / `Serialize` | `to_value` | `Json` | No |
| `samples` | flat feature row(s), `submit` only (see below) | today's drift observation path | observation | No |
| `media` | `wyrd.File(path)`, `wyrd.File(bytes, format=...)`, `bytes`, `pathlib.Path` | hash + size + `MediaFormat::sniff` | `File` | Yes, unless already stored by digest |
| `media` | `wyrd.ArtifactRef(...)` | none | `Artifact` | No |
| `trace` | trace id | none | `Json` | No |

Slot names are not SDK parameters. `deck=` is the name the Verifier author
declared under `spec.inputs`; the SDK signature is generic and validates names
against the hydrated Verifier at call time, before any network call:

```python
def verify(self, verifier: str, value: object = MISSING, /, **inputs: object) -> VerificationResult: ...
# value: shorthand for a Verifier with exactly one required slot
# **inputs: one entry per declared slot name; unknown or missing names are refused locally
#   (WYRD_SDK_400_UNKNOWN_SLOT: "unknown slot 'dek'; declared: deck, rendered")
```

`verifier` and `value` are positional-only, so a slot named `verifier` or
`value` still lands in `**inputs`. Rust takes `VerifyInputs` (a map of slot
name to `InputValue`); TypeScript takes `Record<string, InputValue>`.
`format=` on `wyrd.File` is optional and only needed when the bytes cannot be
sniffed. Per-Verifier typed input stubs are deferred.

A plain `str` is never treated as a path. Bytes and file paths travel the same
upload path: `Vec<u8>`/`Bytes` already implement the upload source trait used
by `WyrdStorageClient`. With no `format=`, the client sniffs magic bytes and
refuses locally when it cannot (`WYRD_SDK_400_MEDIA_FORMAT_UNKNOWN`, remediation:
pass `format=`). The server sniffs again on completion. Small images are not
inlined as base64: one path keeps every file digest-pinned.

```python
# json: one request, no upload, no Artifact
result = run.observe.verify("refund-policy", {"amount": 120, "reason": "damaged"})

# bytes: same upload path as a file path
result = run.observe.verify("deck-spec", deck=wyrd.File(pdf_bytes, format="pdf"))
result = run.observe.verify("brand-check", image=png_bytes)   # sniffed as png
```

```rust
// crates/shared/wyrd-client/src/observe/inputs.rs
/// One caller value before projection; the slot type decides the wire form.
pub enum InputValue {
    Json(serde_json::Value),
    File(LocalFile),          // Path or Bytes, optional MediaFormat
    Artifact(CardRef),
}

impl InputValue {
    /// Project against the slot; IO only for hashing a file.
    async fn declare(self, slot: &InputSlot) -> Result<Declared, WyrdError> {
        match (&slot.kind, self) {
            (SlotKind::Json { .. } | SlotKind::Trace, InputValue::Json(value)) =>
                Ok(Declared::inline(InputDeclaration::Json { value })),
            (SlotKind::Samples { .. }, _) => Err(drift_not_real_time(slot)), // submit routes drift rows
            (SlotKind::Media { formats, max_bytes }, InputValue::File(file)) =>
                file.declare(formats, *max_bytes).await,                        // sha256, size, sniff
            (SlotKind::Media { .. }, InputValue::Artifact(artifact)) =>
                Ok(Declared::inline(InputDeclaration::Artifact { artifact, path: None })),
            (kind, value) => Err(slot_mismatch(slot, kind, &value)),            // local, before network
        }
    }
}
```

#### One way: `verify` is real time, `submit` is background

`submit` is the only background entry point. It routes on the Verifier's
slots; the caller does not choose the path.

| Verifier has | `verify` | `submit` route |
|---|---|---|
| `json` / `media` / `trace` slots | Execute now | Create run → upload → enqueue → `VerifierRunner` picks it up |
| `samples` slot (drift checks) | Refused: drift is never real time | Rows → today's drift observation path → `insert_observation`; the bound `schedule` Trigger judges the window |

```python
result = run.observe.verify("deck-spec", deck=wyrd.File("q3.pptx"))          # real time
job    = run.observe.submit("deck-spec", deck=wyrd.File("q3.pptx"))          # queued run
run.observe.submit("credit-model-stable", current={"income": 52000, "dti": 0.31, "fico": 712, "state": "OH"})  # drift rows
```

The client code for both methods is under "Client" above.


Routing is an exhaustive match on check kinds, never a boolean, so a new
check kind does not compile until its routing is decided
(`crates/wyrd-spec/src/card/verifier.rs`):

```rust
/// Check discriminant, generated from `Check` so the two never diverge.
#[derive(strum::EnumDiscriminants)]
#[strum_discriminants(name(CheckKind), derive(Hash, Serialize, Deserialize, schemars::JsonSchema))]
#[strum_discriminants(serde(rename_all = "snake_case"))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check { Assert { .. }, Trace(..), Agent { .. }, Code(..), Drift { .. } }

/// How a check consumes evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckEvidence {
    /// Judges the inputs of one run: `verify` executes now, `submit` queues a run.
    Run,
    /// Judges a scheduled window of recorded rows: `submit` records, `verify` refuses.
    Window,
}

impl CheckKind {
    pub const fn evidence(self) -> CheckEvidence {
        match self {
            Self::Assert | Self::Trace | Self::Agent | Self::Code => CheckEvidence::Run,
            Self::Drift => CheckEvidence::Window,
        }
    }
}

impl VerifierSpec {
    /// Every check in a Verifier agrees on evidence; registration refuses a mix
    /// (`WYRD_VERIFIER_422_MIXED_CHECK_EVIDENCE`), so this is total.
    pub fn evidence(&self) -> CheckEvidence;
}
```

`verify` matches the same enum: `CheckEvidence::Window` →
`WYRD_SDK_400_DRIFT_NOT_REAL_TIME`. The server admission and binding
validation use `CheckEvidence` too (a `Window` Verifier must bind a
`schedule` Trigger).

Rules:

- A Verifier with a `samples` slot has only `samples` slots and must be bound
  with a `schedule` Trigger; registration refuses anything else.
- `verify` on a drift Verifier is refused locally:
  `WYRD_SDK_400_DRIFT_NOT_REAL_TIME` (remediation: use `submit`).
- Deleted: `observe.drift` and `observe.eval` (absorbed by `submit`), the
  `observations_ready` Trigger activation, `ObservationRunSink` /
  `ObservationRunOutbox`, the eval observation table path, and
  `DirectVerificationInput::DriftSamples` with the immediate drift branch of
  `DirectExecutor`. The scheduled drift path (window read, `vala-drift`
  scoring, fitted baselines) is unchanged.

#### Costs accepted

- With files: one evidence-plan request plus one PUT per new file, then the
  run request. Without files: one request.
- `submit`'s `202` is not a durability guarantee; outbox losses are counted
  and surface to `wait()` as a timeout.
- A run row appears after Scribe's acknowledgement. Until then the status
  lookup can return not found; a handle returned by `submit` keeps polling.
  Completed rows can also precede result visibility; `wait()` keeps polling
  for the result within its timeout.
- Evidence retention: objects under the tenant evidence prefix need a
  retention policy (time- or run-reference-based GC). Not designed here;
  flagged as an open question.


## Orchestrator

`crates/vala/vala-eval/src/executor.rs` becomes the generic orchestrator. The
stage loop, DAG planning and validation, condition evaluation, skip
propagation, concurrent fan-out, `ContextSnapshot`, and the `Executors`
injection bundle are kept.

```rust
pub struct Executors {
    pub extract: Arc<dyn ActionExecutor>,
    pub agent: Arc<dyn ActionExecutor>,
    pub code: Arc<dyn ActionExecutor>,
    pub assert: Arc<dyn CheckExecutor>,
    pub trace: Arc<dyn CheckExecutor>,
    pub agent_check: Arc<dyn CheckExecutor>,
    pub code_check: Arc<dyn CheckExecutor>,
    pub drift: Arc<dyn CheckExecutor>,
}
```

`agent` and `agent_check` are the same `AgentInvoker` called with
`AgentPosition::Step` and `AgentPosition::Check`.

```rust

#[async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn run(&self, action: &Action, inputs: ResolvedInputs, cx: &RunCx)
        -> Result<serde_json::Value, StepError>;
}

#[async_trait]
pub trait CheckExecutor: Send + Sync {
    async fn check(&self, requirement: &Requirement, scope: &serde_json::Value, cx: &RunCx)
        -> Result<CheckOutcome, StepError>;
}

pub async fn execute(
    spec: &VerifierSpec,
    plan: &ExecutionPlan,
    evidence: Evidence,
    executors: &Executors,
    cx: &RunCx,
) -> VerificationOutcome {
    let mut snapshot = Arc::new(ContextSnapshot::from_inputs(evidence));
    let mut ledger = RunLedger::default();
    for stage in &plan.stages {
        let (runnable, skips) = classify_stage(stage, &snapshot, &mut ledger);
        let ran = fan_out(runnable, &snapshot, executors, cx).await;
        // Changed: a step failure is recorded, not propagated; dependents skip.
        snapshot = Arc::new(snapshot.extend(ran.successful_outputs()));
        ledger.record_steps(ran, skips);
        // New: each succeeded step's requirements run against its step scope.
        ledger.record_requirements(
            run_step_requirements(stage, &snapshot, &ledger, executors, cx).await,
        );
        // New: step status and verdicts join the scope document, so the next
        // stage's `when` gates and action inputs can read them.
        snapshot = Arc::new(snapshot.with_step_records(ledger.step_records(stage)));
    }
    // New: verifier-level requirements against the final snapshot.
    ledger.record_requirements(
        run_requirements(&spec.requirements, snapshot.verifier_scope(), executors, cx).await,
    );
    ledger.into_outcome(&spec.decide)
}
```

Changes to the executor:

1. Runs actions, not only assertions; `TaskOutput` becomes step output.
2. Runs requirements after each step and after the graph.
3. Records step failure instead of aborting the run (today the driver
   propagates every error and aborts).
4. Applies per-step retry and timeout.
5. `ContextSnapshot` becomes the scope document: `inputs`, and per step its
   `status`, `output`, and requirement verdicts.
6. Agent and code actions materialize their `workspace:` through `Workspaces`.

The multi-turn scenario simulator in `vala-eval/src/orchestrator/` is
unaffected; it may become a `scenario` action later.

## Triggers and bindings

A Trigger still answers only *when*. `schedule` is the only activation;
`observations_ready` is deleted because `submit` queues runs directly. It
never names a Verifier. `VerificationBinding { verifier, runs_on, on_failure, inputs }`
keeps the same role.

The binding adds an `inputs` map from slot names to scheduled measurement
selectors. Slot names are not inferred. Each selector uses the existing drift
feature or metric source; there is no new source registry. For example:

```yaml
verifier: { kind: Verifier, name: credit-model-stable, version: 1.0.0 }
runs_on: { kind: Trigger, name: hourly, version: 1.0.0 }
inputs:
  current: { features: [income, dti, fico, state] }
  latency: { metric: latency_p95 }
```

The schedule supplies the window; `inputs` selects which measurements fill
each slot within that window. Registration checks that every required slot
has a compatible mapping. Direct `verify`/`submit` calls use their named input
arguments instead of this scheduled mapping.

| Activation | Can fill | Source |
|---|---|---|
| `schedule` | `samples` | Fixed cron window `[previous boundary, due)` (existing semantics, including missed-occurrence handling) |
| none (`verify` / `submit`) | `json`, `media`, `trace` | Caller supplies values |

Binding creation is refused when the activation cannot fill a required slot.
This replaces today's hard-coded activation/implementation compatibility
(`crates/wyrd/wyrd-server/src/components/cards/resolve.rs`).

Operators react to failed verdicts of background runs (`submit` and
`schedule`) on a subject whose binding names them; `verify` never dispatches,
since the caller already holds the verdict.

## Drift

Drift is a requirement check; all three current methods work unchanged.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: { space: risk, name: credit-model-stable, version: 1.0.0 }
spec:
  inputs:
    current: { type: samples, features: [income, dti, fico, state] }
    latency: { type: samples, features: [latency_p95] }
  requirements:
    - id: inputs-stable
      statement: No scoring input drifted from the training distribution
      check:
        kind: drift
        on: inputs.current
        method: psi
        signal: { distribution: { baseline_ref: { kind: Data, name: credit-train, version: 3.1.0 }, features: [income, dti, fico, state] } }
        condition: statistical
        profile: { psi: { binning_strategy: { quantile: 10 }, categorical_features: [state], threshold: { chi_square: { alpha: 0.05 } } } }
    - id: latency-in-control
      statement: p95 latency stays within control limits
      check:
        kind: drift
        on: inputs.latency
        method: spc
        signal: { metric: { name: latency_p95 } }
        condition: statistical
        profile: { spc: { sample_size: 25 } }
    - id: approval-rate-stable
      statement: Approval rate stays within 5 points of 0.42
      check:
        kind: drift
        on: inputs.current
        method: custom
        signal: { metric: { name: approval_rate } }
        condition: statistical
        profile: { custom: { metric_name: approval_rate, baseline_value: 0.42, alert_threshold: 0.05 } }
  decide: all
```

- The drift report (per-feature scores, thresholds, flags, window) is the
  requirement's `detail`; statistical detail is not reduced to a verdict.
- Fitted baselines and SPC limits stay in `vala-drift`, keyed by tenant, exact
  Verifier version, and requirement id. Content-addressed sharing is deferred.
- SPC keeps its baseline requirement (at least 20 complete baseline subgroups;
  insufficient target groups are inconclusive). PSI keeps bins in [2, 1000] and
  its minimum-sample rules. Custom keeps window-mean absolute deviation.
- To drift-check a derived value, an earlier step computes it and `on` binds
  `steps.<id>.output`.

## Examples

Worked examples live in [example.md](example.md): assertions and an LLM
judge, a PDF against a policy, a PowerPoint deck, a code-review DAG, drift,
and an upload-and-verify service.

## Guardrails

| Concern | Rule |
|---|---|
| Side effects | Steps compute and read only. External writes belong to Operators reacting to the final verdict. A worker crash restarts the graph safely; no per-step checkpoints. |
| Tenancy and authority | Effective authority is the intersection of caller authority, declared step capabilities, and deployment policy. Every slot, trace, baseline, media object, and referenced Card is resolved and authorized within the tenant. Hashes identify bytes; they never authorize reads. |
| Agent tools | Step-position Agents use only built-in tools named in their Agent Card's `tool_names` (`fs.list`, `fs.read`, `fs.search`, `code.run`, and today's read-only `cards.get`, `bifrost.query`). Check-position Agents use none. Prompt content never grants tools or retrieval. There is no network or fetch tool. |
| Workspaces | Fetched before the loop from pinned, tenant-authorized evidence into a per-call scratch directory; `/inputs` read-only, `/work` read-write with a per-call budget; deleted when the call ends. Size, entry-count, and decompression limits are enforced while fetching. |
| Untrusted evidence | Documents and diffs are evidence, never instructions. High-stakes requirements should pair an agent check with an assertion or a second independent check. |
| Citations | Citations name original input slots and can travel with derived step output. The server validates locators in pinned evidence reachable through the call's bindings; a downstream judge need not fetch it again. That proves provenance, not correctness. An agent-check pass without a valid citation is `inconclusive`. |
| Custom code | Immutable source (Artifact reference or inline), JSON in and out, no filesystem, network, credentials, or host callbacks; CPU, memory, output, and wall-clock limits set by the deployment; source hash and runtime version recorded. |
| Extraction | Hard limits on file size, decompressed size, entry count, and page count. Rust parsers only. |
| Budgets | Per-Verifier step count, total deadline, concurrency, model-call and token budgets. |
| Reproducibility | Results pin Agent and Prompt versions, code hashes, extractor version, evidence digests, drift windows, and the effective `execution_route` (deployment) per agent call. Model output is reproducible provenance, not guaranteed identical regeneration. |
| Audit | Authorization is checked before any action; decisions are staged non-blockingly on the Scribe outbox as today. Execution errors stay distinct from failed requirements. |
| Retries | Allowed for every step because steps are read-only; bounded by the step's retry policy and the Verifier budget. |

### Limits

Deployment configuration with these defaults. The approved
[specification startup settings](spec.md#deployment-limits) govern exact
environment names, 2 warm / 8 maximum workers, bounded checkout waiting, and
allocation/process/worker-group memory limits. A Verifier slot's `max_bytes`
may lower a file limit, never raise it.

| Limit | Default | Enforced |
|---|---|---|
| One uploaded file | 20 MiB | Client before upload; server when reading bytes |
| All files in one execution | 100 MiB | Admission |
| Unpacked archive | 200 MiB, 10,000 entries, 100× compression ratio | While unpacking |
| Extraction | 500 pages or slides, 10 MiB of text | While parsing |
| One agent call's scratch directory | 250 MiB | While materializing |
| `/work` writes | 50 MiB per agent call | `write_bytes_limit` set to the remaining budget per feed |
| One `code.run` / `code` action | 256 MiB memory, 10 s | Monty `ResourceLimits`; pool `request_timeout` kills the worker |
| All `code.run` in one agent call | 60 s | Server deadline |
| One tool result to the model | 64 KiB; `fs.read` pages with `offset` / `limit` | Tool layer |
| Concurrent scratch directories per server | 16 (bounds worker disk at 4 GiB) | Semaphore |
| Agent turns and time | Agent `run_config`, capped by a deployment maximum | Agent loop |
| `verify` | 60 s | Request deadline |

### Threats

| Threat | Defence |
|---|---|
| Path escape (`..`, absolute paths, symlinks) | Every file access goes through the call's `cap_std` directory handle; the OS refuses resolution outside it. `fs.*` never joins raw paths. |
| Malicious archive (zip slip, links, bombs) | Links and absolute entries refused; size, entry, and ratio limits checked while unpacking. |
| Malicious document against a parser | Rust parsers only, run off the async workers with a timeout and the extraction limits; a panic is a failed step. |
| Code taking over the server | Python runs only in Monty worker processes: Monty's own Rust interpreter, no C extensions, `ctypes`, `subprocess`, or sockets; no network, environment, or files beyond the two mounts. Crash or hang kills the worker only. |
| Files written by code | Only to `/work`; never executed or imported by the server; `fs.read` returns them as text. |
| Prompt injection in evidence | Contained by capability: a hijacked agent reads only its own directory, runs sandboxed code, and calls read-only tenant-scoped tools. No network, no write tools. Worst case is a wrong verdict, mitigated by citation validation, tool-less judges, and pairing agent checks with assertions. |
| Injection through Verifier YAML | References are parsed paths, never evaluated. Code source is immutable (Artifact or inline) and runs only in Monty. |
| Cross-tenant leakage | Evidence under a per-tenant prefix; digests never authorize. Workers hold no files; one fresh checkout per run, ended with `finish()`. Task 3 proves no interpreter state crosses checkouts. |
| Disk or CPU exhaustion | Per-call limits above and the concurrency cap. |

**Residual risk.** An escape from Monty's interpreter would run as the worker
process. Containers are rejected, so the worker is hardened at the OS level:
a separate unprivileged user, an empty environment (no database or provider
credentials), and no network. Task 3 first confirms whether
`PoolConfig::subprocess` supports this; if not, the server launches the worker
binary itself with those settings, or the risk is accepted in writing before
`code` ships.

## What is removed

- `VerifierImplementation`, `TaskVerifierSpec`, `VerifierTask`.
- `EvalSpec`, `EvalTask`, `LlmJudgeTask` as a separate concept,
  `AgentAssertionTask` (now an `assert` on an agent step's output).
- `DirectVerificationInput` and its three variants.
- `VerifierKind` (telemetry reports step action and check kinds instead).
- `Judgment.detail` variants and separate drift/eval/task result payloads.
- `DriftSpec` is kept whole as `Check::Drift`; only the `DriftCondition`
  variants validation already refuses may go.
- Free-form media type strings, replaced by `MediaFormat`.
- The Workflow Card (`WorkflowSpec`, `WorkflowRun`, routes, MCP tools, CLI
  commands, the SDK local workflow runner in Rust, Python, and TypeScript) and
  the `skald-workflow` crate, and `LlmRoute`. deployment-addressing TASK-001
  owns this removal; `execution_route` on the Agent replaces `LlmRoute`.
  Retry and timeout move to `Step`.

Workflow is one of the 15 v1 Card kinds; removing it is a doctrine change that
leaves 14. No compatibility aliases are added. Rationale:

- **Doctrine.** Wyrd verifies workflows without becoming a general-purpose
  runtime. Hosting a user's agent pipeline is runtime work and competes with
  dedicated orchestration frameworks rather than strengthening verification.
- **Verifiers cover the in-Wyrd need.** Every reason to run multi-step agent
  work inside Wyrd is to verify something, which Verifier steps now do. A
  Workflow would be a Verifier without requirements.
- **Subjects run in the user's runtime.** Pipelines under verification are
  observed through traces and registered as Services (`ServiceRuntimeKind`
  keeps its `workflow` label) and Agents. The declared-path hint on today's
  `EvalSpec.workflow` becomes `trace` check configuration.
- **Governance does not depend on Workflow.** Standalone Agent invocation from
  the SDK already goes through the Wyrd Gateway
  (`sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs`), and the
  server judge invoker routes every model call through `WyrdGatewayCaller`.
  The only gateway concept Workflow owns is the `LlmRoute` selector.
- **Skald keeps agent primitives.** Users composing agents locally write code
  against Skald `Agent` in Rust, Python, or TypeScript.
- **Unreleased.** No users to migrate; removal deletes a large surface.

Revisit only if a concrete user needs Wyrd to host a pipeline for a capability
their own runtime plus the Wyrd Gateway cannot provide, or needs Verifier
agents routed through an external gateway (today's `ExtGateway`).

## Task structure

The approved specification is decomposed into five proposed tasks in
[tasks/README.md](tasks/README.md). Contracts come first; engine, hosted checks,
and SDKs/examples/documentation then run in parallel; server integration joins
them. Full journey and integration suites run after all five changes are
merged. The task packet, not this earlier outline, owns decomposition.

### Examples and demo (required for completion)

Extend the existing `examples/support-desk` scenario in Rust, Python, and
TypeScript with the same Cards, policy file, immediate/background flows, and
result breakdown. The Python and TypeScript versions are the same example
in different languages; Rust demonstrates the same first-class workflow.
Update the docsite with all three languages and align the owning architecture
documents as part of this change, not a follow-up.

Replace the vendored SDK symlink with a real build, then add Verifier
`support/policy-check`:

- inputs: `answer` (`json`), `policy` (`media`, `formats: [pdf]`);
- step `review`: an agent with `tool_names: [fs.read]` and
  `workspace: { /inputs/policy.pdf: inputs.policy }`, returning findings;
- requirement: `assert` that `inputs.answer` does not contain
  "guaranteed refund";
- step output includes findings with citations to the original `policy`
  input; the judge receives both through explicit Prompt bindings;
- requirement: an agent judge over `steps.review.output` and
  `requirement.statement`, returning a Verdict with those source citations.

`POST /ask` calls `verify` and returns the breakdown. `POST /review` calls
`submit` and returns the `execution_id`; `GET /review/:id` calls `wait()`. The
demo shows `verify`, `submit`, an assertion, a file, an agent reading it, and
an agent judge.

## SDK surface

See "Running a Verifier" above: `observe.verify` (execute now), `observe.submit`
(Run evidence returns a queued `VerificationExecution`; Window evidence
returns `Recorded`), `VerificationExecution.wait()`, with
`wyrd.File` and `wyrd.ArtifactRef` as input values. Rust, Python, and
TypeScript share the `wyrd-client` implementation.

## Lessons carried from mitari-platform

The bohmian `mitari-platform` repository implements a requirements and
verification model. Carried over:

- **Requirement** as the noun; the statement is a field, not a separate
  "claim" type on the wire.
- **Immutable, pinned execution inputs.** Mitari freezes requirement versions
  and artifact digests into each assignment; here Verifier versions, Agent
  versions, and evidence digests are pinned in every result.
- **Absence of a finding is never a pass.** Matches `inconclusive` semantics.
- **Evidence-validated passes.** Mitari rejects a PASS without validated
  citations; here an agent-check pass without valid citations is
  `inconclusive`.
- **Per-requirement verdicts plus completeness**, rather than one opaque score.

Not carried over: two overlapping discovery pipelines, string-typed status
columns, binary artifacts in Postgres, a findings-severity-to-check mapping
hard-coded in code, and legacy names.

## Open questions

1. **Scenario simulation.** Should the multi-turn simulator in
   `vala-eval/src/orchestrator/` become a `scenario` action in this graph?
   Recommended later; not blocking.
2. **Reusing a whole Verifier as a step or check.** Strongest form of reuse
   (`check: { kind: verifier, ref: pii-check }`), but conflicts with today's
   "never a Verifier DAG" doctrine. Recommended: allow one level of nesting
   later, once the base contract ships.
3. **Requirement grouping.** Sections for policy breakdowns (`group:`) — add
   when a real report needs it.
4. **Human verdicts.** A permissioned, audited path for a person to record a
   requirement verdict (needed for underwriting). Out of scope here; the
   `Check` enum is the place it would land.
5. **Personal data retention.** Evidence for regulated work (bank statements,
   identity documents) conflicts with append-only result storage. Needs a
   design (per-run key destruction, or evidence outside Iceberg with digests
   only) before regulated customers.
6. **Extraction fidelity.** XML extraction recovers text and counts, not
   layout or charts. Visual checks need a PDF or image rendition supplied by
   the caller; server-side rendering is not proposed.

## Risks

- **Graph creep.** DAG only, skip-only conditions, per-Verifier caps on steps,
  cost, and time. Loops are not supported.
- **Large coordinated cut-over.** Integration touches contracts, SDKs, results,
  bindings, and generated surfaces at once. Nothing is released, so no
  aliases, but it must land as one change.
- **Sandbox maturity.** Custom code is the main user extension point; it must
  not ship before isolation is proven.
- **Model-judged requirements.** Multimodal judges agree with humans roughly
  70% of the time on current benchmarks and exhibit modality neglect.
  Per-requirement focus, citations, and pairing with deterministic checks are
  the mitigations; regulated decisions should not rest on an agent check
  alone.
