# TASK-003 r6 behavior review

## Immutable subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `b954976648b49429f1c0950c4fa3509573885be8`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediation authority: TASK-003 R1 through R5 and their prior verdicts and validated ledgers

I reviewed the complete base-to-candidate range, not only the R5 delta. The
human-approved `WyrdGatewayCall.model: ModelRef` amendment, the same-spec native
`401` send-once correction, and the run-start blocking/single-snapshot decisions
are part of the accepted authority. The candidate remained the stated commit
through final inspection.

This review was strictly source-only. I did not build, compile, run tests,
Cargo, mise, pnpm, pytest, formatting, linting, code generation, test listing,
or any verification lane. Verification references below are records already
committed in the original task and R1-R5 remediation packets; they were checked
for correspondence with the candidate source, not rerun.

The repository has no `.codegraph/` directory, so ordinary Git and source
navigation were used. The main caller paths inspected were:

- `Workflows::{create,get,cancel,wait}` -> the existing `WyrdClient` idempotent
  and authenticated HTTP transport;
- shared/Python/TypeScript loaded `Workflow` -> retained client facade ->
  `Workflow::run_with` -> `SelectedRoutes` -> one blocking run-start
  configuration snapshot -> Skald execution dependencies;
- Skald plan -> immutable per-attempt `WyrdGatewayCall` ->
  `PublicWyrdGatewayCaller` -> crate-private native HTTP operation;
- authenticated OpenAI, Anthropic, and Gemini ingress -> fallback-header
  decoding/validation -> existing `GatewayInvocation`;
- native refusal -> protocol envelope normalization -> `RemoteProblem` ->
  portable Workflow error projection.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-031/045/046 and AC-019: one shared create/get/cancel/wait handle, direct run snapshots, stable create idempotency, fixed one-second polling, terminal failures returned as values, and dropping wait does not cancel | `crates/shared/wyrd-client/src/workflow/remote.rs:23-122` delegates to the existing shared client and `submit_idempotent`; no remote lifecycle implementation appears in an SDK | Original `shared_workflow_client_contract` record covers first acceptance/replay, stable key under retry, get/cancel, terminal statuses, polling error, and dropped wait; R1-R3 reran the exact selector | PASS |
| Rust SDK projects the same shared handles without enabling a second implementation or Python behavior | `crates/shared/wyrd-client/src/lib.rs:32-40`; `sdks/wyrd-sdk-rust/src/lib.rs:27-49` | Original `test:wyrd-sdk`, client-tier, SDK-client-tier, and PyO3-scope records | PASS |
| REQ-036A/038/039/043 and INV-020: each call carries immutable Prompt-derived model, fallback, remaining deadline, cancellation, and trace-only correlation; supported public dialects are used and Vertex is refused before IO | `crates/skald/skald-workflow/src/plan.rs` resolves the Prompt before route construction; `route.rs:62-94,266-427,543-608` stores model/fallback on private attempt adapters; `crates/shared/wyrd-client/src/workflow/gateway.rs:50-124,217-286` projects each call without shared mutable metadata | Original `public_gateway_call_context_and_errors` and Skald route records cover concurrent fallback isolation, four public dialect projections, model identity, local Vertex refusal, pre-cancel, timeout, and post-dispatch cancellation; R2 added the direct post-dispatch case | PASS |
| Native model calls are sent once; any observed `401` begins existing-owner renewal before body collection; renewal failure is authoritative and the refused call is never replayed | `crates/shared/wyrd-client/src/transport/http.rs:351-410`; `crates/shared/wyrd-client/src/auth.rs:499-514` | R1-R3 records cover complete-body renewal success/failure, spoofed known code, cut-off body, pending body, one model POST, and no resend | PASS |
| Native errors retain only safe common status/code/message/optional OpenAI field/remediation; known codes use catalog text; uncoded statuses retain existing provider categories | `crates/shared/wyrd-client/src/workflow/gateway.rs:157-214,299-340` discards native message text and all details except `field`; category selection reuses `ProviderError::from_status` | Original/R1 focused records cover OpenAI, Anthropic, Google, known-code canaries, uncoded 400/401/408/429/503/504 behavior, and absence of raw-body canaries | PASS |
| AC-011A: authenticated public OpenAI Chat/Responses, Anthropic, and Gemini ingress consume exactly one bounded fallback header, validate it against the requested model, refuse bad input before dispatch, and never forward it | `crates/wyrd-spec/src/gateway/policy.rs:88-184`; `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:189-224,261-303,343-430`; `components/gateway/routes.rs:863-920,1757-1800` interprets fallback only after a verified caller exists | Original policy/PG records cover round-trip, malformed/oversized/empty/duplicate/self-reference, absence compatibility, all affected ingresses, and non-forwarding | PASS |
| REQ-058 and AC-029/031 task slice: local dependency assembly is shared, route-first, selected-secret-only, and preserves explicit Native injection and a loading client's connection context | `crates/shared/wyrd-client/src/workflow/mod.rs:36-188`; `workflow/local.rs:30-149`; Python retains `ClientWorkflow` at `sdks/wyrd-sdk-python/src/workflow.rs:204-215,305-435,537-580`; TypeScript retains and runs the shared facade at `sdks/wyrd-sdk-ts/native/src/workflow.rs:21-49,126-160` | Original selected-dependencies evidence; R1 public Python hostile/absent-ambient retained-client journey; existing Python and TypeScript runtime/type records. The task explicitly assigns the full real route journeys to TASK-005 | PASS |
| One client-less mixed-route run derives external bindings and its public gateway client from one run-start `GlobalConfig` snapshot on the blocking pool; retained-client and Native-only paths avoid ambient assembly | `Workflow::run_with` and `load_local_setup` at `crates/shared/wyrd-client/src/workflow/mod.rs:115-161,190-238` contain one `GlobalConfig::load`; `workflow/local.rs:62-110` consumes the two outputs | R5 exact-selector record for `selected_local_dependencies_use_shared_config`: one real client-less Workflow traverses route selection/run-start/dependency consumption, uses both configured boundaries, and passed 1 selected test with 221 skipped; its mutation check failed when the gateway ceased using the snapshot | PASS |
| Loading/apply resolve no execution secret and dispatch nothing; only selected binding secret refs resolve at run start, off the async polling thread | `Workflow::from_path` at `workflow/mod.rs:46-97` hydrates only; `SelectedRoutes::dependencies` and `resolve_binding` at `workflow/local.rs:72-149` resolve only names in the selected `BTreeSet`, each through `spawn_blocking`; shared secret output remains `SecretString` in `wyrd-utils/src/secret.rs:22-60` | Original selected-dependencies record covers no load call, unused unreadable secret, selected secret, missing/wrong/unreadable refusal before dispatch, and no secret in the result; R5 shared-family record remains green | PASS |
| Served API and durable authority describe the shipped fallback boundary and changed local-run behavior | Four route annotations use `FALLBACK_HEADER_DOC`; `architecture/wyrd-design.md:400-445` records encoding, bounds, authentication order, refusal, consumption, and absent-header behavior; Rust operation docs at `workflow/mod.rs:99-161` and `workflow/local.rs:72-149` record ambient IO/cancellation; Python source and assembled stubs agree at `python/wyrd/stubs/agent.pyi:716-743` and `python/wyrd/agent/__init__.pyi:717-744` | Original served OpenAPI record; R5 `codegen:check`, Python format/lint/typecheck, and source-comparison records | PASS |
| No prohibited expansion: no new ingress or Vertex endpoint, credential mutation API, provider-body Workflow field, polling option, arbitrary-header public API, language remote lifecycle, second parser/graph/validator/executor, cache/watcher/generation protocol, bespoke check, or compatibility path | Complete cumulative diff; `post_native` stays crate-private, `Workflows` and the public caller reuse the shared client, Skald retains one executor, and R5 removed the helper-only proof rather than adding a hook | Source inspection plus the recorded client-tier/PyO3/codegen/language evidence | PASS |
| Human standing DRIFT rule: every added mechanism/file/setting/option is required by approved authority or ordinary existing practice | The split workflow modules are cohesive owners; `Workflows` is a conventional dependency-owning facade; request-local typed header encoding, Tokio blocking pool, standard auth refresh, and existing protocol envelope types are reused. The new external transport test earns its file by driving real HTTP/mock boundaries. No new permanent checker, feature, cache, runtime, retry knob, or fixture framework was added | Source comparison with existing client/transport/auth/test owners and all five remediation non-goals | PASS |

## Prior finding closure

| Stable finding | Closure in candidate `b954976648b49429f1c0950c4fa3509573885be8` |
|---|---|
| `FIND-TASK-003-1` | **CLOSED.** Python owns `ClientWorkflow`, mutates its inner Skald value through `as_skald_mut`, and runs that same facade. Registered and authored-ref loads therefore retain their loading client. The recorded Python journey proves explicit client A wins over absent/hostile ambient configuration. |
| `FIND-TASK-003-2` | **CLOSED.** `post_native` sends once, calls `force_refresh` immediately after reading `401` status and before reading the body, propagates refresh failure, and never replays. Recorded complete, truncated, and held-body cases exercise the ordering. |
| `FIND-TASK-003-3` | **CLOSED.** `Ingress::problem` ignores envelope message text; a recognized code uses the derive-backed catalog title/remediation and uncoded responses use fixed provider-category text. |
| `FIND-TASK-003-4` | **CLOSED.** `read_secret_file` is private; the shared cross-module boundary returns `SecretString` through `read_secret_ref`. |
| `FIND-TASK-003-5` | **CLOSED.** The identified function-local imports are in their owning module import blocks under the same cfg restrictions. |
| `FIND-TASK-003-6` | **CLOSED.** Changed declarations use imported bare or role-specific aliases (`Bytes`, `LoadedTree`, request/response types, `Prompt`, `SkaldWorkflow`, and `ClientWorkflow`). |
| `FIND-TASK-003-7` | **CLOSED.** The task-owned panic-capable helpers and tests retain substantive `# Panics` contracts naming fixed fixture, parsing, serialization, recording, and assertion invariants. |
| `FIND-TASK-003-8` | **CLOSED.** Remote create/cancel, native POST, public gateway caller, and shared run operations document local cancellation, possible post-dispatch progress, retry/idempotency, and no rollback. |
| `FIND-TASK-003-9` | **CLOSED.** The focused transport source waits until `/v1/chat/completions` is observed, then cancels a 300-second call and asserts the cancellation category without another request. |
| `FIND-TASK-003-10` | **CLOSED.** `load_local_setup` has one `GlobalConfig::load` feeding both consumers, and the R5 replacement test reaches it through a real client-less mixed-route `Workflow::run_with`, consuming both the binding and public client. The exact selector/count and Python retained-client rerun are recorded. |
| `FIND-TASK-003-11` | **CLOSED.** `AuthMiddleware::force_refresh` documentation distinguishes replay-safe HTTP/gRPC retry from native send-once renewal for later calls. |
| `FIND-TASK-003-12` | **CLOSED.** `Workflow::into_skald` states that it discards the loading client and automatic dependency composition, and points callers needing those behaviors to retained borrows/the shared facade. |
| `FIND-TASK-003-13` | **CLOSED.** The active Workflow section of `architecture/wyrd-design.md` records the exact authenticated fallback-header contract implemented by the typed codec and public ingresses. |
| `FIND-TASK-003-14` | **CLOSED.** `Workflow::run`, `run_with`, `SelectedRoutes::dependencies`, and `resolve_binding` accurately document both ambient-config triggers and partial progress/already-started blocking reads after future drop. |
| `FIND-TASK-003-15` | **CLOSED.** The hand-authored Python stub and assembled public stub describe Native, retained/ambient public-gateway, and selected external-secret behavior consistently with the PyO3 owner. |

## Proposed findings

None. I found no reachable missing, incorrect, drifted, violating, or
regressed behavior within TASK-003's approved boundary. The R5 correction uses
existing owners and standard mechanisms and introduces no qualifying DRIFT.

## Verification evidence and limits

The cumulative implementation record reports the exact transport, selected
dependency, policy, Skald, gateway, PG ingress, served OpenAPI, SDK, Python,
TypeScript, codegen, boundary, formatting, and lint lanes required by the
original task. R1-R3 add direct proof for retained Python context, one-send
native renewal across complete/cut-off/pending `401` bodies, safe known-code
normalization, and post-dispatch cancellation. R4 records the single-snapshot
production correction and Python regression. R5 records the exact caller-level
mixed-route selector (1 passed, 221 skipped), `test:shared` (708 passed, 15
skipped), the public Python retained-client journey (1 passed), codegen and
Python declaration checks, ordinary Rust lint/format checks, and a mutation
that turns the focused mixed-route test red when the gateway client no longer
uses the loaded snapshot.

Those results are historical recorded evidence, not commands executed by this
review. Source inspection found no contradiction between the recorded cases
and candidate code. Full accepted-run/in-process server and real
Workflow-to-public-gateway language journeys remain assigned by the approved
task to TASK-004 and TASK-005; they are not silently claimed as TASK-003 proof.

## Overall result

**PASS**

The cumulative candidate satisfies the original TASK-003 behavior and every
R1-R5 remediation obligation exactly. The acceptance ledger is empty.
