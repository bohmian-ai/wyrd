---
id: TASK-002-R1
kind: remediation
status: ready
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
parent_task: TASK-002
supplemental_task: TASK-009
remediates: [FIND-002-1, FIND-002-2, FIND-002-3, FIND-009-1, FIND-009-2]
depends_on: []
---

# Preserve admitted verification scope and execution context

## Outcome and subject

A verification can run only for a subject admitted by the caller's signed Card
limit. A state using an existing authorized User token can write native
observations. Bound verification retains the exact selected occurrence and
accepted correlation context through inputs/result rows and recovery. Existing tenant-isolated single-statement hot paths remain intact. Required
Rust contracts are documented.

This is one cumulative remediation for TASK-002 and its TASK-009 supplement.
It corrects the reviewed candidate; it does not redo stable accounts or change
schema/layout decisions.

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-002`.
- Original implementation base: `036fc48fe26fb87648a277b10099bc475d293b92`.
- Reviewed candidate: `b1ff72dc34c344b596c85cf408e6a9217685be35` on
  `task/service-identity-and-telemetry/TASK-002`.
- Approved authority: [spec revision 16](../../spec.md),
  [auth](../../supplements/auth-system.md),
  [telemetry](../../supplements/telemetry-schemas.md),
  [storage/correlation](../../supplements/table-storage-and-correlation.md).
- Original tasks: [TASK-002](../../tasks/TASK-002-stable-accounts-and-auth.md)
  and [TASK-009](../../tasks/TASK-009-configured-service-context.md).
- Original verdict and independently validated findings:
  [review](../TASK-002-and-009-b1ff72dc3/verdict.md),
  [source validation](../TASK-002-and-009-b1ff72dc3/findings-validation.md).
- Revised-authority disposition: [validation](findings-validation.md),
  [verdict](verdict.md). Preserve the five FIND IDs above in evidence and later review.

Revision 16 explicitly supports Wyrd-managed configured telemetry and
application-managed standard OpenTelemetry resources plus authenticated OTLP.
No exporter accepting WyrdState, public state.client accessor, or separate
exporter identity settings are required. The former FIND-009-4 is an authority
resolution, not an implementation fix; do not implement its old API proposal.
Task003 owns the two-path final telemetry projection/readback; Task008 publishes
the tested guidance. No new finding is assigned to the maintainer's decision.

## Diagnosis and selected correction

### FIND-002-1 — Direct verification skips Card subject admission

REQ-010/012/033 require RootRequired and Scoped limits independently of operation
grants. At server `components/verification/service.rs:490`, direct execution
passes no subject to `authorize_run:932–973`. Both Card refusals require one.
Target lookup checks existence/readiness, not caller scope, and a scalar check
can run without any downstream gateway or evidence-read check. Thus a family
account with the operation grant can verify rootless, or a rooted caller can
verify an out-of-graph tenant subject. Existing enqueue already supplies the
subject to the same decision owner.

Pass the direct requested subject through that existing authorization boundary
before reservation/engine work. Preserve its single audit decision and ordinary
AnyRegistered rootless privileges. Fix HTTP and MCP through their shared control
owner; do not add route-specific or downstream compensating guards. Existing
positive or enqueue-only tests cannot prove this direct admission.

### FIND-002-2 — Native state IO refuses the approved User token flow

REQ-013 and TASK002 scenarios3/4 permit an existing User bearer with a loaded
state. The state pins a Service UID, but bearer auth returns that token unchanged
and native gRPC sends both. Bifrost `gate/auth.rs:157–184` refuses a selector
requiring resolution; server `grpc/mod.rs:360–365` mounts that Gate without the
server optional-root selection applied by the OTLP sibling. The locally safe
refusal therefore rejects a required positive flow.

Compose existing server-owned optional-root selection before native admission,
reusing `AuthenticatedPrincipal::select_root`, `wyrd-auth::select_request_root`
and the OTLP authentication composition pattern. Keep signature verification
store-free and supply only validated replacement context. Retain frame limits,
permission checks, row attribution, readiness, transport/lifecycle and ACK order.
Do not simply remove the Gate selector guard, ignore a selector, require machine
credentials for Users, introduce per-record SQL or fail the shared process.
Correct the signed-root-only native exception introduced at security posture
`:495–499` so owning prose matches the already approved behavior. Other-root,
unknown/foreign/wrong-kind selection still refuses without identity leakage.

### FIND-009-1 — Binding occurrence preservation only

Superseding maintainer disposition: retain only committed spec REQ-012/015,
INV-004 and AC-005. They require exact owner, subject occurrence and Verifier
to survive direct/submitted work, retry and recovery. The Run's selected site
must not collapse to a first subject/Verifier match before acceptance.

Preserve that occurrence through the existing request, binding resolution and
accepted-work owners. Reuse existing containing Card and occurrence identity;
no new identity, resolver, registry or activation mechanism is required. Preserve
explicit-only bindings, authorized unbound work and existing schedule/group
semantics. Prove distinct selected occurrences through direct/submitted work
and restart, with false owner/subject/Verifier combinations refused.

### FIND-009-2 — Superseded by correlation context retention

FIND-009-2 as written is rejected by the maintainer: environment must not reach
execution. Replace its engine/gateway/evaluator/population handoff requirement
with correlation context retention:

Direct and queued verification capture one context
{service_name, service_version, deployment_environment} at acceptance. Queued
work stores it with the accepted execution; both paths write it unchanged to
the result and inputs rows. Nothing else reads it.

Prove direct and queued inputs/result row readback, including queued retry and
restart, with optional tags preserved unchanged. A status-only assertion does
not prove row retention. Preserve existing authority, leases, deadlines and
evidence semantics; use the existing accepted-work and row projection owners.
TASK-003 owns final canonical columns and full warehouse readback.

### FIND-002-3 — Required changed-item rustdoc is missing

Root AGENTS makes rustdoc merge-blocking for changed Rust items, including
private/test helpers. The exact independently checked inventory is included
below. Add accurate local workflow/error/panic contracts alongside these items
and new items changed by remediation. Preserve code, assertions and error
handling. No placeholder promises, suppressions, checker or runtime test solely
for prose. Batch this with the functional corrections; do not create style rounds.

## Existing statement-tenant exception

FIND-009-3 is rejected by [focused independent validation](statement-tenant-exception-validation.md)
under the maintainer's clarification. VerificationTargets is an existing exact
exception in `scripts/check_tenant_isolation.py::STATEMENT_TENANT_OWNERS`, already
present at the original base, alongside OracleTableCuts and GatewayAdmissions.
Preserve its single request-role SECURITY INVOKER statement, transaction-local
RLS binding from verified tenant context, mismatch refusal and caller-owned
transaction lifetime. Do not convert this owner to TenantConn merely to satisfy
the rejected finding or add BEGIN/binding/COMMIT round trips. Standard tenant
work elsewhere still uses TenantConn. No new owner/exception or weakened isolation
is authorized. The existing isolation proof remains a regression boundary.

## Ordered focused correction scenarios

### 1 — Direct subject authorization

**Behavior.** Direct HTTP and MCP refuse RootRequired or out-of-scope subjects
before any engine/provider/input/result work; authorized scoped and rootless
AnyRegistered cases still succeed.

**RED.** Add the planned direct-admission HTTP/MCP cases in the existing targets
listed under proof A. Expect the current direct path to execute a forbidden subject.

**GREEN.** Enforce the subject at the shared existing decision boundary.

**REFACTOR.** Keep one authorization/audit owner for direct and submitted work.

### 2 — Native User-state admission

**Behavior.** A normal User bearer plus loaded state reaches native ACK for an
allowed subject, retaining User identity and validated root. Wrong/foreign/
unknown roots, signed mismatch, missing grant and out-of-graph subjects refuse.
Rooted machine and bare rootless generic native behavior remain intact.

**RED.** Add proof B at the real native mount; expect current unresolved-selector
refusal on the valid User-state case.

**GREEN.** Reuse server optional-root selection without weakening Gate admission.

**REFACTOR.** Retain shared selection/parsing and normal store-free verification.

### 3 — Exact occurrence through accepted work

**Behavior.** Two aliases of the same Card using the same Verifier remain distinct. Explicit-only and activated bindings work;
forged owner/site/subject/verifier combinations refuse before staging. Direct,
submitted, retry and restart preserve the selected site; unbound authorized work
remains supported.

**RED.** Extend proof C and affected exact SDK cases in proof F. Expect current
bound selection to lose the selected site.

**GREEN.** Carry and validate the existing site identity once, including all wire
projections and accepted-work consumers. Rerun proof A's admission cases.

**REFACTOR.** Remove lossy repeated binding matching; retain existing durable and
pinned Card owners, with no invented activation requirement.

### 4 — Accepted context reaches inputs/result rows

**Behavior.** Direct and queued verification capture the context at acceptance
and write it unchanged to inputs/results. Queued retry and restart preserve it.

**RED.** Add proof D asserting inputs/result row readback for present and absent
tags, including queued recovery; a status echo alone cannot close this seam.

**GREEN.** Retain context in the existing accepted execution and row projection
paths. Rerun proofs A/C where shared acceptance changed.

**REFACTOR.** Keep the existing accepted snapshot and queue lifecycle.

## Acceptance mapping and preserved boundaries

| Finding | Completion observation | Focused proof |
| --- | --- | --- |
| FIND-002-1 | Card-sensitive direct admission enforces signed subject scope before protected work; legitimate rootless privilege remains | A |
| FIND-002-2 | Existing User state writes through real native mount; all original validation/ACK/lifecycle guarantees remain | B |
| FIND-009-1 | Exact selected binding site survives request/acceptance/recovery, including explicit-only sites; no cross-root/site forgery | C and affected cases in F |
| FIND-009-2 | Direct/queued inputs/result rows retain accepted context unchanged across restart | D and affected cases in F |
| FIND-002-3 | All confirmed inventory items and new changed Rust items document real contracts | Static inventory review and existing format/lints |

All prior permission/tenant/actor/account/revocation guarantees remain. Preserve
no-new-grant, store-free signature checks, no per-record database lookup, no new
token descriptors, no latest-root inference, no provider replacement/flush,
existing typed errors and nonblocking audit. No partition/sort/Bloom/table-version
change, backfill, compatibility alias, global state, replacement graph loader,
new test binary, dependency or broad cleanup is authorized. Do not weaken tests
or manufacture RED for static/generated/already-correct obligations.

## Verification and evidence

Every new selector below is **planned**, not an assertion a test already exists.
Owners/targets/setup are inspected in current Cargo manifests and test roots.
After adding a test, confirm its exact selector with nextest list or the language
runner before RED; an empty selection is failure. Existing selectors in F are
confirmed in the original task evidence. Commands run through mise, sequentially
for Cargo-backed work in this checkout. Use existing tracing for diagnosis.

### A — Direct subject admission

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_routes::direct_execution_enforces_card_access)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::direct_execution_enforces_card_access)"'
```

### B — Native selected User context

Existing owner: `wyrd-testing/tests/bifrost/server/verification_runtime.rs`, target
server. Drive the loaded shared client state and actual native transport.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::user_state_native_io_selects_validated_root)"'
```

### C — Bound occurrence recovery

Prove only committed REQ-012/015, INV-004 and AC-005 occurrence preservation
through existing owners; no new activation or identity mechanism.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::selected_binding_occurrence_survives_acceptance_and_restart)"'
```

### D — Direct and queued correlation row readback

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::accepted_context_survives_inputs_and_results_after_restart)"'
```

### F — Focused affected public projections

Extend only the existing configured-context cases affected by the request/context
change. These are exact tests, not full SDK or capability suites. Use the current
checked-in Service/Agent fixtures and repository server setup; add no harness.
Assert exact selected binding occurrence and unchanged optional correlation tags
on inputs/results for direct and queued work, including queued restart. No
environment-dependent execution or refusal is part of these proofs.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=observe_a_run::configured_service_context_reaches_both_verification_subjects)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run py:setup && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_observe_a_run.py::test_configured_service_context_reaches_both_verification_subjects'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise run ts:build && mise run ts:build:testing && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/observe-a-run.test.ts -t "^configured service context reaches both verification subjects$"'
```

### Static contracts and narrow closure lanes

Review the rustdoc inventory and preserve the documented single-statement
exceptions; no connection-type conversion is required for those owners. Run
`mise run fmt:check`, `mise run lints`, `mise run check:deps` and
`mise run check:tenant-isolation`. If public contracts change, run the existing
`mise run codegen:regen` then `mise run codegen:check`; close Python authored
stubs/exports with `py:format:check`, `py:lints`, `py:typecheck`, and TS generated
bindings/wrappers with `ts:format:check`, `ts:lints`, `ts:typecheck`,
`ts:napi:check` when those surfaces change. Use `mise run` for each named lane.
No generator/formatter is to be simulated by hand-editing generated output.
Run only relevant existing owner regressions exposed by these corrections;
record any additional specifically named test with its exact command. No gate,
full user-journey suite or broad aggregate belongs to this remediation.

Append focused RED/GREEN results, selected counts and static evidence mapped to
FIND IDs. Keep the original candidate/base identity and original task obligations
for later cumulative task review. Completion of implementation is not PASS.

## Material stop conditions and handoff

If correction needs a new identity model, activation/persistent semantics,
auth grant, changed authority/lease/deadline/whole-trace policy or public exporter
API, stop that affected work and name the conflict. Ordinary private shape is
implementation-owned; do not ask the user to select helpers or local algorithms.
Route this task directly to `$wyrd-implement`. Do not implement, merge, commit,
push or deploy as part of this review handoff.

## Confirmed rustdoc inventory

The following inventory is the independently validated cumulative list for
FIND-002-3, reproduced here so this remediation is self-contained. Candidate line
numbers locate items; document the item even if prior fixes move its line.
D = missing rustdoc; E = missing Errors; P = missing Panics. One documentation
correction covers the list; preserve tests and assertions.

| Item | Cumulative changed status / validation | Missing contract and evidence |
| --- | --- | --- |
| `crates/shared/wyrd-auth-oidc/src/registry.rs:206` `WorkloadBindingResolver::binding` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:1169` `exchange` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:1187` `exchange_workload` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-auth/src/jwt_bearer.rs:153` `resolve_workload_binding` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:230` `PgWorkloadBindingResolver::binding` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2024` `seed_federation` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2122` `build_workload_binding` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs:472` `run_direct` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:88` `Target::resolved` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/wyrd/wyrd-testing/src/server.rs:3662` `bootstrap_machine` | CHANGED / CONFIRMED | D/E: no item rustdoc; fallible result, no Errors section |
| `crates/wyrd/wyrd-testing/src/verification.rs:176` `release_with` | CHANGED / CONFIRMED | E: fallible result, no Errors section |
| `crates/shared/wyrd-client/src/auth.rs:2063` `spawn_workload_mock` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/shared/wyrd-runtime/src/request_context.rs:268` `selector_never_rebinds_a_signed_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/gate/mod.rs:2609` `rootless_family_account_needs_a_root_to_attribute_cards` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:3857` `context_for_leasing` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3233` `native_run_id_is_replaced_by_one_authoritative_nullable_field` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3518` `scoped_service_principal` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:3697` `member_uids` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:1298` `oversized_decoded_request_is_rejected_before_wal_append` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/vala/vala-bifrost-redux/src/scribe/replay.rs:2100` `nested_accepted_subset_replays_one_fence_and_digest` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd-spec/src/auth/workload.rs:352` `workload_uid_selector_accepts_one_card_uid` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd-spec/src/error.rs:4515` `api_key_invalid_remediation_names_credential_issue` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert_eq!` |
| `crates/wyrd-spec/src/vala/audit_detail.rs:1331` `audit_detail_golden_vectors_cover_all_variants_and_nested_values` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:416` `issue_scope_error_passes_through_other_errors` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:514` `uid_of` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:527` `resolve_single_service_card_produces_own_scope` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:548` `resolve_refuses_foreign_and_unknown_roots_identically` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/card_scope.rs:586` `resolve_scope_populates_every_member_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:719` `unregistered_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1068` `api_key_exchange_issues_no_refresh_token_or_row` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1185` `seed_suspended_account_key` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1349` `cross_tenant_key_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1373` `revoked_key_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:1408` `hash_mismatch_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/issuance.rs:1366` `issue_signs_current_grants_and_follows_grant_changes` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/issuance.rs:1477` `unregistered_uid` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:863` `stored_human_connection_requires_the_sub_subject_claim` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:992` `binding_resolver_prefers_audience_then_falls_back` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/refresh.rs:1365` `a_machine_refresh_row_cannot_rotate` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:251` `a_service_account_is_revoked_under_the_service_kind` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:310` `a_mismatched_kind_is_refused_as_not_found` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/revoke.rs:338` `a_failed_revocation_write_keeps_its_cause_server_side` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-auth/src/service_accounts.rs:36` `principal_with_permissions` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:226` `add_parses_required_args` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:243` `add_requires_principal` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:258` `add_requires_subject` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/auth/workload_binding.rs:273` `add_accepts_optional_audience` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/principal/assignment.rs:236` `list_accepts_only_assignable_kinds` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-cli/src/principal/revoke.rs:106` `revoke_parses_all_kinds` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/query/investigation.rs:370` `refuse_unauthorized_or_oversized_reads` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:171` `workload_jwt_bearer_mints_service_token` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:237` `workload_jwt_bearer_falls_back_to_tenant_slug_with_service_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:291` `workload_jwt_bearer_refuses_an_agent_default_root` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:341` `wrong_audience_is_rejected_and_audited` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:398` `expired_assertion_is_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:440` `untrusted_issuer_is_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:475` `server_owned_binding_denial_returns_principal_not_found` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:512` `cloud_workload_subject_shape_uses_server_owned_binding` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:560` `same_issuer_subject_is_tenant_scoped` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:612` `stable_account_mapping_selects_only_allowed_context` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:762` `api_key_grant_still_mints` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:810` `test_state_with_external` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:892` `bootstrap_principal` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:1201` `binding` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs:1274` `card_ref` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2461` `sample_binding_entry` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/boot/mod.rs:2486` `workload_binding_rejects_a_non_https_issuer` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1474` `delete_issuer_with_live_binding_conflicts_then_cascades` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1525` `workload_binding_crud_round_trip` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/admin/routes.rs:1614` `create_binding_for_unknown_issuer_is_not_found` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/auth/principal_extractor.rs:206` `extension_hit_returns_stored_principal_without_invoking_verifier` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs:570` `mounts_token_routes` | CHANGED / CONFIRMED | D: no item rustdoc |
| `crates/wyrd/wyrd-server/src/components/auth/routes.rs:575` `extracts_tenant_from_unverified_access_token` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5385` `workload_binding_toml_round_trip` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5435` `workload_binding_entry_unknown_field_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/config.rs:5578` `binding_entry_empty_issuer_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5596` `binding_entry_empty_subject_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/config.rs:5614` `binding_entry_malformed_principal_id_rejected` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect_err(` |
| `crates/wyrd/wyrd-server/src/config.rs:5670` `bindings_without_tenant_slug_invalid` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:524` `targets_serve_only_while_listening_and_until_invalidated` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-server/src/verification/cache.rs:595` `effective_environment_follows_the_selector_rules` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs:94` `binding_card_ref` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-server/tests/integration/identity_e2e.rs:505` `workload_jwt_bearer_unbound_subject_returns_404_keycloak` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/src/queries/auth/principal_directory.rs:151` `directory_excludes_unassignable_principals` | CHANGED / CONFIRMED | P: panic expression `assert_eq!` |
| `crates/wyrd/wyrd-sql/src/queries/auth/service_accounts.rs:538` `restriction_statements_stay_tenant_implicit` | CHANGED / CONFIRMED | P: panic expression `assert!` |
| `crates/wyrd/wyrd-sql/src/queries/auth/workload_bindings.rs:299` `upsert_targets_composite_key_and_overwrites_columns` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `assert!` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:575` `tenant_admin_principal_persists_without_a_card` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_admin_principals.rs:611` `tenant_admin_principal_cannot_carry_a_workload_restriction` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:711` `human_connection_upgrade_preflight` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1681` `cloud_issuer_fk_rejects_delete_with_live_binding` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1745` `cloud_issuer_binding_maps_a_stable_account` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1889` `cloud_issuer_read_path_roundtrips_and_rls` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:2039` `cloud_issuer_crud_write_path_conflict_and_cascade` | CHANGED / CONFIRMED | D/P: no item rustdoc; panic expression `.expect(` |
| `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:499` `request` | CHANGED / CONFIRMED | P: panic expression `.expect(` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:214` `card_attributed_resource_spans` | CHANGED / CONFIRMED | P: indexes fixture resource/scope vectors at `[0]` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:227` `card_attributed_resource_logs` | CHANGED / CONFIRMED | P: indexes fixture resource/scope vectors at `[0]` |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:961` `Signal::body` | CHANGED / CONFIRMED | P: calls the indexing/panic-capable signal fixture helpers |
| `crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:979` `Signal::rejected` | CHANGED / CONFIRMED | P: calls `HttpEncoding::decode`, whose response decode uses expect |
