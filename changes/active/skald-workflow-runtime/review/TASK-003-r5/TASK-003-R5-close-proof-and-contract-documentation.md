---
id: TASK-003-R5
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-TASK-003-10, FIND-TASK-003-13, FIND-TASK-003-14, FIND-TASK-003-15]
---

# Close mixed-route proof and public contract documentation

Implementation skill: `$wyrd-implement`.

## Authority and immutable review subject

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations: TASK-003 R1 through R4 under
  `changes/active/skald-workflow-runtime/review/`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Reviewed candidate: `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069`
- Validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-003-r5/findings-validation.md`

The human-approved `WyrdGatewayCall.model: ModelRef` amendment, native-`401`
task correction, and run-start `spawn_blocking` decision remain authoritative.
Configuration, client, and selected-secret reads stay at run start because
Workflow loading must not read execution secrets.

## Issue diagnosis

### FIND-TASK-003-10 — the corrected single-snapshot path lacks its required caller-level proof

The production correction is present. `Workflow::run_with` derives selected
route needs, invokes `load_local_setup` on the approved blocking pool, and
`load_local_setup` performs at most one `GlobalConfig::load` before
`local_setup_from` derives both external bindings and any client-less public
gateway client from that value.

The new `mixed_routes_use_one_config_snapshot` fixture does not exercise that
path. It calls `local_setup_from(global, true, true, None)` directly, so it
assumes both selected-route flags and bypasses the `Workflow`,
`SelectedRoutes::of`, `run_with`, `spawn_blocking`, `load_local_setup`, and
dependency-consumption seams that R4 expressly required the focused proof to
cover. The older named test covers an external-only Workflow and a separate
retained-client gateway Workflow, not one client-less Workflow selecting both
route families. R4 also records a five-test union rather than the required
exact selector and selected count.

The consequence is a proof gap, not a currently observed runtime split: route
discovery or run-start wiring can regress while the helper-only fixture stays
green. The source correction remains the right owner and must not be replaced
with a cache, generation token, timing hook, reload protocol, or new fixture
system.

### FIND-TASK-003-13 — the active gateway architecture omits the shipped fallback-header contract

The typed contract and authenticated public ingresses implement and publish
`wyrd-gateway-fallback`, but Revision 12 `INV-015` also requires the active Wyrd
gateway architecture authority to record it. `architecture/wyrd-design.md` is
the repository's active design authority and currently omits this
security-relevant request boundary.

The missing authority text leaves encoding, bounds, authentication order,
pre-dispatch refusal, consume-without-forwarding behavior, and absent-header
compatibility discoverable only from implementation and served OpenAPI. The
approved behavior already exists; this is synchronization, not a new design
decision and not a reason to create another architecture file or check.

### FIND-TASK-003-14 — shared run-start Rust documentation is inaccurate or incomplete

`Workflow::run_with` says shared client configuration is loaded only for an
`ext_gateway` route. Source also loads ambient configuration for a selected
client-less `wyrd_gateway` so it can construct the public gateway client.
`Workflow::run` delegates to that path but omits its local-drop and possible
post-dispatch progress contract. `SelectedRoutes::dependencies` and
`resolve_binding` can complete earlier selected reads, and an already-started
blocking secret read can finish after the async future is dropped, but their
operation-local rustdoc does not say so.

These are concrete contradictions or omissions at the deliberately established
R3/R4 run-start boundary. Runtime behavior is correct. The correction is
documentation on the existing owners, not a new cancellation mechanism,
option, helper, checker, or test harness.

### FIND-TASK-003-15 — the shipped Python declaration still describes local-only execution

The PyO3 `Workflow.run` owner now delegates to the shared client Workflow and
documents that selected public gateway routes may perform authenticated Wyrd
IO, selected external routes resolve configured secrets at run start, and a
loaded Workflow retains its loading client. The source declaration at
`sdks/wyrd-sdk-python/python/wyrd/stubs/agent.pyi` still says the method runs
against only the process-local provider registry. The existing assembly script
copies that stale contract into the public package stub.

A green generation check cannot establish semantic parity while its source
declaration is wrong. Correct the hand-authored source and regenerate through
the existing assembly owner; do not hand-edit the assembled output or create a
second generator or declaration layer.

## Intended correction outcome

The existing runtime remains unchanged. One focused regression proves the
real client-less mixed-route Workflow path uses both halves of one run-start
configuration snapshot. The active gateway architecture, Rust operation docs,
and Python public declaration accurately describe the implemented and approved
contracts.

## Decision-complete recommendation

1. Reuse the existing Workflow, route, configuration, public-gateway, and
   deterministic upstream fixtures. Extend or replace the existing
   `selected_local_dependencies_use_shared_config` proof so one client-less
   Workflow selecting both `ext_gateway` and `wyrd_gateway` reaches the actual
   `Workflow::run_with` run-start owner and demonstrates that the selected
   external binding and public-gateway client both come from the single ambient
   snapshot. Keep the existing production implementation and lower-level
   helper proof unless ordinary local cleanup makes the latter redundant.
2. In the existing gateway section of `architecture/wyrd-design.md`, record the
   implemented optional authenticated `wyrd-gateway-fallback` contract:
   unpadded base64url over JCS UTF-8 `GatewayFallbackOverride`, 8 KiB encoded
   and 4 KiB decoded limits, authentication before parsing, invalid-request
   refusal before dispatch, ingress consumption without provider forwarding,
   and unchanged tenant fallback policy when the header is absent.
3. Correct `Workflow::run_with` rustdoc to name both ambient configuration
   triggers: selected external bindings and a selected public Wyrd gateway
   without a retained loading client. Document on `Workflow::run` the same
   applicable caller-drop/already-dispatched boundary as `run_with`. Document
   on `SelectedRoutes::dependencies` and `resolve_binding` that earlier reads
   may have completed and an already-started blocking read may finish after
   drop, while dependency preparation itself dispatches no model call.
4. Update the hand-authored `python/wyrd/stubs/agent.pyi` documentation for
   `Workflow.run` to match the PyO3 owner: Native routes use the process
   registry, public Wyrd gateway routes use the retained or ambient Wyrd
   client, and external routes resolve only selected configured secrets at run
   start. Regenerate the assembled public stub through the existing
   `assemble_stubs.py` owner.

These corrections belong at existing owners and sources. They require no new
public API, runtime behavior, configuration mechanism, cache, watcher,
generation token, setting, compatibility path, dependency, checker, allowlist,
test harness, or timing assertion.

## Constraints and preserved behavior

- Preserve one run-start `GlobalConfig` snapshot for a client-less mixed route.
- Preserve retained Cards client precedence and purely Native no-ambient-read
  behavior.
- Preserve route-first laziness and selected-secret-only resolution at run
  start; loading and apply read no execution secret and dispatch nothing.
- Preserve the approved `spawn_blocking` boundary and current error mapping.
- Preserve native model send-once and `401` renewal-before-body behavior,
  fallback isolation, cancellation, remote lifecycle behavior, and every
  previously closed finding.
- Preserve the existing first-class SDK delegation and prohibition on
  Python/TypeScript server-run lifecycle methods.
- Do not broaden into runtime refactoring, a new architecture file, a new
  declaration generator, or unrelated documentation cleanup.

## Acceptance criteria

| Finding | Required acceptance |
|---|---|
| `FIND-TASK-003-10` | One client-less Workflow selecting both route families traverses the actual shared run-start path and demonstrates use of both the selected external binding and public-gateway client from the same ambient snapshot. The prescribed exact selector and selected count are recorded. |
| `FIND-TASK-003-10` | Retained-client precedence, native-only laziness, selected-secret-only resolution, and the public Python retained-client behavior remain intact. |
| `FIND-TASK-003-13` | The active gateway architecture states the same fallback-header encoding, limits, authentication order, refusal, consumption/non-forwarding, and absent-header behavior as the typed contract, ingress, and served operation. |
| `FIND-TASK-003-14` | Each cited Rust operation's own rustdoc accurately states its ambient IO trigger and applicable cancellation/partial-progress behavior; runtime behavior is unchanged. |
| `FIND-TASK-003-15` | The Python source declaration and regenerated public stub describe the same Native, public-gateway, retained/ambient-client, and selected-secret behavior as the PyO3 owner. |
| All | No new mechanism, option, dependency, checker, compatibility path, language-specific executor, or unrelated scope is introduced. |

## Focused proof and broader verification

Run and record the exact R4 selector and selected count:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=workflow::tests::selected_local_dependencies_use_shared_config)'
```

Rerun and record the existing public Python retained-client journey through its
repository-managed Postgres setup:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && \
   uv run python -m pytest -q -m integration \
   "tests/integration/gateway/test_workflow_gateway_context.py::test_loaded_workflow_calls_the_gateway_through_its_loading_client"'
```

Use source inspection to confirm one `GlobalConfig::load` feeds both live
mixed-route consumers and the active architecture matches the typed ingress
contract. Then run the narrowest current repository tasks covering the changed
surfaces: shared-client tests, Rust formatting and lints, Python formatting,
lints and public type checking, and generated-stub drift (`codegen:check`).
Follow current `mise.toml`; do not add or widen a gate, and do not run the
repository-wide aggregate unless the actual implementation scope independently
requires it under `AGENTS.md`.

## Completion and next review

Record exact implementation locations, commands, selected counts, and results
against all four stable findings. Route this task directly to
`$wyrd-implement`. The next `$wyrd-task-review` must reassess the complete
original base-to-new-candidate range, the original task, all five remediation
tasks, and all prior verdicts; it must not review only the R5 delta.
