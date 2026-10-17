---
id: TASK-005-R2
kind: remediation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 9
requirements: [REQ-002, REQ-007, REQ-008, REQ-010, REQ-011, REQ-012, REQ-013, REQ-017, INV-001, INV-006, INV-007, AC-003, AC-005, AC-006, AC-009]
depends_on: [TASK-005-R1]
parent_task: TASK-005
remediates: [FIND-TASK-005-2, FIND-TASK-005-7, FIND-TASK-005-8, FIND-TASK-005-10, FIND-TASK-005-11, FIND-TASK-005-12, FIND-TASK-005-13]
---

# Finish capacity propagation, evidence admission and Google request projection

Route directly to `$wyrd-implement`. Implement this one bounded remediation; do not start another architecture or planning cycle. This task is self-contained through the validated diagnoses, selected boundaries, preservation constraints and proof below. Private helpers, local control flow and fixture layout remain implementation-owned.

## Authority and immutable candidate

- Approved specification: `changes/active/verifier-building-blocks/spec.md`, r9.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-005-server-integration.md`.
- Previous remediation: `changes/active/verifier-building-blocks/review/task-005-20261016-99882cf82/TASK-005-R1-verification-corrections.md`.
- Previous review: that directory's r9 `verdict.md` disposition and original `findings-validation.md`.
- Current review: this directory's `verdict.md`, `findings-validation.md`, discovery reports and focused `followup-review.md`.
- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Cumulative base exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
- Previous reviewed candidate: `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`.
- This reviewed candidate: `de69cb69b68eced72f7d760681d27253e013f8a1`.
- Read actual committed range with `git diff f2a726c90...de69cb69b`; the recent `99882cf82...de69cb69b` diff locates remediation. No archived source/diff is needed.

The owner was actively editing during review. Those edits and all commits after the candidate were excluded. Before implementation, reconcile these diagnoses with the current source without reverting the owner's work. If a newer change already fixes an item, record its source and focused proof rather than reimplementing it. A subsequent review still uses a new immutable cumulative candidate.

Governing rules are AGENTS.md, architecture/agent-rules.md, current Wyrd/Bifrost/security/doctrine authority, and applicable references through architecture/references/README.md. Nothing has shipped: no upgrade migrations, old catalog/history conversion or legacy pending-dispatch compatibility is required. R9 already approves the local evidence PUT; do not reopen or replace that boundary. Agent-runtime-composition r2 is draft and does not supply implementation requirements.

## Outcome and existing owners

Retain the corrected signed admission, exact-grant File transport, optional scheduled-slot behavior, selected per-payload result identity, queue lifecycle, and caller-linked trace export. Complete the seven validated gaps at their existing owners:

| Outcome | Existing owner to extend |
|---|---|
| First hosted capacity refusal ends its Agent invocation before outer timeout | Vala `ToolSession` / `AgentExecutor`, existing engine Busy and runner deferred/refunded release |
| Present input declarations obey their selected Verifier | Existing Vala verification execution boundary and installed jsonschema dependency; hosted evidence owner for resolved media metadata |
| Combined original input files fit the configured execution limit | Server `TenantEvidence` / hosted composition and existing authorized Artifact inventory/read paths |
| Effective Google output override emits a valid single schema field | Shared `skald-prompt` Google builder used by Gemini and Vertex |
| Signed Google function-call continuation survives typed hosted loop | Shared Google part representation and existing typed assistant/history/rebuild path |
| Required item docs and current capacity docs agree with source | The concrete changed Rust items and two existing runtime docsite pages listed below |

## Validated diagnoses and selected corrections


All locations below are relative to the frozen candidate checkout. Findings 1–9 retain their original IDs; 10–13 are independently confirmed new causes.

### FIND-TASK-005-2 — Capacity refusal can be lost behind pending Agent tool siblings

**Status:** CONFIRMED residual. **Sources:** CONC-R2-001, followup-review. **Classification:** INCORRECT. **Obligation:** approved Deployment limits, TASK-005 Scenario 6, REQ-013; `wyrd-design.md:1337–1347` requires refused work to return to the queue without consuming an attempt.

**Location/evidence:** `vala-eval/src/blocks/tools.rs:83–103,150–169,258–260` and `blocks/agent.rs:277–345`. Sandbox admission now correctly produces Busy (`hosted/sandbox.rs:164–179`); HostedTools preserves its typed cause (`hosted/tools.rs:283–309`); RecordedTool latches it. But Refusal has only an Arc<OnceLock<String>>, with readers before the next model request and after `run_prompt` completes. Skald's normal tool batch uses `buffer_unordered(...).collect().await` (`skald-agent/src/loop_runtime.rs:661–665`); ordinary tool errors become model-visible result values, not short-circuit errors. A supported Agent with code.run plus bifrost.query can therefore raise Busy while a sibling query awaits real IO under its independent query deadline (`hosted/tools.rs:234–247`). The outer step timeout (`blocks/engine.rs:339–385`) drops AgentExecutor before either observer, converting the already-observed refusal to Retryable("timed out"), authored retry/preparation failure, or execution-wide DeadlineExceeded. Existing engine Busy, runner Deferred and fenced release/refund are correct but never receive this refusal.

**Consequence:** valid queued work can spend authored retries or terminate/inconclusively complete due to already-refused process capacity, instead of retaining its identity for actual evaluation after capacity frees. Direct execution can expose timeout/preparation outcome instead of its current Busy/503.

**Decision-complete minimal correction:** extend the existing invocation-owned ToolSession refusal to wake the AgentExecutor while it awaits the loop. Retain first-reason state and use installed Tokio synchronization with a race-safe wait. An observed refusal before the action deadline ends that invocation Busy and drops its owned pending loop/tool futures. Keep the post-completion race safe. Do not change generic Skald ordinary tool failures, serialize all tools, reserve workers across model waits, alter shared query lifecycle, patch terminal verdicts afterward, add a queue, or override earlier caller cancellation/expired deadlines. This consumer observation is appropriate because this session owns the typed host-refusal exception to normal Agent tool-result semantics.

**Focused closure proof:** use the existing Agent/tool fixture through VerificationEngine's externally timed action, with one Busy-producing code.run and a controlled pending sibling, no earlier Agent run timeout. Establish refusal before the step deadline; assert EngineError::Busy before that deadline, no authored retry/next model call, and pending sibling future dropped. Preserve non-Busy error and cancellation/deadline behavior. Retain existing real-pool producer proof and queued deferral/refund proof; a narrow combined queued case can additionally assert eventual completion under the same ID. No host load or broad journeys.

### FIND-TASK-005-7 — Mandatory changed-item rustdoc remains incomplete

**Status:** REVISED residual. **Sources:** MNT-R2-001, REP-R2-001, evidence prose note. **Classification:** VIOLATION. **Obligation:** AGENTS §16 / agent-rules mandatory item docs, errors and panic contracts; task-review's explicit blocking rustdoc exception.

**Locations/evidence:** materially changed `skald-prompt/src/prompt.rs:1269::provider_request_from_py` and `:1661::json_parts` have no item docs or Errors section despite changed authored decode classification. Their callers are Python set_request and native content authoring; bodies respectively accept a native PyRef or decode Python JSON, and decode a part or part list. `wyrd-telemetry/src/context.rs:412` and `:452` tests describe intent but omit remaining fixture/assertion panics. `vala-eval/src/scenario.rs:447::record` parses supplied trace_hex with expect; `vala-eval/src/executor.rs:804::full_spec` constructs fixture paths/IDs with fallible expect-backed helpers; `vala-eval/tests/integration/orchestrator_parity.rs:135::scripted_walk_parity` asserts fixture and scoring invariants. Each lacks the required panic contract. Full bodies and cumulative material changes support these items; merely building cargo doc is insufficient.

Related false prose occurs in `wyrd-server/src/verification/results.rs:143–145::ResultRun::execution_id`, `verification/runner.rs:774–775::result` and `components/verification/service.rs:628::stage_direct`: each still claims result/execution identity reuse. The builder at results.rs:230–242 mints one result ID per payload. `vala-bifrost-redux/src/tables/verification/results.rs:1,15–20` also says exactly one row per completed execution and that unfinished executions have no row, contradicted by the approved stage-before-commit recovery path. This is documentation debt in the same bounded batch, not a remaining wrong-result implementation.

**Consequence:** the explicit hard rustdoc gate remains unmet; changed conversion and fixture contracts are undiscoverable, and identity/recovery prose describes the invariant removed by FIND-5's fix.

**Minimal correction/proof:** document the listed changed items in place with actual role, conversion errors and remaining panic conditions; align result prose with per-produced-payload identity, possible abandoned rows and authoritative queue selection. Preserve bodies, assertions, SQL and lifecycle. Reuse existing owning docs/format/lint checks plus manual item/body comparison. No behavior test, checker, harness, unrelated helper rewrite or style-only task is required. Original R1 named omissions are repaired; do not redo them.

### FIND-TASK-005-8 — Runtime reference still teaches the retired capacity outcome

**Status:** CONFIRMED residual. **Source:** REP-R2-002. **Classification:** INCORRECT. **Obligation:** REQ-017, baseline authority-update clause, published Deployment limits/runtime behavior.

**Location/evidence:** `docs/src/content/docs/reference/verification-runtime-limits.md:68` says admission/no-worker refusal spends a code step retry, makes a check inconclusive, or reaches the Agent. Its Errors table at :96 catches other limits as step failure/check inconclusive. `docs/src/content/docs/operate/configure-verification-runtime.md:39` calls excess admission retryable. Actual Sandbox/Workspaces Busy, common engine Busy, direct 503 and runner defer/refunded release contradict this; FIND-2 concerns timely propagation, not approval to document capacity as judgment.

**Consequence:** operators are told to tune authored retries for contention whose approved outcome is execution-level refusal/deferral.

**Minimal correction/proof:** update these two current pages to distinguish process-capacity Busy (background remains queued with attempt refund; direct ServiceUnavailable) from genuine code/runtime failures and exhausted authored Python budgets. Preserve earliest deadlines, bounded waiters, cancellation and actual Python error behavior. Do not add configuration or restore old retry semantics. Compare prose to existing owners and run `mise run docs:check` because these are docsite edits; no new behavior tests. Original architecture schema/label/activation inventory is closed.

### FIND-TASK-005-10 — Present evidence bypasses authoritative Verifier slot validation

**Status:** CONFIRMED new. **Source:** INVREV-01 (declaration conformity). **Classification:** MISSING / INCORRECT. **Obligation:** REQ-002 explicitly requires server-authoritative refusal of unknown slots, wrong kinds and schema/slot-size violations; INV-001/006, AC-006, original Scenarios 1/2/5.

**Location/evidence:** `wyrd-server/src/components/verification/service.rs:445–496,554–618`; `verification/hosted/blocks.rs:105–146`; `vala-eval/src/blocks/engine.rs:149–177`, `scope.rs:198–231`. Typed HTTP/MCP decoding validates the envelope, not input names/kinds against the selected spec. Direct resolve loads the exact spec but does not validate inputs. Enqueue stages declarations; queue acceptance checks targets/readiness/hash, and runner dispatch restores the same declarations. HostedBlocks composes readers/executors without checking slots. Engine run plans the graph and passes supplied evidence unchanged; Scope directly returns present Json or whole evidence declarations. Thus a valid no-step Verifier with integer-schema x and an is_not_null check on inputs.x accepts string JSON and can pass. Extra input names are echoed into outcome evidence. Wrong declaration kinds and File format/max_bytes differing from their media slot similarly bypass authoritative refusal.

The SDK's `wyrd-client/src/verify/mod.rs:314–393::Target::project` rejects names/kinds locally; `RunEvidence::pin:479–516` checks File format/slot max_bytes. Neither evaluates the Json slot schema; Artifact paths do not gain File pin checks. HTTP/MCP bypass all these SDK checks. Lower-level file digest/sniffing validates storage integrity, not declared slot conformity.

**Consequence:** a subject can receive a passing result from evidence the selected Verifier declares invalid; primary headless callers receive inconsistent validation across surfaces.

**Decision-complete minimal correction:** validate the supplied declaration set once at the existing execution boundary before graph actions/checks. Reuse the exact loaded spec, closed InputSlot/InputDeclaration types, existing structured VerificationInputInvalid/TooLarge errors and Vala's installed jsonschema facility (already used by Agent output validation). Keep schema evaluation in the execution-owning tier; do not move jsonschema to foundational wyrd-spec or add parallel per-SDK validators solely for sharing. Direct and queued paths must both refuse present-invalid evidence; queued validation after exact spec load may use the existing terminal errored execution surface and must not spend authored/provider retries or produce a subject verdict. Validate media slot format/max_bytes for File declarations and for the authorized selected Artifact file at the server evidence boundary where its actual metadata becomes known. Preserve existing Artifact authorization, per-file digest/length/sniffing and structured public errors. No request-path PG write, new endpoint or new queue.

Missing required slots remain missing evidence/inconclusive; optional omission remains null. Do not replace those semantics with required-field rejection or infer omitted scheduled slots. Scheduled Window evidence keeps its explicit mapping/report path.

**Focused closure proof:** existing raw typed HTTP/MCP/runner fixtures should exercise direct and queued unknown name, wrong kind, JSON schema failure, File/media format and slot max_bytes, plus authorized Artifact selected-file equivalents. Assert refusal before block/model work, no passed result/Operator, existing structured error/terminal error. Keep missing-required inconclusive and optional omission proofs, and a valid control. Exact new regression selections plus narrow owning modules suffice; SDK JSON schema submission should be covered through the shared server path, not a duplicated language implementation.

### FIND-TASK-005-11 — Combined input-file deployment limit has no execution owner

**Status:** CONFIRMED new, separated from FIND-10. **Source:** INVREV-01 (resource accounting). **Classification:** MISSING / VIOLATION. **Obligation:** Deployment `WYRD_VERIFICATION_FILES_MAX_MIB`, INV-007, AC-006 and Scenario 6.

**Location/evidence:** `wyrd-server/src/components/verification/service.rs:325–345` sums only one evidence-plan request; individual local PUTs require no plan/session. `verification/hosted/evidence.rs:121–269` bounds each File/selected Artifact read separately. `hosted/workspace.rs:68–102` sums only that workspace's materialized inputs. HostedBlocks creates one TenantEvidence per execution but that owner retains no aggregate byte validation/accounting. Separately plan/upload two individually legal Files, then reference both in distinct extract actions: neither action needs a workspace and both read under their individual limits. Selected Artifact files bypass evidence upload plans. Several workspaces also have separate totals. Therefore individually valid files can exceed the declared per-execution bound and still produce a result.

**Consequence:** the approved deployment bound can be bypassed through supported extraction/native-media/Artifact consumers, including SDK Artifact evidence and raw HTTP/MCP Files. An upload-plan total does not prove execution admission.

**Decision-complete minimal correction:** enforce the existing combined evidence budget at the execution-owned hosted evidence boundary across the original pinned input set, using declared File lengths and authorized selected Artifact metadata, before that set can drive block work. Reuse TenantEvidence's existing authorized Artifact/inventory and stored-byte owner; do not introduce a second Card loader or workspace-only guard. Validate known File totals without fetching their bodies; resolve selected Artifact metadata through the existing audited owner, retaining later bounded read/digest/sniffing. Use overflow-safe totals. Account for original inputs once; internal repeated reads/citations, retries and multiple workspaces must not reset or repeatedly charge the same original evidence. Keep per-file and per-workspace restrictions. This is enforcement of an existing budget, not a new memory guarantee for extraction/model/derived output buffers. No persistent reservation, upload session, prefetch of all bytes, queue change or new global admission policy is needed. Direct returns its current size error; queued uses existing terminal execution-error handling before model/actions, not a subject failure or retried provider work.

**Focused closure proof:** lower hosted limits in the existing fixture; separately upload two distinct small legal files whose combined size is too large and execute/submit a two-extract graph. Refuse before blocks, with no passing result or Operator, while a below-budget control succeeds. Cover selected Artifact files and native-media/multiple-workspace consumers through the same execution owner; repeated reads of a valid input must not produce false exhaustion. Reuse existing fixture/runtime tests and exact new selections; no host stress, cloud credentials or broad gate.

### FIND-TASK-005-12 — Google builder keeps both mutually exclusive output-schema fields

**Status:** CONFIRMED new. **Source:** PROV-01. **Classification:** REGRESSION. **Obligation:** REQ-007 Prompt-owned output contract and preservation of existing supported provider-shaped authoring.

**Location/evidence:** `skald-prompt/src/builder.rs:397–420::google_settings`, public `gemini:235` and `vertex:254` through google_prompt:270. The complete helper clones native settings, takes generation_config, and sets new response_json_schema when effective output/ResponseFormat supplies a schema, without clearing response_schema. Previously this branch overwrote response_schema. Both fields serialize; shared response schema extraction simply prefers the new one, so local compilation accepts it. Both Python public builders use these same owners. Portable authoring and gateway translation start from their own fresh configs and are not separate defects requiring patches.

Google's [GenerateContent GenerationConfig reference](https://ai.google.dev/api/generate-content#v1beta.GenerationConfig) states the alternative JSON-schema field excludes responseSchema. Primary protocol source was checked; no live request was sent. This establishes invalid native request construction, not a speculative model capability request.

**Consequence:** supplying existing native response_schema settings plus a replacing effective schema creates a request the provider refuses, instead of preserving the existing builder's override behavior.

**Minimal correction/proof:** clear response_schema in this existing JsonSchema override branch before assigning response_json_schema; preserve the effective-schema precedence, all unrelated native config and native-only settings without an override. No generic validator/provider adapter or dependency. IO-free tests for both Gemini and Vertex build settings with old schema plus effective output/ResponseFormat; serialized request has only the effective new schema and preserves another setting. Run exact builder regression selections and the narrow Prompt check lane.

### FIND-TASK-005-13 — Hosted Google tool continuation loses the required signed part

**Status:** REVISED new. **Source:** PROV-02. **Classification:** MISSING. **Obligation:** REQ-007/008, AC-003: the newly integrated hosted Agent action must use its selected Prompt and built-in loop through completion.

**Location/evidence:** task-owned `wyrd-server/src/verification/hosted/blocks.rs:119–136` composes AgentExecutor, built-ins and ServerWyrdGatewayCaller. That caller's buffered native answer decode (`components/gateway/caller.rs:97–145,175–184`) becomes typed ProviderResponse. `skald-spec/src/wire/google_generate.rs:121–125::GooglePart::FunctionCall` retains only the function_call; its nested GoogleFunctionCall at :170–178 holds only name/args. Neither stores the provider part's thoughtSignature/thought_signature. Permissive untagged decode discards that field. `skald-agent/src/request_builder.rs:166–177::assistant_message` copies the already-decoded candidate, and `:236–245::rebuild_request_messages` replays it. The loop's provider adapter extracts tools, executes them and appends results using this same native history; no raw response recovery exists.

The wire limitation predates this task, but the hosted built-in-tool consumer is newly composed here and required by the approved task. Google/Vertex are supported conversation dialects; this is not Jev/SystemOne's existing unsupported-loop refusal, a native-only zero-caller feature, or draft SDK WyrdClient/MCP composition. Raw native SDK gateway relay retains bytes and does not prove this typed consumer.

Google's primary [thought-signature protocol](https://ai.google.dev/gemini-api/docs/generate-content/thought-signatures#signatures-in-function-calling-parts) requires replaying a function-call signature on its original part; omission on the first required function call yields 400 for affected models. This supports the consequence for signed tool responses without requiring a live call or universal claims across every Google model.

**Consequence:** a supported hosted Agent tool can execute successfully but the next model request fails because Wyrd discarded mandatory continuation metadata, preventing the action from returning its result.

**Decision-complete minimal correction:** preserve the optional opaque function-call-part signature in the existing shared Google part wire representation, accepting provider camelCase and current snake_case and serializing it in the provider-supported form. Reuse current native assistant/rebuild history and preserve part order. Do not synthesize signatures, disable validation, create raw-history storage, add another caller/loop, require optional non-function signature support, or implement draft composition. Update existing pattern matches/constructors and projections necessarily affected by the wire representation; preserve ordinary unsigned responses and other providers. This completes native protocol projection, not a new authored product capability.

**Focused closure proof:** use an existing mock upstream/tool-loop fixture containing a signed native function-call part, typed buffered decode and one tool execution; assert the second upstream request contains the original signature on that part, original order and the matching tool result. Cover both decoded spellings and unsigned control. The test must traverse the typed history used by the hosted caller; a raw gateway relay-only test or isolated serializer alone is insufficient. No credentials/live sweep or broader draft/native feature work.


## Consolidated acceptance and preserved behavior

| Finding | Required observable closure |
|---|---|
| FIND-TASK-005-2 | With a Busy code tool and a pending parallel sibling, the externally timed action returns Busy promptly before its deadline, cancels owned pending futures and neither retries nor judges contention. Existing queued release refunds the attempt and can later finish the same identity. |
| FIND-TASK-005-7 | The enumerated changed Rust items have substantive intent/errors/panic contracts; result docs distinguish execution correlation, payload identity, abandoned rows and selected queue pointer. Bodies and assertions remain intact. |
| FIND-TASK-005-8 | Published capacity behavior distinguishes queued deferral/refund and direct 503 from genuine Python/provider/runtime failures and existing deadlines/budgets. |
| FIND-TASK-005-10 | Direct and queued raw HTTP/MCP evidence with unknown names, wrong kinds, violated JSON schemas, or media slot incompatibility is refused with existing structured errors before block/model work; valid input works, required absence stays inconclusive and optional omission stays null. |
| FIND-TASK-005-11 | Separately uploaded valid files and authorized selected Artifact files cannot exceed the execution's combined original-file bound through extraction, native media or multiple workspaces. Repeated internal reads/retries do not double-charge or reset accounting. |
| FIND-TASK-005-12 | Gemini and Vertex effective schema overrides serialize only the replacing response_json_schema while retaining unrelated/native-only settings. |
| FIND-TASK-005-13 | A signed function-call response crosses typed hosted decoding, tool execution and assistant history; the next upstream request retains the original opaque signature on the original part, plus the matching tool result. Both accepted spellings and unsigned responses work. |

No new public field/endpoint/error catalog, client-owned durable validation, duplicate loader/engine/transport, queue/store/receipt, new dependency, global cancellation service, platform-specific mechanism, or persistent reservation is authorized. Do not weaken digest/length/sniffing, Artifact permissions, exact invocation, tenant/signed scope, nonblocking audit, existing per-file/workspace limits, leases, shutdown, result-loss semantics, ordinary tool errors or other providers. No deployment migration, benchmark redesign, Jev/SystemOne dispatch, optional non-function signature work, or draft SDK/MCP runtime composition belongs here. The rejected absent-target conflict proposal is not remediation scope.

## Focused proof and execution discipline

Use one behavior scenario at a time: confirm expected RED on the relevant current source, implement the smallest owning correction, rerun GREEN and the immediately affected regressions, then refactor only if the passing behavior calls for it. Documentation-only edits use static item/body comparison; no new tests or checkers to imitate TDD. Never weaken an assertion, timeout, skip or gate to pass. Do not generate host load; a tiny configured pool and controlled pending tool are sufficient.

The per-finding focused closure scenarios above are normative. Add/select tests in existing owning modules/fixtures, including the real typed hosted provider history and raw server evidence paths. Record and run the exact `mise exec -- cargo nextest run --locked -p <owning-package> --lib|--test <target> -E 'test(=<actual-name>)'` command for every specifically named new/existing test in the implementation report, with minimal needed features and the managed Postgres wrapper when required. Confirm names/targets with source and nextest list; never use positional filters that can select zero tests. Newly chosen test names and fixture internals remain implementation-owned.

After exact new regressions pass, run only the following affected slices, consolidating overlaps once. These commands identify existing owning modules rather than invent new test names:

```bash
# Existing Vala engine/tool/Agent seam, including input validation and Busy.
mise exec -- cargo nextest run --locked -p vala-eval --lib -E 'test(/^blocks::/)'

# Existing Google builder regression neighborhood; both public builders share it.
mise exec -- cargo nextest run --locked -p wyrd-skald-prompt --lib -E 'test(/^builder::builder_tests::/)'

# Real server input admission plus hosted consumers. Setup is a test prerequisite,
# not an upgrade migration or a new deployment behavior.
mise run build:monty-worker
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration --test-threads=1 --run-ignored=all -E 'test(/^pg_verification_routes::/) or test(/^pg_hosted_artifacts::/)'"
```

For typed Google continuation, run the exact new existing-fixture upstream/Agent regression and its minimal neighboring cases, using package names `wyrd-skald-spec`, `wyrd-skald-agent`, or `wyrd-server` for the actual chosen owning target. Do not substitute a raw gateway relay serializer test or a live smoke sweep. For File budgets and queue effects, add exact focused runtime selections only if the new tests live outside the route/hosted slice; do not run whole runtime/SQL/family/journey suites merely because those packages depend on the changed code.

Required hygiene for the resulting write set:

```bash
mise run fmt
mise run lints
mise run check:deps
mise run docs:check
```

Run `mise run check:tenant-isolation` if the minimal correction actually changes SQL/query/tenancy owners; no SQL change is required by the selected fixes. Run `mise run codegen:check` when derived provider wire/schema/contracts or generated surfaces are affected, regenerating from source rather than editing generated files. The signature fix must update all necessarily affected constructors/matches/projections; do not add a new public language API merely to carry native protocol state. If Python files/stubs/exports or Python-visible behavior change, use existing public-import Python tests and required `py:format`, `py:lints`, `py:typecheck`/narrow unit lane for that write set. Keep Python/Node lifetimes in their own runtimes. Item-level rustdoc/panic completeness still requires manual source review; a green build does not prove it.

Do not require a full gate, full three-language journey suites, family aggregates, live provider/cloud runs, capacity qualification or external example rebuilds for this bounded remediation. Final change review owns integrated broad proof. No credential access is needed for any prescribed closure.

## Completion evidence

For each retained stable ID, record the changed existing owner, invalid-state producer and all affected consumers, exact RED/GREEN commands and observed outcomes (or documentation/deletion static proof), preserved adjacent behavior and remaining limits. Retain all seven stable IDs. FIND-1/3/4/5/6/9 are already closed in this review; do not reopen their approved behaviors or rebuild their machinery. Review must then reassess the complete original base-to-new-candidate range, this task, R1, both prior ledgers/verdicts and all preserved evidence. No implementation agent may mark TASK-005 complete without that independent review.

## Implementation evidence

Postgres-backed commands ran under `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && …"` after `mise run build:monty-worker`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-005-2 | `vala-eval/src/blocks/tools.rs` `Refusal`: the first reason goes on a Tokio `watch` channel, and a race-safe `refusal_raised`. `blocks/agent.rs::invoke` `select!`s the loop against the refusal and drops the pending loop and tool futures as Busy. Ordinary tool errors, deadlines and cancellation are unchanged (3aac923b0). | RED: `DeadlineExceeded`. GREEN: `mise exec -- cargo nextest run --locked -p vala-eval --test integration -E 'test(=blocks_agent::busy_tool_ends_the_agent_step_before_a_pending_sibling)'`. Slices: `-p vala-eval --lib -E 'test(/^blocks::/)'` (23 passed) and `--test integration -E 'test(/^blocks_agent::/)'` (10 passed). | PASS |
| FIND-TASK-005-7 | Rustdoc on `provider_request_from_py`, `json_parts`, the two `wyrd-telemetry` context tests, `scenario::record`, `executor::full_spec` and `scripted_walk_parity`. Result prose in `results.rs::ResultRun::execution_id`, `runner.rs::result`, `service.rs::stage_direct` and the `vala.verification.results` table docs now describes per-payload identity, possible abandoned rows and the selected queue result. Bodies and assertions are unchanged (66cfab60c). | Static item/body diff. `mise run lints` (all features) and `mise run fmt` pass. `mise exec -- cargo nextest run --locked -p vala-eval --test integration -E 'test(=orchestrator_parity::scripted_walk_parity)'` passes. | PASS |
| FIND-TASK-005-8 | `reference/verification-runtime-limits.md`: worker, admission and workspace refusal is busy. It keeps the queued ID with the attempt refunded and returns direct 503, with no retries spent. Errors table row added; a spent Agent budget stays a step failure. `operate/configure-verification-runtime.md` waiter overflow follows the same wording (46170e9a8). | `mise run docs:check` passes. | PASS |
| FIND-TASK-005-10 | `blocks/engine.rs::conform` runs before planning, so it applies to both direct and queued runs. It refuses unknown slots, wrong kinds and Samples evidence, and checks JSON schema, file format and slot `max_bytes`. New `EngineError::InputInvalid`/`InputTooLarge`/`EvidenceUnavailable` map to the existing 400/413/503 (`service.rs::execute`) and the queued `input_invalid` terminal error (`runner.rs::engine_failure`). A required missing slot stays inconclusive (3a33ba8d1). | `-p vala-eval --lib -E 'test(=blocks::engine::tests::nonconforming_evidence_is_refused_before_any_block)'`; `-p wyrd-server --features test-support --test integration -E 'test(=pg_verification_routes::direct_execution_refuses_nonconforming_evidence)'`; `-E 'test(=pg_hosted_artifacts::nonconforming_queued_evidence_errors_without_a_verdict)'` (47687a51f). | PASS |
| FIND-TASK-005-11 | `EvidenceReader::admit` (default no-op) runs inside the execution deadline before `drive`. `TenantEvidence::admit` reads Artifact metadata through `artifact_file` (no bytes) and checks format and slot size. It sums each File and Artifact declaration once with `checked_add` against `files_max_bytes`, so repeated reads and retries are never charged twice. | `-E 'test(=pg_hosted_artifacts::artifact_evidence_is_admitted_against_its_slot_and_the_combined_limit)'`. It covers format, slot size, the same Artifact pinned in two slots, Artifact plus File over the combined limit, and a valid control read by two steps. | PASS |
| FIND-TASK-005-12 | `skald-prompt/src/builder.rs::google_settings` clears native `response_schema` when it sets `response_json_schema` (83f4a9ec0). | `mise exec -- cargo nextest run --locked -p wyrd-skald-prompt --lib -E 'test(=builder::builder_tests::google_output_schema_replaces_native_response_schema)'`, plus the `builder::builder_tests::` slice (10 passed). | PASS |
| FIND-TASK-005-13 | `GooglePart::FunctionCall.thought_signature` accepts both spellings, is skipped when absent, and is carried through every constructor and match (`skald-spec`, `skald-tool`, `wyrd-gateway`, `skald-prompt` python) (acd70a13a). | RED via temporary `serde(skip)`. GREEN: `mise exec -- cargo nextest run --locked -p wyrd-skald-agent --test integration -E 'test(=loop_gemini::signed_function_call_replays_its_signature_on_continuation)'`, covering camelCase, snake_case and an unsigned control. `wyrd-skald-spec`/`wyrd-skald-tool` lib (130), `loop_gemini::`+`agent_journey` (8) and `wyrd-gateway` (62) pass. `mise run codegen:check` passes. | PASS |

Slices: `pg_verification_routes::` + `pg_hosted_artifacts::` 18/18 passed. Hygiene: `mise run fmt`, `mise run lints`, `mise run check:deps`, `mise run docs:check`, `mise run codegen:check` and `git diff --check` all pass. `check:tenant-isolation` was not run: no SQL changed.

Non-goals stayed excluded. There is no new public field, endpoint, error code, dependency, queue, store, reservation or platform-specific mechanism. Generic Skald tool errors, other providers, the SQL lifecycle and the closed findings (1/3/4/5/6/9) are untouched.

Limits:
- MCP evidence reaches the same `VerificationService::execute`/engine path. It is proven through HTTP and the hosted engine, not by a separate MCP test.
- The queued Busy deferral and refund is proven by the existing runner tests and was not re-proven in combination with FIND-2.
