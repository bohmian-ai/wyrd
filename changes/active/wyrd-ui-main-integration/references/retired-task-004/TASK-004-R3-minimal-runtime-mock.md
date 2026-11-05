---
id: TASK-004-R3
kind: remediation
status: superseded
spec: SPEC-wyrd-ui-main-integration
spec_revision: 17
requirements: [REQ-037, AC-020]
depends_on: [TASK-003]
parent_task: TASK-004-R2
remediates: []
---

> Retired on 2026-11-02 by the caller-directed revision 18 replacement.
> Historical instructions and evidence only; do not execute this task.
> Current owners: [Task 01](../../tasks/TASK-004-revision/TASK-01-mockserver.md)
> and [Task 02](../../tasks/TASK-004-revision/TASK-02-live-observe-workflow.md).
> No historical implementation claim becomes review PASS or visual acceptance.


# TASK-004-R3 — Reduce Stage 1 to a runtime mock provider

## Outcome and Value

Correct the rejected Stage 1 implementation. A client runtime scope causes the
existing gateway to call a mock provider instead of an external provider.
Everything around provider dispatch retains its ordinary behavior. No new table
column, workflow state, routing rule or accounting policy is part of this task.

```python
with wyrd.mock("support-desk"):
    await desk.handle(ticket)  # existing OpenAI client and Cards unchanged
```

The gateway call made inside this scope uses the mock. A concurrent call outside
it to the same tenant, deployment and model uses the ordinary provider. The
attempt provider is `mock` because that is what answered. Its requested/resolved
model address still describes the original target.

This is the only correction assignment for R2 Stage 1. Revision 17 is draft;
execute after approval. Do not execute the conflicting revision-16 Stage 1
instructions or its queued-judge/provenance proof. This task does not authorize
Stages 2–5, a commit, a push, staging files or data deletion.

## What Changes and What Must Stay True

### Keep

- Runtime client helpers, request-local scope and the gateway mock header.
- The unchanged authenticated OpenAI construction, including
  `DefaultAsyncHttpxClient(auth=self.state.http_auth)`.
- `[gateway] mocks` fixture configuration and the stateless matcher using
  existing validated Chat Completions responses. Example names belong in fixture
  data only. Initial support remains buffered OpenAI-compatible chat.
- Mock dispatch at the existing provider boundary, ordinary response/error
  types, normal attempt-provider identity and the existing nonbillable result
  mechanism. Mock responses do not produce actual vendor charges.
- Existing authentication, permissions, credential resolution, normal routing,
  admission, retries/fallback, limits, timeouts, capture and settlement behavior.

### Remove exactly these additions

| Rejected addition | Required correction |
|---|---|
| `mock_fixture` in `bifrost.gateway.calls` | Remove its field/constant/whole-struct registration and schema expectations. Restore the ordinary calls schema exactly; no dataset reset or schema-evolution project. |
| `mock_fixture` in `GatewayCallPayloadV1` and `GatewayAttemptSpanFieldsV1` | Remove those new contract fields, their construction and projections. Use the existing provider field, not a replacement provenance contract. |
| Mock attempts named `openai`/`openai_compatible` | Set the existing attempt provider to `mock` consistently for live and captured attempts. Keep real attempts unchanged and requested/resolved model/deployment addresses unchanged. |
| Fixture-dependent `retain_representable` filtering in `routing.rs` | Remove only the mock-dependent filter and arguments. Keep ordinary capability/body-representability filtering and original candidate order. Do not preselect a fallback because it has a fixture. |
| `verifier_executions.gateway_mock`, snapshot serialization/claim fields and changed execution-key semantics | Remove mock-only persistence/query/staging/claim/idempotency changes and their callers/tests. Restore verification behavior, not a different snapshot implementation. |
| `20261027000000_verification_gateway_mock.sql` | Remove the rejected new migration from the proposed source. Do not edit any already-applied migration or wipe a database. If it was applied outside disposable local testing, report that fact and stop only cleanup of that database; do not invent rollback SQL. |
| Mock-specific verification HTTP/MCP/control/runner/hosted-caller changes | Remove them and restore existing verification behavior. No mock inheritance or refusal feature is added to verification, grouping or scheduling. |
| Additional mock-specific ledger/admission rules | Remove those policy changes. Express the mock result with the existing nonbillable attempt/result mechanism and let unchanged ledger/admission owners handle it. |
| Fixture snapshots/digests used only by removed provenance or verification storage | Delete orphaned types/exports/helpers/tests. Do not replace them with another registry, version store or metadata field. |

Ordinary optional span attributes may retain a fixture label if already useful;
no field, table, digest or new observability mechanism is required for it.
Only request selection and provider substitution are new behavior. Existing
native/transport code may retain the minimum header bridge used by gateway
requests; it must not become a verification workflow feature.

### Preserve source and data

The baseline is `HEAD 930a2de96` plus the existing uncommitted implementation
and unrelated UI work. The earlier `d87d24882` and `930a2de96` commits contain
both reusable code and the rejected deployment design. Do not revert whole
commits or restore the whole tree to HEAD: HEAD still contains the old design.
Make a reviewed forward correction. Preserve R1/UI changes, the canonical plan,
normal provider regressions and unrelated fixes. Do not reintroduce
`ProviderAdapter::Mock`, mock deployment admin DTOs or the mixed-plan ban.
Record the starting source identity and distinguish pre-existing edits before
changing anything. No destructive git command is authorized.

### Explicit boundary

This task mocks gateway calls carrying the selection. It does not mock a judge
that runs separately on the server without that selection. Do not extend Runs,
verification APIs, MCP, grouping, schedulers or queue persistence to close that
limit. R2 Stage 2's no-external-provider requirement remains a separate planning
constraint; do not claim the old queued-judge proof still applies, start Stage 2
or silently run paid judges as part of this correction.

## Approach

1. Inventory mock-specific changes against HEAD and the two Stage 1 commits;
   protect unrelated edits. Identify all consumers of fields being removed.
2. Remove the rejected schema, migration and verification-workflow additions;
   close their contracts, bindings, fixtures and tests without replacing them.
3. Restore normal routing/admission/accounting, retaining only request selection
   and mock provider dispatch through existing result/error mechanisms.
4. Prove unchanged-client scope, same-target real/mock concurrency, provider
   identity, no-network refusals and ordinary routing/credential behavior.
5. Correct doctrine/generated projections and the rejected R2 stage record;
   record focused results and stop before Stage 2.

## Ordered Implementation Scenarios

### Scenario 1 — The ordinary gateway dispatches to the selected provider

**Behavior.** With the same tenant/key/deployment/model, a scoped request gets
its configured mock response; an unscoped request reaches the real provider.
Authentication and credentials are checked normally. Mock attempts record
provider `mock`, use existing nonbillable results and never contact the external
provider. Invalid/unknown/unmatched/unsupported selections fail explicitly.
A primary with no fixture must not disappear from the route because a fallback
has a matching fixture. Ordinary route order and fallback rules still apply;
all attempts in the scoped call remain mock dispatches.

**RED.** Adapt C1/C2/C4 and the public journey C6; add planned C3. Confirm that
old real-provider identity, new schema reliance and fixture-based route pruning
fail the new assertions. Use the normal stable invalid-request behavior for
fixture refusal; no preflight route rewrite may be used to make it pass.

**GREEN.** Remove mock-dependent route filtering, use the existing provider
result/error and nonbillable mechanisms, and set actual mock attempt identity.
Run C1–C6 plus C9. No verifier, queue, schema or ledger policy is added.

**REFACTOR.** Delete orphaned snapshot/provenance/adapter machinery. Keep the
existing dispatcher, matcher, protocol validation, routing and capture owners.

### Scenario 2 — Existing clients scope only their gateway calls

**Behavior.** Python/TypeScript/Rust scope helpers retain request-local
selection. Existing long-lived authenticated OpenAI clients work unchanged;
concurrent real/mock requests remain isolated and nested/exception exit restores
normal dispatch. Each request retains its captured selection through ordinary
retries. No global client/header mutation, Card edit or deployment change is
needed. No test claims that autonomous verification inherits this scope.

**RED.** Replace the workflow-heavy Python/TS journeys with C7/C8's focused
provider journeys, keeping genuine language-lifetime scope assertions. Extend
C6 to use the public Rust scope. Prove the tests select gateway substitution,
not persisted verification snapshots or a second provider endpoint.

**GREEN.** Retain only the scope/header bridge needed by gateway callers and
remove verification-specific native/export support that is now orphaned. Run
C6–C8 and affected existing scope unit assertions using exact recorded selectors.

**REFACTOR.** Reuse existing auth hooks and language-local context support;
remove duplicate configuration and unused generated/native surfaces.

## Acceptance Criteria

| Obligation | Required proof and rejection condition |
|---|---|
| REQ-037; AC-020: runtime substitution | C1/C2/C6–C8 show same-target scoped/mock and unscoped/real calls, normal auth and credentials, restored scopes, mock attempt identity and no external mock dispatch. A changed Card/deployment/client block or silent real fallback fails. |
| Ordinary routing and gateway behavior | C3/C9 preserve candidate order, normal filtering/fallback and credential checks; fixtures must not select the route. New mock admission/ledger policies fail. |
| No storage/workflow expansion | C5 and static diff audit show unchanged calls schema/payloads and removal of the snapshot migration, execution-key changes and verification plumbing. Any required dataset recreation or new schema field fails. |
| Evidence and completion honesty | Preserve old Stage 1 results as historical source-bound evidence, explicitly withdrawn for current acceptance. Record actual new checks; a red check cannot be marked PASS because a future commit might clear it. Stop before Stage 2. |

## Implementation Reference

- **Dispatch/accounting:** `wyrd-gateway/src/adapter/mock.rs`,
  `adapter/http.rs`, `engine.rs::AttemptRecord`. Reuse stateless response matching
  and the existing provider-result/nonbillable mechanism. Both live provider
  spans and captured AttemptRecord currently name the configured real adapter;
  correct those assignments to the answering mock provider.
- **Routing:** `routing.rs::retain_representable` and
  `wyrd-server/src/components/gateway/invocation.rs` invoke fixture-dependent
  filtering before admission. Remove that branch/argument, preserving normal
  capability and representability checks. Existing rejection mapping at mock
  dispatch must still produce the required stable refusal after removal.
- **Evidence:** `wyrd-spec/src/gateway/record.rs`,
  `vala-bifrost-redux/src/tables/gateway/calls.rs`, server gateway `capture.rs`
  and `invocation.rs`. Provider is already emitted as `gen_ai.provider.name`;
  requested/resolved model columns already describe the target. Remove the
  added payload/table fields and adapt readers/builders/journey assertions.
- **Verification removal closure:** `verification/submissions.rs`,
  `verification/runner.rs`, `verification/hosted/blocks.rs`,
  `components/verification/{routes,service}.rs`, `mcp/verification.rs`,
  gateway `caller.rs`, SQL `queries/verifier_executions.rs` and the new migration.
  Remove mock-specific changes only; preserve ordinary identity, authorization,
  persistence, queue and group semantics.
- **SDKs:** Python `_mock.py`, `client/_http_auth.py`; TypeScript scope/auth
  hooks in `wyrd/src/index.ts`; shared client `transport/{trace,http}.rs` and
  language native bridges. Existing hooks supply runtime scope. Remove any
  verification-only fixtures/testing projections/native additions; retain only
  the necessary gateway surface and matching exports/stubs.
- **Consumers:** Rust `compatible.rs` journey currently selects `mock_fixture`
  and expects real provider names. Python `test_client_http_auth.py` and TS
  `runtime-mock.test.ts` currently include direct/queued judge fixtures. Change
  assertions to this provider-only contract; remove only tests of rejected
  behavior, preserving normal verification regression coverage.
- **Doctrine:** owning gateway section of `architecture/wyrd-design.md`,
  credential section of `architecture/wyrd-security-posture.md`, R2 Stage 1
  instructions and verification reference must agree on the narrow contract.
  Regenerate derived outputs from source and author Python stubs as needed.

## Verification and Evidence

Source inspection confirmed the packages/modules below. C1/C2/C4–C6/C9 exist;
C3/C7/C8 are planned selectors in existing test owners. Confirm exact planned
selectors after writing the test, before RED. Use repository native/package
setup from `mise.toml` and the verification reference; do not run a full suite
of 1,093 tests or `gate` merely to certify this bounded correction.

**C1 — matcher/protocol/no network:**
```bash
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=adapter::tests::mock_chat_fixtures_are_stateless_and_do_not_call_upstream)'
```
**C2 — ordinary credentials and existing nonbillable attempt:**
```bash
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=engine::tests::mocked_attempts_resolve_credentials_but_are_never_billable)'
```
**C3 — fixtures do not change route order (planned; replaces the opposite assertion):**
```bash
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=routing::tests::mock_scope_does_not_filter_or_reorder_candidates)'
```
**C4 — ordinary captured provider identity (extend with mock):**
```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::capture::tests::attempt_spans_name_the_admitted_adapter_provider)'
```
**C5 — calls schema exactly matches the original contract:**
```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=tables::gateway::calls::tests::calls_table_is_the_payload_contract_plus_the_managed_envelope)'
```
C5 must compare against the pre-feature schema, not pass because both the
payload and table still contain the same new field. Inspect the schema diff
against HEAD as part of static proof.

**C6 — public Rust SDK/HTTP gateway journey:**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::request_scoped_mock_preserves_real_deployment_and_auth)"'
```
**C7 — unchanged Python OpenAI client (planned):**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/gateway/test_client_http_auth.py::test_runtime_mock_only_replaces_gateway_provider'
```
**C8 — TypeScript scope/stock client (planned in current integration owner):**
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && pnpm --dir sdks/wyrd-sdk-ts/wyrd exec vitest run tests/integration/runtime-mock.test.ts -t "^runtime mock only replaces the gateway provider$"'
```
**C9 — unchanged real-provider/routing regressions:**
```bash
mise exec -- cargo nextest run --locked -p wyrd-gateway --lib -E 'test(=engine::tests::idempotent_failures_fall_back_and_one_failure_keeps_plan_order) | test(=routing::tests::fallback_precedence_is_first_defined_without_merging) | test(=routing::tests::fallback_keeps_only_capable_and_representable_candidates) | test(=engine::tests::unsafe_failures_never_retry_and_credentials_fail_closed)'
```

**Static removal proof (no manufactured RED):** record a final source diff
showing no new calls schema/record fields, no mock execution-key/snapshot
contract, no snapshot migration in the proposed change, no verification workflow
changes or new ledger policy. Account for all previously changed consumers,
exports, fixtures and generated outputs. Removed behavior is the caller's
explicit scope rejection; do not weaken unrelated tests or add ignores.

Run format/lint/type checks for touched owners and `codegen:check` when affected.
For native declaration changes, run `ts:napi:check` and report its actual status.
Its `git diff` compares against the index; it does not intrinsically require a
commit. Do not stage or commit without authorization, falsify PASS, or change the
check to hide drift. Record generated-file reproducibility separately if the
check remains red solely because the index differs. Finish with `git diff --check`.

## Material Stop Conditions

Revision 17 approval is required before implementation. A migration already
applied to non-disposable data blocks only database cleanup; preserve the data
and report it. If the existing provider result cannot express mock/nonbillable
behavior without a new ledger policy, report the exact owner constraint rather
than broaden this task. Do not solve Stage 2 judge automation here.

## Authority Links

- [Spec revision 17 draft](../../spec.md), especially REQ-037 and AC-020.
- [R2 integrated task](TASK-004-R2-live-dev-workbench.md), later stages only.
- [Plan](../../plan.md), [verification setup](../verification.md).
- [Execution](../../../../implementation-execution.md), [testing](../../../../../TESTING.md).
- [Wyrd gateway authority](../../../../../architecture/wyrd-design.md),
  [security](../../../../../architecture/wyrd-security-posture.md),
  [Bifrost](../../../../../architecture/bifrost-design.md).

## Planning Evidence

Read-only source review located existing provider identity, the added schema
closure, fixture-dependent routing and rejected verification persistence.
Selectors above were read from source, not run during planning. This document
records no implementation PASS and no approval of existing Stage 1 evidence.

Independent comprehension read revision 17, this task and R2's supersession
banner together. It found no material contradiction blocking the bounded R3
proposal; keep/remove scope, source/data preservation, proof and approval are
explicit. Stage 2 autonomous-judge mocking remains separately unresolved, not
part of this task. Static packet checks and `git diff --check` passed. No
runtime code, commit, migration or database was changed during this planning.
