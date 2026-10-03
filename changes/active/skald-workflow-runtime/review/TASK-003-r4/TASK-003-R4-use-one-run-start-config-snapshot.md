---
id: TASK-003-R4
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-TASK-003-10]
---

# Use one run-start configuration snapshot

Implementation skill: `$wyrd-implement`.

## Authority and immutable review subject

- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations:
  `review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`,
  `review/TASK-003-r2/TASK-003-R2-close-renewal-proof-and-source-contracts.md`,
  and
  `review/TASK-003-r3/TASK-003-R3-close-pending-renewal-and-async-context-contracts.md`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Reviewed candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Validated ledger:
  `changes/active/skald-workflow-runtime/review/TASK-003-r4/findings-validation.md`

The human-approved `WyrdGatewayCall.model` amendment, native-`401` task
correction, and run-start `spawn_blocking` decision remain authoritative.
Configuration, client, and selected secret reads stay at run start because
Workflow loading must not read execution secrets.

## Issue diagnosis

### FIND-TASK-003-10 — mixed routes read two ambient configuration snapshots

R3 required the shared Workflow run-start owner to move filesystem-bearing
setup to the blocking pool and to load one ambient `GlobalConfig` snapshot for
all selected local route preparation. The candidate closes the blocking-thread
half but not the single-snapshot half.

`Workflow::run_with` computes whether selected routes need external bindings
and a public Wyrd gateway, then invokes `load_local_setup` on the blocking
pool. For a client-less Workflow containing both an `ext_gateway` step and a
`wyrd_gateway` step, `load_local_setup` first calls `GlobalConfig::load()` and
moves its `.workflow` section into the external-binding configuration. It then
calls `WyrdClient::from_global()`. That path reaches
`ClientConfig::from_global()` and calls `GlobalConfig::load()` again.

The producer is
`crates/shared/wyrd-client/src/workflow/mod.rs:190-210`; the second path is
visible through `crates/shared/wyrd-client/src/client.rs:47-54` and
`crates/shared/wyrd-client/src/config.rs:91-103`. The first snapshot supplies
external origins and secret references. The second supplies the Wyrd endpoint,
tenant, token-cache settings, and ambient credential assembly.

If the configuration is replaced between reads, one run can resolve external
bindings under configuration A while sending its governed Wyrd call under
configuration B. A successful first read can also be followed by a transient
second-read or parse failure. Even without rotation, the duplicate read
contradicts R3's decision-complete single-snapshot correction.

The R3 evidence does not close this gap. Its selected-dependencies test passes
an already-parsed external configuration and separately uses a retained client
for the gateway case, so it never reaches `load_local_setup(true, true, None)`.
R3 also required retained-client/language coverage after changing shared
`Workflow::run_with`, but its recorded commands omit the existing public
Python retained-client integration. That missing proof belongs to closure of
this same finding; it is not a separate implementation defect.

## Intended correction outcome

One synchronous run-start setup stage derives every ambient value needed by a
client-less mixed-route Workflow from one loaded `GlobalConfig`. A retained
Cards client remains authoritative and avoids ambient client assembly. Purely
native runs retain no ambient read. Selected route and secret behavior, public
errors, and the approved blocking boundary remain unchanged.

## Decision-complete recommendation

Correct the invalid state at its producer, the existing `load_local_setup`
owner. When selected external bindings or a client-less selected public Wyrd
gateway require ambient state, load at most one `GlobalConfig`. Derive the
Workflow section and the client configuration from that same value using the
existing `ClientConfig::from_global_with_env`; assemble the client through the
existing `WyrdClient::with_config`. When a retained Cards client is present,
reuse it and do not assemble an ambient client.

This is the smallest sufficient correction because all required constructors
already exist. Do not add a downstream equality/version guard: by the time
`SelectedRoutes::dependencies`, Skald, a language SDK, or the public gateway
caller sees the values, the split has already occurred. Do not add a cache,
watcher, generation token, lock, reload protocol, async config API,
configuration hook, setting, option, dependency, compatibility path, checker,
allowlist, new harness, timing test, or synthetic slow-filesystem fixture.

## Constraints and preserved behavior

- Preserve the human-approved run-start `spawn_blocking` boundary.
- Preserve route-first laziness and no ambient configuration read for a purely
  native run.
- Preserve retained Cards client precedence and its endpoint, credentials,
  token cache, and connection context.
- Preserve selected-secret-only resolution and resolve no execution secret
  during load or apply.
- Preserve existing public error mapping, including blocking-task failure
  projection.
- Preserve explicit native dependency injection and current Rust, Python,
  TypeScript, and CLI delegation to the shared Workflow owner.
- Preserve native model send-once and `401` renewal behavior, fallback/header
  isolation, remote lifecycle behavior, and all previously closed findings.
- Do not broaden this remediation into an unrelated configuration, client,
  SDK, auth, transport, or documentation refactor.

## Acceptance criteria

| Finding | Required acceptance |
|---|---|
| `FIND-TASK-003-10` | A client-less Workflow selecting both `ext_gateway` and `wyrd_gateway` loads no more than one ambient `GlobalConfig`; its external bindings and Wyrd client configuration derive from that same snapshot through existing owners. |
| `FIND-TASK-003-10` | A retained-client gateway run does not assemble an ambient gateway client; a purely native run performs no ambient configuration read. |
| `FIND-TASK-003-10` | Selected external secrets still resolve only at run start and only for selected bindings; loading and apply read no execution secret and dispatch nothing. |
| `FIND-TASK-003-10` | The existing public Python retained-client path still uses its loading client after the shared run-start correction, even when ambient configuration is absent or hostile. |
| `FIND-TASK-003-10` | No new configuration mechanism, setting, option, dependency, checker, harness, compatibility path, or language-specific execution implementation is introduced. |

## Focused proof and broader verification

Static source inspection must show one `GlobalConfig::load` feeding both
consumers on the client-less mixed-route path. Extend or reuse the existing
selected-local-dependencies test to exercise one Workflow selecting both route
families without a retained client; do not add a timing assertion or a new
fixture system. Record the exact focused command and selected count:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib \
  -E 'test(=workflow::tests::selected_local_dependencies_use_shared_config)'
```

Rerun and record the existing public Python runtime proof through its
repository-managed Postgres setup:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && \
   uv run python -m pytest -q -m integration \
   "tests/integration/gateway/test_workflow_gateway_context.py::test_loaded_workflow_calls_the_gateway_through_its_loading_client"'
```

Preserve the remote-client and authentication regressions required by R3 and
run the narrowest current `mise` tasks covering changed shared-client code,
Python runtime projection, formatting, and lints. Follow current `mise.toml`
and AGENTS.md; do not add or widen a gate, and do not run the repository-wide
aggregate unless the actual implementation scope independently requires it.

Record implementation locations, exact commands, selected counts, and results
in this task. The next `$wyrd-task-review` reassesses the complete cumulative
candidate against the original task and all four remediation packets.

