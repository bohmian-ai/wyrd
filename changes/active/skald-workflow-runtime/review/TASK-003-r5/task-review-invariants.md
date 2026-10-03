# TASK-003 r5 invariant review

**Overall result: FAIL.** The cumulative candidate closes the source defect in
`FIND-TASK-003-10`: client-less mixed-route setup now loads at most one ambient
`GlobalConfig` and derives both external bindings and the public-gateway client
from that snapshot. The required direct regression proof is still incomplete,
however. The new test calls the extracted helper with manually supplied
`true, true` flags; it never constructs a mixed-route `Workflow`, never lets
`SelectedRoutes` produce those flags, and never enters `Workflow::run_with`.
That contradicts TASK-003-R4's explicit closure requirement and leaves the
producer-to-consumer regression boundary unproved.

## Immutable subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Approved authority:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediations: TASK-003-R1 through TASK-003-R4 under
  `changes/active/skald-workflow-runtime/review/`

The human-approved 2026-10-03 `WyrdGatewayCall.model` amendment, the R1
same-spec native-`401` wording correction, and the approved run-start
`spawn_blocking` boundary for configuration, client, and selected-secret reads
were treated as authority. Loading remains secret-free.

This replacement review inspected the complete base-to-candidate diff, current
candidate source, the original task and all four remediation packets, the
approved specification, applicable repository and architecture authority, and
the recorded evidence. CodeGraph was attempted first and reported that no
`.codegraph/` index exists. Review was strictly source-only: no build, compile,
test, Cargo, mise, pnpm, pytest, formatter, linter, typecheck, codegen, diff
checker, or other verification command was run.

## Producer-to-sink invariant trace

- `Workflow::run_with` derives `SelectedRoutes` from the hydrated Workflow,
  takes `needs_config` and `needs_gateway`, clones the retained loading client,
  and executes `load_local_setup` on Tokio's blocking pool
  (`crates/shared/wyrd-client/src/workflow/mod.rs:131-146`). Secret resolution
  remains after route selection and at run start.
- `load_local_setup` loads `GlobalConfig` once exactly when external bindings
  are selected or a client-less public-gateway route needs ambient client
  assembly (`workflow/mod.rs:179-202`). It passes that owned snapshot to
  `local_setup_from`.
- `local_setup_from` reuses a retained Cards client when present. Otherwise it
  derives `ClientConfig` from the same `GlobalConfig` via
  `ClientConfig::from_global_with_env` and builds the gateway client through
  `WyrdClient::with_config`; it moves `global.workflow` into the selected
  binding configuration (`workflow/mod.rs:204-240`). The previous second call
  through `WyrdClient::from_global` is gone.
- `SelectedRoutes::dependencies` resolves only the names selected by
  `ext_gateway` routes, puts each secret-file read on the blocking pool, and
  installs a `PublicWyrdGatewayCaller` only when a `wyrd_gateway` route exists
  (`workflow/local.rs:30-138`). Unselected bindings and secrets are not read.
- Registered and externally hydrated Workflows retain the exact client that
  performed their Cards reads (`workflow/mod.rs:78-97,303-349`). Python keeps
  that shared facade through load, mutation, and run
  (`sdks/wyrd-sdk-python/src/state/mod.rs:2635-2638` and
  `sdks/wyrd-sdk-python/src/workflow.rs:204-215,305-435,537-580`); TypeScript's
  existing native wrapper also owns and runs `wyrd_client::Workflow`.
- Skald resolves the Prompt provider/model once into `StepRoute`, then creates
  a private attempt adapter containing model, fallback, deadline,
  cancellation, and correlation. Each provider call constructs a fresh
  `WyrdGatewayCall` (`crates/skald/skald-workflow/src/route.rs:281-309,360-427,
  543-590`). No call context is stored in shared provider or header state.
- `PublicWyrdGatewayCaller` projects the call to the existing protocol ingress,
  keeps the fallback header request-local, races the one native POST against
  cancellation and remaining timeout, and records correlation only in its
  span (`crates/shared/wyrd-client/src/workflow/gateway.rs:32-124`). Vertex is
  refused before IO.
- `HttpTransport::post_native` sends exactly one model POST. On `401`, it calls
  the existing `AuthMiddleware::force_refresh` after reading status and before
  collecting the body, never replays the model request, gives renewal failure
  precedence, and otherwise preserves the original response/body-read outcome
  (`transport/http.rs:351-411`). The outer caller continues to own deadline and
  cancellation.
- Public authentication middleware establishes the principal before a handler
  interprets the fallback header (`wyrd-server/.../gateway/ingress.rs:101-139`).
  `requested_fallback` rejects repeated or invalid values and consumes the
  header into typed `GatewayCallRequest.fallback`; the governed invocation
  receives no caller headers (`ingress.rs:189-224,249-303,332-430` and
  `routes.rs:1329-1346,1757-1806`).
- Native refusal normalization retains status, code, catalog/category message,
  optional OpenAI field, and remediation only; recognized-code message text is
  catalog-owned rather than envelope-owned
  (`workflow/gateway.rs:157-214,313-340`).
- `Workflows` remains a thin facade over shared authenticated transport: create
  uses one minted idempotency key across retries, get/cancel return direct
  snapshots, and wait polls once per second without cancelling the server run
  when dropped (`workflow/remote.rs:17-121`).

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Recorded verification evidence | Result |
|---|---|---|---|
| REQ-024/031/045/046 and AC-019: exact shared create/get/cancel/wait facade, stable create key, direct snapshots, one-second polling, drop without server cancellation | `workflow/remote.rs:17-121` delegates to the shared client and native DTOs | TASK-003 and R1-R4 record `shared_workflow_client_contract`; R4 records it in the combined shared-client selector | PASS |
| Rust SDK projects the same handles without Python features; Python/TypeScript gain no server-run lifecycle facade | `wyrd-client` exports plus `sdks/wyrd-sdk-rust/src/lib.rs`; no Python/TypeScript lifecycle additions | Original SDK and boundary records remain source-aligned | PASS |
| REQ-036A/038/039/043 and INV-020: model, fallback, remaining deadline, cancellation, and correlation are fresh immutable call values | Skald `StepRoute`/`WyrdGatewayProvider`; `PublicWyrdGatewayCaller::call` | Recorded route and transport cases cover model projection, concurrent fallback isolation, timeout, cancellation, supported dialects, and local Vertex refusal | PASS |
| Corrected native `401` behavior: one POST, refresh before body collection, no replay, renewal failure precedence | `transport/http.rs:376-411`; `auth.rs:503-544` | R2/R3 record complete, failed, truncated, and pending-body renewal cases in `public_gateway_call_context_and_errors` | PASS — `FIND-TASK-003-2` remains closed |
| Native refusal normalization retains only approved safe fields and catalog/category text | `workflow/gateway.rs:157-214,313-340` | Recorded three-envelope canary and uncoded-status cases remain source-aligned | PASS — `FIND-TASK-003-3` remains closed |
| AC-011A/012 and INV-020: authenticated ingress consumes the exact bounded fallback header and served OpenAPI documents it | `gateway/policy.rs:88-184`; `gateway/ingress.rs:189-224`; affected route assembly and OpenAPI annotations | Original/R1/R2 records include contract, PG ingress, and served OpenAPI evidence | PASS |
| REQ-058/INV-004/007/011/012: one shared local dependency path, retained client context, explicit native injection, selected-secret-only preparation, no secret at load/apply | `Workflow::run_with`, `SelectedRoutes`, shared secret reader, Python/TypeScript facade ownership | Prior shared and language evidence plus R4's recorded Python retained-client rerun | PASS for source behavior |
| R4 `FIND-TASK-003-10`: derive external bindings and an ambient public-gateway client from one run-start `GlobalConfig`; retained clients avoid ambient assembly; native-only runs read no ambient config | `workflow/mod.rs:179-240` has one load and one borrowed-snapshot client derivation | R4 records a helper-level unit case and the Python retained-client journey | PASS for implementation |
| R4 direct closure proof: an actual client-less Workflow selecting both route families exercises the shared `run_with` producer-to-consumer path; exact focused proof is recorded | `mixed_routes_use_one_config_snapshot` at `workflow/mod.rs:783-826` parses a snapshot and calls `local_setup_from(global, true, true, None)` directly; it constructs no Workflow, does not call `SelectedRoutes::of`, and does not enter `run_with` | R4 records a broad five-test expression, not the required exact `selected_local_dependencies_use_shared_config` command; the existing selector still tests external-only and retained-client gateway paths separately | **FAIL — INV-R5-001 / `FIND-TASK-003-10` proof remains open** |
| Prior source-contract findings: private plaintext helper, top-level imports, bare declaration types, panic/cancellation rustdoc, and context-discard documentation | Current source retains the R1-R3 corrections | Recorded static/lint evidence remains source-aligned | PASS — `FIND-TASK-003-4` through `-8`, `-11`, and `-12` remain closed |
| Post-dispatch cancellation is directly distinguished from deadline and sends no replay | Existing focused raw-server case observes the first POST before cancellation and reports no second path | R2/R3 record `public_gateway_call_context_and_errors` | PASS — `FIND-TASK-003-9` remains closed |
| Non-goals: no new ingress, Vertex endpoint, credential mutation API, provider-body Workflow context, polling knob, public arbitrary-header API, duplicate graph/parser/transport/executor, cache/watcher/check/setting/option/compatibility path | Complete cumulative diff and callers | Static inspection | PASS |

## Proposed finding

### INV-R5-001 — MISSING: R4 does not directly prove the client-less mixed-route Workflow path

- **Stable finding:** `FIND-TASK-003-10` remains open for incomplete closure
  proof; no new stable defect ID is warranted because the source correction and
  its required regression boundary are one remediation obligation.
- **Violated obligation:** TASK-003-R4's focused-proof section requires the
  existing selected-local-dependencies proof to exercise one client-less
  `Workflow` selecting both `ext_gateway` and `wyrd_gateway`, and requires the
  exact focused selector and selected count to be recorded. The standing review
  direction makes missing or contradicted recorded evidence a finding to rerun.
- **Exact locations:**
  `crates/shared/wyrd-client/src/workflow/mod.rs:582-758,783-826` and
  `changes/active/skald-workflow-runtime/review/TASK-003-r4/TASK-003-R4-use-one-run-start-config-snapshot.md:122-151,160-177`.
- **Evidence:** `mixed_routes_use_one_config_snapshot` never creates a
  `Workflow`; its only production call is
  `local_setup_from(global, true, true, None)`. The test therefore assumes the
  exact two route-selection values whose producer-to-sink wiring R4 required it
  to prove. `selected_local_dependencies_use_shared_config` still exercises an
  external-only Workflow through a helper that bypasses `Workflow::run_with`,
  then a separate gateway-only Workflow with an injected retained client. No
  recorded case reaches `Workflow::run_with -> SelectedRoutes::of ->
  load_local_setup` for a client-less mixed graph. The implementation record
  also substitutes a broad expression selecting five tests for R4's prescribed
  exact focused command and gives no individual selected count for the required
  selector.
- **Observable consequence:** the current source is correct, but the required
  regression can return undetected if mixed-route selection stops setting both
  needs flags, if `run_with` stops passing them to the one-snapshot owner, or if
  retained-client state is accidentally introduced on the authored client-less
  path. The helper test would remain green because it supplies those values
  manually.
- **Required testable correction:** extend the existing
  `selected_local_dependencies_use_shared_config` proof, using its current
  Workflow/config/client fixtures, so an actual client-less Workflow containing
  both route families enters the shared run-start composition path and proves
  that the selected bindings and ambient Wyrd client come from the same
  snapshot. Preserve route-first laziness, selected-only secret reads, retained
  client precedence, and no dispatch during loading. Add no configuration hook,
  timing assertion, generalized harness, setting, option, checker, or new
  dependency. Record the exact R4 selector and selected count, plus the existing
  Python retained-client journey. This is proof remediation only; the current
  production correction should remain unchanged.

## Prior-finding closure

| Stable finding | Result |
|---|---|
| `FIND-TASK-003-1` | CLOSED IN SOURCE AND RECORDED EVIDENCE. Shared and Python Workflows retain their loading client through mutation and run; R4 records the post-change Python journey. |
| `FIND-TASK-003-2` | CLOSED. One native POST; renewal begins on known `401` before body collection; no replay. |
| `FIND-TASK-003-3` | CLOSED. Recognized-code portable text comes from the catalog. |
| `FIND-TASK-003-4` | CLOSED. Only `read_secret_ref -> SecretString` is public across modules. |
| `FIND-TASK-003-5` | CLOSED. Identified imports remain at module scope. |
| `FIND-TASK-003-6` | CLOSED. Changed declarations use imported bare or role-specific aliases. |
| `FIND-TASK-003-7` | CLOSED. Identified panic-capable helpers/tests retain substantive panic contracts. |
| `FIND-TASK-003-8` | CLOSED. Durable operations document cancellation and partial progress. |
| `FIND-TASK-003-9` | CLOSED. Direct proof cancels only after dispatch and observes no resend. |
| `FIND-TASK-003-10` | SOURCE CORRECTION CLOSED; REQUIRED DIRECT PROOF STILL OPEN as `INV-R5-001`. |
| `FIND-TASK-003-11` | CLOSED. Auth-owner documentation distinguishes replay-safe and native send-once consumers. |
| `FIND-TASK-003-12` | CLOSED. `into_skald` documents discarded client/composition context. |

## Verification notes

No commands that build, compile, test, lint, format, typecheck, generate, or
otherwise verify the candidate were run. Recorded implementation evidence was
judged only against source. The recorded remote lifecycle, native call,
fallback ingress, OpenAPI, language-boundary, and retained-client results are
not contradicted by current source. R4's one-snapshot implementation evidence
is source-aligned, but its mixed-route regression evidence is incomplete for
the reason above.
