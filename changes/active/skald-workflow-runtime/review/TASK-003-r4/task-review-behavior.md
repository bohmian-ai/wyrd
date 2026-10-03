# TASK-003 r4 behavior review

## Review findings

### Critical

None.

### Important

- **BEH-R4-001 (reopens `FIND-TASK-003-10`) —
  `crates/shared/wyrd-client/src/workflow/mod.rs:195-208`:** A client-less
  Workflow that selects both `ext_gateway` and `wyrd_gateway` reads
  `GlobalConfig` twice at one run start. The first read supplies Workflow
  bindings, while `WyrdClient::from_global` performs a second independent read
  for the public-gateway endpoint and client settings. This contradicts R3's
  selected correction to load one ambient snapshot and can compose one run
  from different configuration revisions if the file changes between reads.
  Reuse one loaded `GlobalConfig` through the existing
  `ClientConfig::from_global_with_env` and `WyrdClient::with_config` owners to
  derive both outputs, preserving retained clients and selected-route laziness.
  Static source closure plus the existing selected-local-dependencies and
  language-context regressions is sufficient; do not add a watcher, cache,
  timing fixture, new harness, setting, option, or check.

### Suggestions

None. This acceptance audit does not prescribe optional work.

## Subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation tasks:
  `review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`,
  `review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`,
  and
  `review/TASK-003-r3/TASK-003-R3-close-pending-renewal-and-async-context-contracts.md`

The human-approved 2026-10-03 addition of `pub model: ModelRef` to
`WyrdGatewayCall`, the R1 same-spec correction that native `401` handling
renews without replaying the model POST, and the approval of `spawn_blocking`
for run-start configuration/client/secret reads are authoritative. The
run-start reads remain at run start because Workflow loading must not read
execution secrets.

I reviewed the complete base-to-candidate range and traced the remote Workflow
handle, Skald route projection, public native caller, authenticated transport,
fallback ingress, local dependency composition, language context retention,
and the R1-R3 corrections through their callers and focused evidence. The
candidate stayed at the stated commit during this review. CodeGraph was
unavailable because the repository has no `.codegraph/` directory.

This was a strict source-only review. I did not build, compile, run tests, run
Cargo, Mise, pnpm, pytest, or execute any verification lane or command.
Verification entries below are the implementer's recorded results, assessed
against the source they purport to exercise.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Recorded verification evidence | Result |
|---|---|---|---|
| REQ-024/031/045/046 and AC-019: one shared Rust handle exposes create/get/cancel/wait, direct snapshots, one idempotency key per create, fixed one-second polling, terminal failures as values, and drop-without-cancel behavior | `crates/shared/wyrd-client/src/workflow/remote.rs:23-121`; exports in `crates/shared/wyrd-client/src/lib.rs` and `sdks/wyrd-sdk-rust/src/lib.rs` | Original task records `shared_workflow_client_contract`, shared-client, and Rust SDK proof; source remains consistent | PASS |
| REQ-036A/038/039/043 and approved Revision 12 model field: each local WyrdGateway call carries Prompt-derived model, fallback, remaining timeout, cancellation, and trace-only correlation as immutable call state; public protocols dispatch and Vertex refuses before IO | `crates/skald/skald-workflow/src/plan.rs`, `route.rs`, and `workflow.rs`; `crates/shared/wyrd-client/src/workflow/gateway.rs:50-286` | Original focused caller/Skald evidence records protocol projection, model carriage, timeout, cancellation, Vertex refusal, and concurrent isolation | PASS |
| Corrected native-401 contract: exactly one model POST; refresh begins once `401` status is known and before a pending body can postpone it; renewal failure is authoritative; successful renewal preserves the original refusal or body-read failure | `crates/shared/wyrd-client/src/transport/http.rs:351-410` branches on status and awaits `force_refresh` before reading the body | R3 records the pending-body raw-socket case producing `/auth/token`, one model POST, then `/auth/token`, plus the prior complete, failed-renewal, and cut-off cases | PASS — `FIND-TASK-003-2` closed |
| REQ-043/INV-012: protocol-native refusals retain only status, stable code, approved optional field, safe message, and remediation; recognized-code text comes from the Wyrd catalog | `crates/shared/wyrd-client/src/workflow/gateway.rs:157-214,313-339` | Recorded three-dialect canaries, uncoded-category cases, and catalog comparisons remain source-consistent | PASS — `FIND-TASK-003-3` remains closed |
| AC-011A/012 and INV-020: after authentication, affected public ingresses bound, decode, validate, consume, and never forward the fallback header; absent header retains tenant policy; served OpenAPI documents the contract | `crates/wyrd-spec/src/gateway/policy.rs`; `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs`, `routes.rs`, and served OpenAPI test | Original task records policy, authenticated PG ingress, and served OpenAPI proof across OpenAI Chat/Responses, Anthropic, and Gemini | PASS. Receiver-side byte recanonicalization is neither approved established behavior nor ordinary JSON receiver behavior and is not required |
| REQ-058 and INV-004/011/012: route selection is shared; explicit native injection remains; only selected ExtGateway secrets resolve at run; loading/apply dispatch nothing; public WyrdGateway uses a retained client or ambient client | `crates/shared/wyrd-client/src/workflow/mod.rs:34-210`; `workflow/local.rs:30-145`; `global_config.rs`; `wyrd-utils/src/secret.rs` | Original and R1 evidence covers selected/unselected bindings, retained public client, load-time behavior, and language context | PASS except for the single-snapshot portion below |
| R3 `FIND-TASK-003-10`: all task-added ambient filesystem work is off the async polling thread and one ambient `GlobalConfig` snapshot supplies both selected Workflow config and a client-less public-gateway client | `Workflow::run_with` moves setup to `spawn_blocking` at `workflow/mod.rs:135-142`, so the blocking boundary is correct; however `load_local_setup` calls `GlobalConfig::load` at `:195-196` and then `WyrdClient::from_global` at `:203`, whose `ClientConfig::from_global` loads `GlobalConfig` again (`client.rs:52-53`, `config.rs:96-98`) | R3 records the existing selected-dependencies, loader, and remote-client selectors, but those tests do not exercise a client-less mixed ExtGateway+WyrdGateway run or prove one snapshot; source contradicts the implementation claim | **FAIL — BEH-R4-001 / `FIND-TASK-003-10` reopened** |
| REQ-058/INV-007: Rust, Python, and TypeScript keep the shared Workflow and originating Cards/external-ref client through local execution; Python edits preserve it | Shared facade in `wyrd-client/src/workflow/mod.rs`; Python owner/handoff in `sdks/wyrd-sdk-python/src/workflow.rs` and `state/mod.rs`; TypeScript native wrapper stores the same shared Workflow | R1 records a Python journey distinguishing loading client A from ambient B/absence; existing Rust/TypeScript paths use the same owner | PASS — `FIND-TASK-003-1` remains closed |
| R3 `FIND-TASK-003-11`: `AuthMiddleware::force_refresh` accurately separates replay-safe retrying consumers from native send-once preparation for later calls | `crates/shared/wyrd-client/src/auth.rs:503-511` | R3 records auth-focused selectors and ordinary lint proof | PASS — `FIND-TASK-003-11` closed |
| R3 `FIND-TASK-003-12`: consuming `Workflow::into_skald` documents loss of retained client and automatic dependency composition | `crates/shared/wyrd-client/src/workflow/mod.rs:165-174`; current direct callers provide explicit dependencies | R3 records static documentation proof | PASS — `FIND-TASK-003-12` closed |
| R1/R2 source-contract corrections remain intact: Python client retention, safe recognized-code messages, private plaintext reader, module imports, panic contracts, cancellation/partial-progress docs, and post-dispatch cancellation proof | Current cumulative source at the previously cited shared-client, Python, server, spec, Skald, and test locations | R1/R2 recorded focused and static evidence; no later source change reintroduces those defects | PASS — `FIND-TASK-003-1`, `-3` through `-9` remain closed |
| Non-goals and drift boundary: no new ingress, Vertex public endpoint, credential mutation, provider-body Workflow context, polling/retry option, public arbitrary-header API, duplicate graph/parser/executor/transport, language remote lifecycle, or nonstandard mechanism/check/file/setting/option | Complete cumulative diff and live callers; R3 reuses Tokio's established blocking pool and existing configuration/auth/client owners | Source review plus recorded boundary evidence | PASS |
| AC-013 and cumulative remediation proof are credible for every claimed behavior | Original/R1/R2 evidence directly exercises the remote client, public caller, ingresses, context retention, safe errors, and cancellation; R3 directly proves pending-401 renewal | R3's existing selectors do not close its required single-ambient-snapshot correction, and source shows two reads on the mixed client-less route path | **FAIL — BEH-R4-001** |

## Proposed finding

### BEH-R4-001 — mixed client-less runs do not use one ambient configuration snapshot

- **Stable finding:** reopens and revises `FIND-TASK-003-10`.
- **Classification:** **INCORRECT / VIOLATION**.
- **Violated obligation:** R3's decision-complete correction requires the
  shared Workflow preparation owner to load one ambient `GlobalConfig`
  snapshot when selected routes require it and reuse the current
  `GlobalConfig`, `ClientConfig`, and `WyrdClient` construction owners for both
  Workflow bindings and a client-less public-gateway client. The original task
  also requires one shared selected-dependency path for mixed local routes.
- **Exact locations:**
  `crates/shared/wyrd-client/src/workflow/mod.rs:190-210`, especially
  `GlobalConfig::load` at `:195-196` and `WyrdClient::from_global` at `:203`;
  the second read is established by
  `crates/shared/wyrd-client/src/client.rs:52-53` and
  `crates/shared/wyrd-client/src/config.rs:96-98`.
- **Caller-to-result evidence:** `Workflow::run_with` computes both booleans
  from the resolved graph and calls `load_local_setup`. A wholly local authored
  Workflow retains no client. If its steps select both `ext_gateway` and
  `wyrd_gateway`, `needs_config` and `needs_gateway` are both true and `loaded`
  is `None`. The helper first reads the ambient file for
  `LocalWorkflowConfig`, then `WyrdClient::from_global` independently reads the
  same file again for endpoint/tenant/cache configuration before resolving
  credentials. The work is correctly off the async executor, but it is not one
  snapshot.
- **Observable consequence:** A config replacement between those reads can
  combine ExtGateway bindings from one revision with the public Wyrd endpoint,
  tenant, or token-cache settings from another. Even without a concurrent
  update, every such run performs duplicate synchronous file reads and parses,
  contrary to the approved correction.
- **Evidence assessment:** R3's implementation evidence claims that
  `load_local_setup` closes `FIND-TASK-003-10`, but it mentions only the
  blocking placement. `selected_local_dependencies_use_shared_config` prepares
  ExtGateway dependencies from an already-parsed value and separately tests a
  retained-client, gateway-only Workflow; it does not exercise the mixed,
  client-less helper branch. The recorded evidence therefore does not close
  the contradicted source contract.
- **Decision-complete minimum correction:** In the existing
  `load_local_setup` owner, load at most one ambient `GlobalConfig` when either
  selected Workflow config or a client-less gateway client needs it. Derive
  the Workflow section and, when needed, the client via the existing
  `ClientConfig::from_global_with_env` and `WyrdClient::with_config` behavior.
  Preserve retained Cards clients, route-first laziness, selected-secret-only
  resolution, current public errors, run-start timing, and the established
  blocking boundary. Do not introduce another loader, cache, watcher, async
  config API, option, setting, checker, or harness.
- **Focused closure proof:** Static inspection must establish one ambient
  snapshot on the mixed client-less path. Rerun and record the existing
  selected-local-dependencies and retained-client/language regressions. A
  timing test or synthetic filesystem fixture is not warranted.

## Prior-finding closure

| Finding | Closure assessment |
|---|---|
| `FIND-TASK-003-1` | Closed. Python retains the shared owner through both loading paths, mutation, and run. |
| `FIND-TASK-003-2` | Closed. Renewal begins on known `401` status before body collection and the native POST is never replayed. |
| `FIND-TASK-003-3` | Closed. Recognized codes use catalog text and discard provider-controlled messages. |
| `FIND-TASK-003-4` | Closed. Only the redacted secret-reader result crosses modules. |
| `FIND-TASK-003-5` | Closed. Required imports remain in module import blocks. |
| `FIND-TASK-003-6` | Closed. Changed declarations use imported bare or role-specific aliased types. |
| `FIND-TASK-003-7` | Closed. Identified panic-capable helpers/tests retain substantive panic contracts. |
| `FIND-TASK-003-8` | Closed. Durable operations retain accurate cancellation and partial-progress documentation. |
| `FIND-TASK-003-9` | Closed. Focused proof cancels after dispatch and observes no resend. |
| `FIND-TASK-003-10` | **Reopened/revised by BEH-R4-001.** Blocking placement is correct, but the approved one-snapshot composition is not implemented on the mixed client-less path. |
| `FIND-TASK-003-11` | Closed. The auth owner now distinguishes retrying and send-once callers. |
| `FIND-TASK-003-12` | Closed. `into_skald` now documents discarded client/composition context. |

## Open questions

None. The retained defect has a bounded correction through existing owners and
requires no specification, product, public API, architecture, security,
compatibility, persistent-data, or concurrency decision.

## Verification notes

- No command that builds, compiles, tests, formats, lints, lists tests, or
  executes a verification lane was run by this reviewer.
- The implementer records green focused evidence for pending-`401` renewal,
  existing Workflow selected-dependency behavior, loader behavior, remote
  transport, auth refresh, Clippy, formatting, and diff checks.
- That evidence is credible for the exercised source paths. It does not prove
  R3's one-snapshot requirement, and the two calls in source contradict the
  claimed closure; this is a finding for the implementer to correct and rerun.

## Overall result

**FAIL**

One bounded behavioral finding remains: **BEH-R4-001**, reopening
`FIND-TASK-003-10`.
