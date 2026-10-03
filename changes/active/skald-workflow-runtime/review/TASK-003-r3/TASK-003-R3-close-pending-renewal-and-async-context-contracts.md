---
id: TASK-003-R3
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-TASK-003-2, FIND-TASK-003-10, FIND-TASK-003-11, FIND-TASK-003-12]
---

# Close pending renewal and async context contracts

Implementation skill: `$wyrd-implement`.

## Authority and immutable review subject

- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations:
  `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`
  and
  `changes/active/skald-workflow-runtime/review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Reviewed candidate: `0732ed92fc2907cb806ac918b09abbf6d44ff04f`
- Validated ledger: `changes/active/skald-workflow-runtime/review/TASK-003-r3/findings-validation.md`

The human-approved 2026-10-03 `WyrdGatewayCall.model` amendment and the R1
same-spec native-`401` wording correction remain authoritative.

## Diagnosis and selected correction

### FIND-TASK-003-2 — pending native `401` bodies postpone renewal

`HttpTransport::post_native` knows the response is `401` after headers arrive,
but it awaits the complete body before calling `AuthMiddleware::force_refresh`.
The public caller races that entire future against cancellation and the
remaining deadline. A peer can therefore send `401` headers and hold the body
open until the future is dropped, leaving the known-refused cached credential
available to a later call. R2's raw response closes after a truncated body, so
it proves renewal after a completed body-read error rather than this pending
state.

Keep the single native model POST and the existing auth owner. Once status is
known to be `401`, attempt the existing refresh before body collection can
postpone it. A renewal failure remains authoritative. After successful renewal,
preserve the original refusal when its body is collected or the existing body-
read failure when it is not. Preserve the public caller's cancellation boundary
and sibling JSON, framed, and gRPC retry owners. Do not add cache retirement,
a detached task, response provenance, retry configuration, another transport,
or another auth owner.

### FIND-TASK-003-10 — ambient filesystem work runs on the async executor

Task-added local Workflow preparation has two reachable ambient paths that may
perform synchronous filesystem IO while `Workflow::run_with` is being polled.
A selected `ext_gateway` route calls `GlobalConfig::load`, which uses
`std::fs::read_to_string`. A client-less selected `wyrd_gateway` route calls
`WyrdClient::from_global`, which loads the same configuration and can resolve
file-backed ambient credentials synchronously. This violates the repository's
async boundary and can delay cancellation or unrelated work on the shared
runtime.

At the existing shared Workflow preparation owner, place the complete ambient
configuration/client assembly that may touch the filesystem on the established
Tokio blocking pool. Load one ambient `GlobalConfig` snapshot when selected
routes require it, and reuse the current `GlobalConfig`, `ClientConfig`, and
`WyrdClient` construction behavior for Workflow config and a client-less
public-gateway client. Preserve retained Cards clients, lazy route selection,
selected-secret-only resolution, current public errors, and the existing
blocking treatment of authored bundle and secret-file reads. Do not create an
async config API, cache, watcher, option, dependency, per-language path, timing
test, slow-filesystem fixture, new harness, or permanent check.

### FIND-TASK-003-11 — auth owner documentation promises retry for every caller

`AuthMiddleware::force_refresh` documents one workflow in which the caller
always retries once. TASK-003 adds a live native consumer that must never replay
its non-idempotent model POST: successful refresh prepares only later calls and
the current refusal is returned. The implementation is intentionally different
from replay-safe HTTP/gRPC callers, so the owner documentation is now false and
can lead a maintainer to restore the prohibited replay.

Qualify the existing rustdoc to distinguish replay-safe HTTP/gRPC operations,
which may retry after refresh, from the native model path, which refreshes for
later calls and returns the current refusal. Change no runtime behavior and add
no policy type, option, helper, retry, or enforcement mechanism.

### FIND-TASK-003-12 — consuming the shared Workflow silently drops context

TASK-003 added `client: Option<WyrdClient>` to the shared `Workflow` facade and
made it authoritative for automatic public-gateway dependency composition.
`Workflow::into_skald` consumes the facade and drops that endpoint, credential,
token-cache, connection-pool, and automatic-composition context, but its rustdoc
still says only that it takes the hydrated Skald Workflow. Existing direct
callers provide explicit dependencies, so the defect is the undocumented
ownership transition rather than a separate runtime failure.

Extend the existing rustdoc to state that conversion intentionally discards
retained client context and automatic shared dependency composition. Callers
that need those behaviors keep and run the shared `Workflow`; callers taking
the Skald value provide explicit dependencies. Add no wrapper, compatibility
path, new API, clone, guard, or checker.

## Preserved behavior and non-goals

- Preserve exactly one native model POST and no replay after `401`.
- Preserve renewal failure precedence and the existing original-refusal/body-
  read-error results after successful renewal.
- Preserve prompt cancellation and the public caller's documented local-IO
  cancellation boundary.
- Preserve retained Cards/client context and Python/TypeScript/Rust projection.
- Preserve lazy route selection, selected-only secret resolution, explicit
  native injection, fallback isolation, and current public errors.
- Preserve sibling JSON, framed, and gRPC retry behavior.
- Do not add a dependency, feature, public API, configuration surface,
  compatibility path, cache, watcher, setting, option, allowlist, checker,
  detached worker, generalized harness, or synthetic filesystem/load test.
- Do not broaden the change into unrelated async, auth, or documentation work.

## Acceptance criteria and closure proof

| Finding | Required acceptance and direct proof |
|---|---|
| `FIND-TASK-003-2` | With `401` headers received and the response body still pending, the existing renewal path begins before body release/closure; exactly one model POST occurs and no resend occurs. Complete-body renewal success/failure and immediate cut-off cases remain intact. Extend and rerun the existing `public_gateway_call_context_and_errors` selector using its existing loopback fixture. |
| `FIND-TASK-003-10` | Static inspection shows that selected external-binding config loading and client-less public-gateway ambient client assembly cannot perform filesystem IO on the async polling thread. The existing selected-local-dependencies proof and retained-client/language coverage are rerun and recorded. |
| `FIND-TASK-003-11` | `force_refresh` rustdoc accurately separates replay-safe callers from native send-once behavior. Ordinary documentation/lint evidence is recorded; no runtime test or bespoke check is added. |
| `FIND-TASK-003-12` | `Workflow::into_skald` rustdoc explicitly states the lost client/composition context and the caller responsibility after conversion. Current callers are inspected and ordinary documentation/lint evidence is recorded; no runtime test or bespoke check is added. |

Use the original TASK-003 and R1/R2 focused selectors and the narrowest current
repository tasks that cover the changed shared-client, language-boundary,
formatting, lint, and documentation surfaces. Record exact focused commands and
results. Follow current `mise.toml`; do not add or widen a gate, and do not run
the repository-wide aggregate unless the implementation's actual scope triggers
it under `AGENTS.md`.

## Completion and next review

Record source closure and exact implementation evidence against every stable
finding ID. Route this task directly to `$wyrd-implement`. The next
`$wyrd-task-review` must reassess the complete original base-to-new-candidate
range, the original task, all three remediation tasks, and all prior verdicts;
it must not review only the R3 delta.

## Implementation Evidence

Implemented in `7b3d03c18` on `wyrd/skald-workflow-runtime/TASK-003`. The human authorized this fourth remediation round on 2026-10-03.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-2 | `transport/http.rs::post_native` checks the status and calls `force_refresh` on `401` before reading the body; body is read afterwards. Rustdoc states renewal precedes the body read. | New case in `public_gateway_call_context_and_errors`: a `401` head with a pending, never-finished body (`Reply::Hold`) yields `/auth/token`, `/v1/chat/completions`, `/auth/token`, then the caller deadline ends the call. RED with the old order: `left: ["/auth/token", "/v1/chat/completions"]`. GREEN after the fix. | PASS |
| FIND-TASK-003-10 | `workflow/mod.rs`: `run_with` runs the new sync `load_local_setup` (config load + gateway client build) via `spawn_blocking`; `from_path` builds `Cards::new` via `spawn_blocking`. Shared `blocking_task_failed` maps join failures to `WYRD_WORKFLOW_500_INTERNAL`. Secret reads were already on the blocking pool (`local.rs::resolve_binding`). Swept `wyrd-client/src` for every `GlobalConfig::load`/`from_global`/`Cards::new` call: the remaining sites are synchronous constructors, not async paths. | `workflow::tests::selected_local_dependencies_use_shared_config`, `workflow::tests::from_path_uses_existing_loader`, `shared_workflow_client_contract` pass. | PASS |
| FIND-TASK-003-11 | `auth.rs::AuthMiddleware::force_refresh` rustdoc distinguishes the replay-safe transport (refresh then retry once) from the native gateway call (refresh, no retry). | `force_refresh_re_exchanges_once`, `workload_force_refresh_re_exchanges_once` pass. | PASS |
| FIND-TASK-003-12 | `Workflow::into_skald` rustdoc states it drops the loading client and automatic shared dependencies, pointing to `as_skald`/`as_skald_mut`. | Rustdoc only. | PASS |

Commands (all exit 0, run with `CARGO_TARGET_DIR=<repo>/target CARGO_BUILD_JOBS=12`):

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib --test workflow_transport -E 'test(/workflow::/) | test(=public_gateway_call_context_and_errors) | test(=shared_workflow_client_contract) | test(/force_refresh/)'   # 6 passed
mise exec -- cargo clippy --locked -p wyrd-client --all-targets --all-features -- -D warnings
mise exec -- cargo fmt --check -p wyrd-client
git diff --check
```

The RED run temporarily restored the previous body-first ordering in `post_native` and failed exactly on the new assertion; the fix was restored before committing. Non-goals stayed excluded: no retry or replay of the model POST was added, and no new configuration, check or mechanism was introduced.
