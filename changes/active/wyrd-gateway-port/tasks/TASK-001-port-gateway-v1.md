---
id: TASK-001
kind: implementation
status: implemented
spec: SPEC-wyrd-gateway-port
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005]
depends_on: []
parent_task:
remediates: []
---

# Port completed Gateway V1 to current Wyrd

Implement this task with `$wyrd-implement`. The approved port specification
and current repository architecture own behavior; the source branch supplies
completed implementation and tests to adapt, not an alternate authority.

## Outcome and Value

Port completed Gateway V1 from archive commit `d8907084` into the new Wyrd
branch rooted at `f7c61336`. Treat approved Gateway V1 Revision 21 and its
runtime code as the feature source. Treat current Wyrd as
the authority for identity, authorization, tenancy, audit, Bifrost, API
registration, SDK ownership, and migrations. Use old integration commit
`0f1d385b` only to understand how gateway code was adapted to admin changes;
its unresolved review cannot certify the new port. The current workflow work
is owned by the imported `changes/active/skald-workflow-runtime` packet after
this gateway task's public and in-process seams are proved.

The source commit resolves in the sibling `../wyrd-skald-workflow-runtime`
checkout, not in this new-history worktree. Read source files with
`git -C ../wyrd-skald-workflow-runtime show d8907084:<path>` and compare them
against this worktree; do not merge or cherry-pick its historical branch.

## Owners, Scope, Consumers, and Prohibited Changes

`wyrd-spec` owns typed contracts; current SQL and auth owners own durable
tenancy and permission decisions; `wyrd-gateway` owns governed invocation;
`wyrd-server` owns the public edge and audit; Skald owns provider transports;
`wyrd-client` and first-class SDKs project the server contract. CLI and MCP
own their respective administration surfaces. Preserve the approved spec's
single in-process gateway, tenant-scoped capture, and workflow separation.
Do not port archive admin/auth state, stale migrations, compatibility routes,
or workflow execution.

## Approach

1. **Inventory before copying.** Compare the pinned runtime branch with the
   new tree by owner and public surface. Build a source-to-target ledger for
   gateway contracts, SQL, admin, credentials, engine, Skald providers,
   inference routes, accounting, capture, SDKs, CLI, MCP, schemas, docs, and
   tests. Mark existing equivalents to reuse, source-only behavior to port,
   and archive assumptions to replace. Exclude unrelated runtime merge commits
   and the old integration's archive admin changes. Record every intentionally
   omitted source file/behavior and its reason.
2. **Contract and durable foundations.** Port typed gateway wire shapes and
   stable errors into `wyrd-spec`; add current-history SQL migrations and
   tenant-scoped storage for deployment/configuration, credential envelopes,
   admission/accounting, and batch state. Adapt admin handlers to current
   principal, `TenantConn`, RBAC, and canonical audit. Reuse existing crypt,
   storage, queue, and Bifrost capabilities. Do not copy old migration numbers:
   runtime uses `20260601000020`–`22` and the old integration uses `26`–`28`.
   Inspect the target's current migration ledger immediately before numbering
   new migrations; do not edit already-applied SQL or reserve a stale number.
3. **One governed inference path.** Port the in-process gateway engine and
   Skald provider/adapter changes. Wire OpenAI-compatible, Anthropic Messages,
   and Gemini GenerateContent ingress through current `wyrd-server` route and
   OpenAPI registration. Preserve operation coverage, native passthrough,
   translation, capability refusal, screened/pinned endpoints, bounded retry
   and fallback, streaming termination, cancellation, and shutdown. Pass
   verified caller context into the engine; the engine never accepts tenant
   authority from request bytes or opens Wyrd Postgres directly.
4. **Governance and capture.** Restore quota/budget/admission, durable usage
   ledger, audit decisions, redacted managed-secret lifecycle, and optional
   Bifrost call/attempt capture. Use the current audit staging/publisher.
   Keep capture off the caller's response path and give its writer only the
   tenant and exact destination scope. Preserve replay and restart behavior.
5. **Gateway consumers and journeys.** Project the same public contracts through
   `wyrd-client`, Rust/Python/TypeScript SDKs, CLI, MCP, served OpenAPI,
   generated types, examples, and docs. Keep provider-credential mutation out
   of first-class SDKs as Revision 21 requires; CLI and scoped MCP own it.
   Restore the old credential-free gateway journey, updating fixtures for
   current auth, migrations, and server setup. Finish the gateway port on the
   new Wyrd main line without depending on the verification branch.

The steps are dependency order, not separate releases or permission to leave
partial public capability on the target branch. Use the smallest cohesive
code from the completed implementation; avoid copying stale scaffolding,
historical compatibility, or already-present behavior.

## Ordered Implementation Scenarios

### Scenario 1 — tenant setup and secrets

**Behavior.** A tenant admin submits a managed key, configures a deployment,
reads only redacted metadata, rotates the key, and survives server restart.
Under-privileged and cross-tenant setup fails closed. Covers REQ-001, 003,
004 and AC-001, 002, 004.

**RED.** Port the source real-server administration journey and add a focused
real-server case that submits, rotates, restarts, and invokes with a managed
secret. The case belongs in the restored `wyrd-testing --test gateway` target;
after adding it, run its exact focused command and record the missing-route or
missing-contract failure:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::managed_secret_rotation_survives_server_restart)"'
```

**GREEN.** Add the minimum typed contracts, migrations, admin handlers,
credential protection, authorization, and audit to pass. Rerun the same test.

**REFACTOR.** Reuse current auth/SQL/crypt owners and remove archive-specific
copies while keeping the journey green.

### Scenario 2 — governed provider call

**Behavior.** An ordinary caller exchanges a Wyrd credential, invokes an
allowed model through an unmodified provider client, and upstream receives
only the configured provider key. Denied callers and wrong tenants never reach
upstream. Covers REQ-001–003, 005 and AC-001–003.

**RED.** Port the source's real-server mock-upstream invocation journey. It
fails at missing ingress or dispatch. The source target and selector are
`wyrd-testing --test gateway` and
`compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage`:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage)"'
```

**GREEN.** Connect current token extraction and scoped authorization to the
one in-process engine, deployment resolution, Skald provider call, accounting,
and correct ingress response. Rerun scenarios 1–2.

**REFACTOR.** Collapse duplicated dialect and provider paths into existing
typed owners without changing their public behavior.

### Scenario 3 — complete operation and failure surface

**Behavior.** Chat, Responses, Embeddings, Images, Audio, Batches, Anthropic
Messages, and Gemini GenerateContent preserve supported native/translated
semantics. Unsupported capability, unsafe endpoint, ambiguous token,
truncated stream, repeated batch, and fallback across denied scope fail as
specified. Covers REQ-001–005 and AC-002–003.

**RED.** Restore the source's focused protocol and resilience journeys before
the corresponding adapter/route code. Start with the source operation target
`operations::responses_audio_and_batches_dispatch_through_the_public_server`:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=operations::responses_audio_and_batches_dispatch_through_the_public_server)"'
```

Record that RED failure. Restore `mise run test:gateway:native` for the full
operation, native ingress, and resilience family rather than treating the
one focused result as complete proof.

**GREEN.** Port only the provider and route behavior those journeys require.
Keep bounded input, retry, streaming, and idempotency invariants intact.

**REFACTOR.** Remove adapter duplication and stale source paths with all
operation journeys still passing.

### Scenario 4 — attributable evidence and cross-surface clients

**Behavior.** Every permission decision is canonically audited; admitted
calls settle accounting; optional call/attempt capture is tenant-scoped,
redacted, durable as specified, and nonblocking under outage/backpressure.
Rust, Python, TypeScript, CLI, MCP, and provider HTTP clients see the same
contract. Covers REQ-001, 006–007 and AC-004–005.

**RED.** Restore the source's audit, capture, outage, and first-class-language
journeys. Start with the source's focused shutdown audit case, which fails on
missing gateway invocation and audit ownership:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=resilience::a_pending_invocation_audit_append_drains_before_shutdown_completes)"'
```

Use the restored `mise run test:gateway:journey` as the combined proof across
all public surfaces, including Rust, Python, TypeScript, CLI, MCP, and HTTP.

**GREEN.** Adapt capture to current Bifrost and scoped capture authority;
port only necessary client wrappers, generated types, CLI/MCP surfaces, and
served OpenAPI registrations. Rerun all prior scenarios.

**REFACTOR.** Delete parallel client transport or audit logic and keep the
combined journey green.

## Acceptance Criteria

- Gateway AC-001–005 and INV-001–005 in `spec.md` have source-to-target ledger
  entry and passing proof, including negative behavior. Missing source tests
  require an equivalent current-repo journey, not a unit-test substitution.
- The real-server managed-secret journey proves rotation and invocation after
  restart as one user path, complementing the source CLI rotation and Postgres
  envelope tests.

## Expected Write Set and Consumer Closure

Likely owners and consumers are `crates/wyrd-spec`, current SQL migrations,
`crates/wyrd/wyrd-gateway`, `crates/skald/skald-providers`, `crates/wyrd/wyrd-server`,
`crates/shared/wyrd-client`, the three SDK packages, CLI, MCP, examples, docs,
generated contracts, `mise.toml`, and their gateway journeys. This is a
consumer checklist, not a file allowlist: the source-to-target ledger decides
which already-present behavior can be reused and which source behavior must
be ported.

## Verification and Evidence

- Restore `mise run test:gateway:journey` as the capability gate, covering
  Rust, Python, TypeScript, CLI, MCP, native ingress, mock upstream, and local
  Vault. Keep `mise run test:gateway:smoke:live` opt-in; no provider key is
  needed for the required lane.
- Run `mise run fmt`, `mise run lints`, `mise run py:format`,
  `mise run py:lints`, `mise run py:typecheck`, `mise run codegen:check`,
  `mise run check:client-tier`, `mise run check:pyo3-scope`, and the narrow
  current SQL, principals/OpenAPI, Skald, and MCP tests touched by the port.
  Use `mise run gate` if no complete gateway capability gate exists for the
  final cross-boundary candidate, per `AGENTS.md` §11.
- Confirm the actual served `/openapi.json`, credential carrier errors,
  redaction, tenant isolation, canonical audit publication, and capture writer
  scope against the current server. Record exact package/target/test selectors
  for every specifically named Rust test in the implementation report.
- Rebase against the then-current new Wyrd main and run the gateway journeys
  on the resulting candidate. Verification integration is separate work.
- Final diff audit: source behavior accounted for; no old archive admin,
  legacy migration, duplicate transport, or provider secret disclosure. Record
  any source behavior deliberately removed
  because current Wyrd architecture supersedes it.

## Material Stop Conditions

Stop and request a spec revision if current Wyrd cannot support a source
public contract without changing its wire shape; if capture requires broader
authority than the proposed tenant/destination scope; if principal, audit,
SSRF, secret, tenant, or data-loss behavior would weaken; or if a migration
requires rewriting applied history. Ordinary API renames, fixture updates,
code structure, and fresh migration numbering are implementation choices.

## Authority Links

- Approved port spec: `changes/active/wyrd-gateway-port/spec.md` Revision 1.
- Current rules: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
  `architecture/bifrost-design.md`, and
  `architecture/references/languages/spec-driven-development.md`.
- Historical behavior/evidence: both approved specifications at the pinned
  archive commit and their paths in the local spec.

## Implementation Evidence

Candidate: `452a028d0` (port), `f70b30e3c` (restart journey), `7c2b7a3aa`
(Scribe merge-window fix found by the Python journey), plus this evidence
commit. Base `f7c61336a` is current `main`; no rebase was required.

### Source-to-target ledger

| Source (`d8907084` via `0f1d385b` adaptation) | Target | Disposition |
|---|---|---|
| Gateway wire types, stable errors, schemas | `crates/wyrd-spec/src/gateway/*` | Ported |
| In-process engine | `crates/wyrd/wyrd-gateway` | Ported |
| Admin/invocation/ingress/ledger/capture/batch/multipart handlers | `crates/wyrd/wyrd-server/src/components/gateway/*`, `mcp/gateway.rs` | Ported onto current principal, `TenantConn`, RBAC, canonical audit, runtime OpenAPI registration |
| Runtime migrations `20260601000020`–`22`, integration `26`–`28` | `wyrd-sql/migrations/20260924000000`–`02`, `vala-sql/migrations/20260924000000_forge_gateway_namespace.sql` | Renumbered fresh; no applied SQL edited |
| `20260916000000_audit_staging_system_principal` and `system` principal kind | — | Superseded: capture principal is a card-free `service`; main's `20260910000026` already widened kinds |
| Checked-in `openapi.yaml`, `docs/api/openapi.md` | served `GET /openapi.json` | Superseded by runtime OpenAPI |
| `invocation_tests.rs`, `tests.rs` | `pg_invocation_tests.rs`, `pg_administration_tests.rs` | Renamed to current Postgres-test naming |
| Skald provider/adapter changes | `skald-providers`, `skald-spec`; `skald-runtime` workflow tests get wire-type consumer fixes only | Ported; no workflow execution (INV-005) |
| Capture identity (auth issue/verify) | `wyrd-auth-issue`, `wyrd-auth-verify`, `vala-bifrost-redux` gate reservation, `tables/gateway/calls.rs` | Adapted: tenant-bound service, ≤900 s token, exactly two record-write grants (`vala.gateway.calls`, `vala.traces.spans`); refresh tokens refused |
| Client/SDK/CLI | `wyrd-client/gateway*.rs`, `sdks/wyrd-sdk-{rust,python,ts}` gateway, `wyrd-cli gateway.rs` | Ported; credential mutation stays in CLI/MCP |
| Python test-server wrapper `wyrd-testing/src/python.rs` | `sdks/wyrd-sdk-python/src/testing.rs` | Relocated per PyO3 ownership rule |
| Real-server journeys | `wyrd-testing/tests/gateway/{compatible,native,operations,resilience,vault}.rs`, Python/TS gateway integration tests | Ported; added `compatible::managed_secret_rotation_survives_server_restart` |
| `changes/`, `.agents`, `.claude`, `CONTRIBUTING.md`, AI co-author trailer text in `AGENTS.md` | — | Omitted: not gateway |
| `workflow_gateway` examples | — | Pre-existing, owned by `skald-workflow-runtime` |

### Acceptance

| Criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| AC-001 managed secret, deployment, narrow caller, unmodified SDK, redacted reads | admin handlers, `wyrd-crypt` keyring envelopes, ingress | `compatible::managed_secret_rotation_survives_server_restart`; Python `test_gateway_admin_journey`, `test_openai_client_reaches_every_backend_through_the_server`, `test_two_users_have_distinct_model_access_and_traceable_usage` | PASS |
| AC-002 fail-closed negatives | RBAC, tenant scoping, token extraction, endpoint screening, admission | restart journey (reader 403, cross-tenant 404/4xx); `test_gateway_admin_requires_gateway_permissions`; `test_openai_client_receives_stable_refusals_without_dispatch_or_leakage`; native `*_is_native_governed_and_fail_closed`; `resilience::a_retryable_refusal_fails_over_and_a_limit_refuses_before_dispatch`; vault refusal tests | PASS |
| AC-003 operations, native ingress, streams, batches, retry/fallback | engine + Skald adapters | `mise run test:gateway:native`; `operations::responses_audio_and_batches_dispatch_through_the_public_server`; `compatible::openai_compatible_streams_terminate_and_survive_a_capture_outage` | PASS |
| AC-004 ledger, audit, capture; restart/rotation/replay/drain/outage | ledger, canonical audit append, capture writer | `resilience::a_pending_invocation_audit_append_drains_before_shutdown_completes`, `resilience::a_cancelled_call_is_still_settled_and_the_server_keeps_serving`, restart journey, `test_embedding_and_image_evidence_reaches_bifrost_and_storage`, vault rotation | PASS |
| AC-005 Rust/Python/TS/HTTP/CLI/MCP, served contract | client + SDKs + CLI + MCP | `mise run test:gateway:journey` (Rust 8, Vault 2, CLI 1, MCP 1, Rust SDK 1, Python 8, TS 2); `mise run codegen:check`; `mise run test:principals:integration` | PASS |
| INV-001 single in-process gateway | `wyrd-gateway` engine inside `wyrd-server` | journeys use only `WyrdTestServer`; no listener/route alias added | PASS |
| INV-002 no caller-selected key / plaintext secret | redacted views, sealed envelopes | restart journey redaction and upstream-key assertions; refusal test asserts no leakage | PASS |
| INV-003 capture scope | capture principal shape + gate reservation | `wyrd-auth-issue`: `issue_gateway_capture_access_token_is_a_card_free_service_with_only_the_capture_role`, `issue_refresh_token_refuses_the_reserved_capture_identity`; `wyrd-auth-verify`: `reserved_capture_identity_verifies_only_in_its_exact_shape`; `vala-bifrost-redux`: `gate_reserves_the_gateway_call_table_to_the_capture_principal` | PASS |
| INV-004 no historical merge | patch built from `git diff 41e60be6 0f1d385b` minus archive admin/auth/migrations | ledger above; fresh migration numbers | PASS |
| INV-005 no workflow execution | Skald wire-type consumer fixes only | diff audit | PASS |

### Commands

- `mise run test:gateway:journey`, `mise run test:gateway:native` — pass.
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=compatible::managed_secret_rotation_survives_server_restart)"'` — pass.
- `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::persistence::tests::merge_window_bytes_is_bounded_by_the_rows_a_claim_holds)'` — pass (fails under the previous full-window-per-member formula).
- `mise run fmt`, `lints`, `py:format`, `py:lints`, `py:typecheck`, `codegen:check`, `check:client-tier`, `check:pyo3-scope`, `check:unwrap-audit`, `docs:check` — pass.
- `mise run test:principals:integration` (15 + 17, includes served `/openapi.json` contract), `mise run test:sql` (119 + 2), `mise run test:skald` (421) — pass.
- `scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --lib --run-ignored=all -E "test(/mcp::/) | test(/components::gateway::/)"'` — 58 pass (MCP catalog/tools and gateway administration/invocation Postgres tests).
- `git diff --check` — clean; tracked/untracked audit: only this task's write set plus the untouched `changes/active/skald-workflow-runtime` edits owned elsewhere.

### Limits

- RED was not observable for ported tests: code and tests were applied together
  from the source patch. The new restart journey first failed on a harness
  issue (peer principal re-provisioned on restart, fixed by
  `WyrdTestServer::restart_bound` reusing peer credentials), not a missing
  gateway feature.
- `mise run test:gateway:smoke:live` is opt-in and was not run.
- The Python journey exposed a Scribe merge-workspace over-reservation
  (3.1 GB requested against a 2.6 GB ceiling); fixed in `7c2b7a3aa`.
