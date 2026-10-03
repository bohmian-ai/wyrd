---
id: TASK-003-R2
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-003
remediates: [FIND-TASK-003-2, FIND-TASK-003-6, FIND-TASK-003-7, FIND-TASK-003-8, FIND-TASK-003-9]
---

# Close native renewal, direct cancellation proof, and source contracts

Implementation skill: `$wyrd-implement`.

## Authority and immutable review subject

- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediation: `changes/active/skald-workflow-runtime/review/TASK-003-r1/TASK-003-R1-preserve-client-context-and-native-call-safety.md`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Reviewed candidate: `2e6f13f505daf19050e588ea0d1d7fc936697e27`
- Validated ledger: `changes/active/skald-workflow-runtime/review/TASK-003-r2/findings-validation.md`

The 2026-10-03 minimal Revision 12 amendment adding
`pub model: ModelRef` to `WyrdGatewayCall` and the same-spec R1 native-`401`
wording correction remain approved authority.

## Diagnosis and selected correction

### FIND-TASK-003-2 — an unreadable native `401` body bypasses renewal

`HttpTransport::post_native` sends the model request once and knows the HTTP
status before it collects the response body. It currently propagates body-read
failure before calling `AuthMiddleware::force_refresh`. A `401` with a
truncated body or connection reset after headers therefore leaves the refused
cached credential available to the next call, contrary to the approved R1
outcome that every observed native `401` renews through the existing auth
owner without replay.

Keep the single model POST and the existing authentication owner. Once the
status is known to be `401`, renewal must be attempted regardless of whether
body collection succeeds. A renewal failure remains authoritative. After
successful renewal, return the original status/body when collection succeeded
or preserve the existing body-read transport failure when it did not. Do not
change sibling JSON, framed, or gRPC retry behavior and do not add response
provenance, retry configuration, or model-call replay.

### FIND-TASK-003-6 — changed declarations bypass the import manifest

New or changed fields and signatures use qualified paths for `Bytes`,
`LoadedTree`, Wiremock requests, Skald `Prompt`, Axum `Response`, and the
shared-client `Workflow`. This violates the mandatory module-top import and
bare-name rule, and it obscures owner identity at the Python boundary where
Skald and shared-client Workflow types coexist.

Use the owning module import blocks and bare, role-specific names at every
location identified by the validated ledger. Where two Workflow types share a
module, alias them by ownership and use the aliases consistently. This is a
source-shape correction only; add no wrapper, feature, allowlist, or checker.

### FIND-TASK-003-7 — new panic-capable tests and helpers lack panic contracts

The new task-owned tests and helpers listed in the validated ledger can panic
through assertions, fixed-fixture parsing/serialization, request recording, or
existing panicking parsers, but their rustdoc omits `# Panics`. Repository
authority makes this documentation mandatory for tests and private helpers as
well as public code.

Add concise `# Panics` sections only to the identified new panic-capable items.
Name their actual fixture, parsing, serialization, recording, or assertion
invariants. Do not add boilerplate to items without a real panic path, replace
the tests, introduce a new harness, or refactor production behavior solely to
avoid test-only panics.

### FIND-TASK-003-8 — durable operations omit cancellation and partial progress

The new remote `create` and `cancel` methods and the native POST operation do
not document their post-dispatch cancellation boundaries. A caller can lose a
response after the server accepted a run, applied cancellation, or dispatched a
non-idempotent model call. Existing behavior is acceptable, but the owning API
does not tell callers that local future cancellation is not server rollback.

Extend the existing rustdoc on these operations with their actual
cancellation, partial-progress, idempotency, and retry boundaries. Preserve
the current stable create key, send-once native call, reactive renewal, and
server ownership. Do not add a cancellation protocol, response marker,
setting, option, or retry mechanism.

### FIND-TASK-003-9 — post-dispatch cancellation lacks direct proof

The existing focused transport test cancels before the call is polled and
separately proves an in-flight timeout. It does not exercise Workflow
cancellation after the model POST reaches the public gateway boundary, even
though TASK-003 Scenario 2 assigns that proof to the named selector.

Extend the existing `public_gateway_call_context_and_errors` proof using the
current client, cancellation token, and deterministic local HTTP/mock
facilities. Establish that one model POST reached the boundary, cancel that
pending call, and prove the existing cancellation category returns promptly
without resend. Synchronization must distinguish cancellation from the call's
deadline. Do not add a production cancellation mechanism, generalized test
harness, sleep-based load test, setting, option, checker, or transport.

## Preserved behavior and non-goals

- Preserve one native model POST. Renewal prepares later calls; it never
  replays the refused call.
- Preserve the original native refusal after successful renewal when its body
  was collected, and preserve the existing body-read error when it was not.
- Preserve Python's retained shared `Workflow` owner and the R1 context journey.
- Preserve catalog-derived recognized-code messages and the approved optional
  OpenAI field projection.
- Preserve the current fallback producer/receiver contract. Do not add
  receiver-side JCS reserialization/equality enforcement.
- Preserve authenticated ingress, authorization, audit, tenant isolation,
  selected-only secret resolution, and ordinary native gateway relay.
- Do not add a dependency, public API, configuration surface, compatibility
  path, feature, check, file, setting, option, allowlist, or new test harness.
- Do not broaden this remediation into unrelated documentation or import
  cleanup outside the changed items identified by the validated ledger.

## Acceptance criteria and closure proof

| Finding | Required acceptance and direct proof |
|---|---|
| `FIND-TASK-003-2` | A native response whose status is `401` and whose body cannot be fully read triggers one renewal through the existing auth owner, sends one model POST, performs no resend, and returns the existing body-read/connection failure after successful renewal; renewal failure remains authoritative. Retain complete-body success and failure cases. |
| `FIND-TASK-003-6` | Every cited changed declaration uses a module-imported bare or role-specific aliased type. Static inspection shows no wrapper or new enforcement machinery. |
| `FIND-TASK-003-7` | Every identified new panic-capable test/helper has substantive `# Panics` documentation naming its real invariant; unrelated items are unchanged. |
| `FIND-TASK-003-8` | Rustdoc on remote create, remote cancel, and native POST states the actual cancellation and partial-progress boundary without claiming server rollback. |
| `FIND-TASK-003-9` | The existing focused public-gateway selector cancels one already-dispatched pending call, returns through cancellation rather than deadline, and observes exactly one model POST. |

Use the existing raw/local HTTP fixture patterns and extend the existing
`public_gateway_call_context_and_errors` selector rather than creating a new
transport harness. Record its exact focused command:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport \
  -E 'test(=public_gateway_call_context_and_errors)'
```

Preserve the existing remote lifecycle regression proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --test workflow_transport \
  -E 'test(=shared_workflow_client_contract)'
```

Run and record the narrowest current repository tasks that cover the changed
shared-client, Skald, server-test, Python-wrapper, formatting, lint, codegen,
and client/PyO3 boundary surfaces. The static import and documentation
corrections do not justify a bespoke check or runtime test. Follow the original
TASK-003 verification scope and current `mise.toml`; do not expand to the
repository-wide aggregate unless the repository's broad-scope trigger becomes
true because of the implementation.

## Completion and next review

Record source closure and exact implementation evidence against each stable
finding ID. Route this remediation directly to `$wyrd-implement`. The next
`$wyrd-task-review` must reassess the complete original
base-to-new-candidate range, the original task, both remediation tasks, and all
prior verdicts; it must not review only the R2 delta.
