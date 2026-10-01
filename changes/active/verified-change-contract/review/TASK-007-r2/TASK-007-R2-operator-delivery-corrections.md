---
id: TASK-007-R2
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-139, REQ-140, REQ-141, REQ-142, REQ-145, REQ-147, AC-029, AC-030, AC-031]
depends_on: []
parent_task: TASK-007
remediates: [FIND-TASK-007-1, FIND-TASK-007-9, FIND-TASK-007-10, FIND-TASK-007-11, FIND-TASK-007-12]
---

# TASK-007-R2 — Close Remaining Operator Delivery Findings

## Authority and immutable subject

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review ledger: `changes/active/verified-change-contract/review/TASK-007-r2/findings-validation.md`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Reviewed candidate: `b50ce7bc45c67f62dc7dabe34a2a1ef1ed421c34`

## Outcome

Finish the existing Operator connection and delivery implementation by making
its public key error fully selector-free, enforcing exact representable key
versions, bounding the complete rewrap pass, redacting key-owner diagnostics by
construction, and isolating provider wire mechanics. Preserve all working
security, persistence, delivery, SDK, and retry behavior.

## Diagnoses and required corrections

### `FIND-TASK-007-1` — selector-free public remediation

R1 made `OperatorKeyUnavailable` detail and internal logging selector-safe, but
the derive-backed public remediation still names the environment, file, and
Vault lookup templates. HTTP and MCP return that remediation in the complete
problem document, so the approved prohibition on exposing key locations is not
closed.

Change only the existing catalog remediation to a selector-free instruction to
restore the configured Operator key provider and retry. Preserve the error
code, status, title, constant detail, details object, and retry classification.
Extend the existing complete-problem redaction proof so generic templates and
concrete sentinels are both absent.

### `FIND-TASK-007-9` — whole-pass rewrap deadline

Rewrap now runs independently of delivery and bounds each tenant transaction,
but `rewrap_pass` starts its deadline before awaiting cross-tenant referenced
key-version discovery and applies no deadline to that SQL future. A lock wait
or stalled statement can therefore outlive the configured pass ceiling and
prevent the interval loop from starting its next pass.

Apply the remaining duration from the existing `RewrapBudget::pass` to the
discovery operation. On expiry, cancel that future, use the existing
selector-free timeout diagnostic, and return control without mutating rows.
Preserve `OperatorPool` discovery, `TenantConn`, tenant timeouts, `SKIP LOCKED`,
secret-version fencing, shutdown cancellation, per-tenant old-key reuse, and
delivery independence. Do not introduce another scheduler, cache, or budget.

### `FIND-TASK-007-10` — exact active key version

Configuration accepts every `NonZeroU32`, while persisted key versions are
`i32`. `OperatorKeys::active_version` silently maps every value above
`i32::MAX` to `i32::MAX`. Boot, new seals, metadata, and rewrap can therefore
use a different external key than the operator configured.

Reject values above `i32::MAX` in the existing `OperatorKeysConfig::validate`
boundary for both TOML and environment input. Once validated, preserve the
value exactly in `OperatorKeys` under a named invariant. Do not widen the SQL
schema or create another version type.

### `FIND-TASK-007-11` — redacted key-owner Debug

`VaultKeysConfig`, `OperatorKeysConfig`, and `OperatorKeys` derive `Debug`.
Although token bytes are masked, ordinary debug formatting still emits Vault
address, directory, mount, prefix, and token-file selectors, including through
the public `WyrdServerConfig` owner. This violates the repository's mandatory
custom-redacted `Debug` rule for secret-bearing structs.

Replace those three derived implementations with the existing repository
redaction pattern. Retain only source kind, active version, and whether Vault
configuration/client state is present; omit all selectors and token material.
Preserve cloning, deserialization, construction, and runtime behavior. Add no
wrapper or logging abstraction.

### `FIND-TASK-007-12` — provider wire isolation

The live generic delivery module directly constructs Slack and PagerDuty JSON,
parses Slack provider responses, and owns provider-specific transient codes.
This makes provider protocol changes modify the same module that owns leasing,
retry, SSRF, and credential ordering, contrary to the repository's explicit
provider-module boundary.

Keep `OperatorWorker`, `OperatorDelivery`, screened/pinned transport, response
bounds, status/retry classification, credential attachment order, endpoints,
and limits in their existing owners. Move only Slack- and PagerDuty-specific
request construction and response interpretation into focused private provider
modules. Do not add traits, a registry, a generic adapter framework, or a new
dependency.

## Constraints and preserved behavior

- Preserve providers, routes, permissions, encryption/AAD, persistence schema,
  retry ceilings, deadlines, success boundaries, and stable public error code.
- Preserve forced RLS, transactional audit, production Vault HTTPS, owner-only
  mounted secrets, boot readiness, SSRF screen/pin ordering, redaction, shutdown
  recovery, dispatch fencing, and delivery availability during rewrap.
- Preserve exact Python and TypeScript contracts, runtime OpenAPI/MCP surfaces,
  and generated-artifact ownership.
- Keep environment/file allowances limited to their approved development and
  explicit single-tenant profiles.

## Non-goals

- No new provider, public API, error type, version abstraction, SQL schema,
  scheduler, cache, secret backend, transport, or dependency.
- No redesign of Operator delivery or rewrap scheduling.
- No provider trait, registry, generic framework, or unrelated refactor.
- No change to credentialed Slack/PagerDuty smoke as release evidence.

## Acceptance criteria

| Finding | Required closure |
|---|---|
| `FIND-TASK-007-1` | The complete public problem contains no generic or concrete env/file/Vault selector or secret, while stable code/status/detail and retry behavior remain unchanged. |
| `FIND-TASK-007-9` | A stalled cross-tenant discovery returns within the existing pass budget without mutation; a later pass succeeds after the stall clears, and delivery remains independent. |
| `FIND-TASK-007-10` | `i32::MAX` is accepted and preserved exactly; `i32::MAX + 1` is rejected from TOML and environment input before boot or provider access. |
| `FIND-TASK-007-11` | Debug formatting of all three types, including transitively through server configuration, exposes no selector/token sentinel and retains only approved operational facts. |
| `FIND-TASK-007-12` | Slack/PagerDuty wire fields, response parsing, and provider-declared transient codes live only in private provider modules; existing shared transport/policy ownership and behavior remain unchanged. |

## Focused and broader proof

Add the smallest focused assertions described above: extend the existing key
redaction test; add boundary configuration and redacted-`Debug` tests; add one
deterministic Postgres stalled-discovery test; and retain existing provider
unit/journey coverage while proving wire details are confined to their private
modules. Use exact `mise exec -- cargo nextest run` selectors for every named
Rust test and the repository-managed Postgres wrapper where required.

Then run the original TASK-007 verification set for the touched owners:
SQL/shared/Wyrd families; server, CLI, MCP, Rust/Python/TypeScript journeys;
Python and TypeScript type checks; code generation; tenant/client/PyO3/unwrap
boundary checks; format, lint, and `git diff --check`. The credentialed live
provider smoke remains gated release evidence.

Route this task directly to `$wyrd-implement`.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-007-1` selector-free public remediation; code/status/detail/retry unchanged | `crates/wyrd-spec/src/error.rs` `OperatorKeyUnavailable` remediation now "Restore the configured Operator key provider…"; code, status 503, title, constant detail, empty details unchanged | `components::operators::keys::tests::key_failures_disclose_no_selector` (extended: complete problem JSON carries no `WYRD_OPERATOR_KEK`, `operator_keys`, `<dir>`, `<mount>`, `<prefix>`, `<version>`, `Vault`, `KV v2` template nor any concrete sentinel) | PASS |
| `FIND-TASK-007-9` stalled discovery bounded by the pass budget, no mutation, later pass succeeds, delivery independent | `OperatorKeys::rewrap_pass` wraps `referenced_key_versions` in `tokio::time::timeout_at(deadline)`; expiry logs the selector-free `timed_out` diagnostic (`pass_elapsed`) and returns `Ok(0)` | `pg_operator_delivery::stalled_rewrap_discovery_returns_within_the_pass_budget` (ACCESS EXCLUSIVE lock stalls discovery; 300 ms budget returns 0 with rows `[1,1,1]`; after release the next pass moves 3 → `[2,2,2]`); `pg_operator_delivery::slow_rewrap_never_holds_back_another_tenants_delivery` | PASS |
| `FIND-TASK-007-10` `i32::MAX` preserved exactly; `i32::MAX + 1` refused from TOML and env before boot | `OperatorKeysConfig::validate` refuses `active_version > i32::MAX`; `OperatorKeys` stores the validated `i32` (`active_version` field, named invariant) | `config::tests::operator_key_version_must_fit_i32`; `components::operators::keys::tests::operator_keys_debug_redacts_selectors` (asserts `active_version() == i32::MAX`) | PASS |
| `FIND-TASK-007-11` redacted `Debug` for all three types, including through `WyrdServerConfig` | Custom `Debug` on `VaultKeysConfig` (type name only), `OperatorKeysConfig` (source, active_version, vault presence), `OperatorKeys` (source, active_version, vault_client presence), `finish_non_exhaustive` per repository precedent | `config::tests::operator_keys_debug_redacts_selectors` (direct and via `WyrdServerConfig`); `components::operators::keys::tests::operator_keys_debug_redacts_selectors` | PASS |
| `FIND-TASK-007-12` Slack/PagerDuty wire isolated in private provider modules; shared transport/policy unchanged | `verification/operators/slack.rs` (`message`, `outcome`, `TRANSIENT`), `verification/operators/pager_duty.rs` (`event`); `OperatorDelivery` keeps screening, bearer attachment order, response bounds, status classification, endpoints | `verification::operators::slack::tests::slack_reply_classification`; `pg_operator_delivery::failed_verdict_fans_out_to_every_provider_independently` (in `test:wyrd`); `grep` shows no `routing_key`, `"ok"`, or Slack error code in `operators.rs` | PASS |

### Human-accepted reuse findings

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. Dispatch settlement types | `operator_dispatches` reuses `verifier_runs::{LeaseToken, RetryOutcome, Settlement, settlement}`; `DispatchRetry` deleted; `deliver`/`fail`/`release` return `Settlement`; `DispatchLease.token: LeaseToken`; server `fenced` deleted, one `claims::settled` used by runner and Operator worker | `pg_verifier_runs::dispatch_delivery_obeys_budget_deadline_and_fencing` (in `test:sql`), `test:wyrd` | PASS |
| 2. Audit-if-uncommitted copies | One `audit::record_unless_committed`; cards, operators, and verification services call it; the two helpers and the inline copy deleted | `test:wyrd`, `test:principals:integration` | PASS |
| 3. Sealing key decode | `boot::build_sealing_key` uses `components::operators::keys::decode_key` (now `pub(crate)`; trims, zeroizes); boot's `base64` import removed | `components::operators::keys::tests::decode_key_requires_32_base64_bytes`, `test:wyrd` | PASS |
| 4. MCP `structured` | One `structured` in `mcp/mod.rs`; both copies deleted | `test:bifrost:journey:mcp`, `test:wyrd` | PASS |
| 5. SDK decode helpers | Python `verification.rs` uses `operators::{decode, to_python}`; TS native `verification.rs` uses `operators::decode` (redacting: field + category, no value) | `py:test:unit`, `py:test:integration`, `py:typecheck`, `ts:test:integration`, `ts:typecheck` | PASS |
| 6. CLI `print_json` | `card::print_json` is `pub(crate)`; `operator_connection` uses it and maps to `ExitCode::SUCCESS`; copy deleted | `test:cli:journey` | PASS |
| 7. `refund_late_claim` | Inlined into `VerifierRunner::release_late` | `test:wyrd` (`pg_verification_runtime`) | PASS |
| Nit: `RewrapBudget` / `REWRAP_BATCH` | `RewrapBudget` deleted; `rewrap_pass(postgres, operator, pass_budget, tenant_budget)` takes the two `RuntimeLimits` durations; FIND-9 applies `pass_budget` to discovery; `REWRAP_BATCH` private | `test:wyrd` | PASS |

Non-goals held: no new provider, public API, error type, version type, SQL
schema, scheduler, cache, secret backend, transport, dependency, provider
trait, or registry. Stable error code/status/detail unchanged; retry ceilings,
deadlines, SSRF ordering, audit, and tenancy unchanged. No unrelated file
changed.

Focused commands (exit 0):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::operators::keys::tests::key_failures_disclose_no_selector) | test(=components::operators::keys::tests::operator_keys_debug_redacts_selectors) | test(=config::tests::operator_key_version_must_fit_i32) | test(=config::tests::operator_keys_debug_redacts_selectors) | test(=verification::operators::slack::tests::slack_reply_classification) | test(=components::operators::keys::tests::decode_key_requires_32_base64_bytes)'
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_operator_delivery -E "test(=stalled_rewrap_discovery_returns_within_the_pass_budget) | test(=slow_rewrap_never_holds_back_another_tenants_delivery)"'
```

Lanes run in this session, all exit 0: `test:sql`, `test:shared`,
`test:wyrd`, `test:wyrdstate:journey`, `test:platform:journey`,
`test:cli:journey`, `test:principals:integration`,
`test:bifrost:journey:server`, `test:bifrost:journey:mcp`,
`py:test:integration`, `py:test:unit`, `py:typecheck`,
`ts:test:integration`, `ts:typecheck`, `codegen:check`,
`check:tenant-isolation`, `check:client-tier`, `check:pyo3-scope`,
`check:unwrap-audit`, `fmt`, `py:format`, `lints`, `py:lints`,
`git diff --check`. The credentialed Slack/PagerDuty smoke remains gated
release evidence and was not run.

Status: `IMPLEMENTED`.
