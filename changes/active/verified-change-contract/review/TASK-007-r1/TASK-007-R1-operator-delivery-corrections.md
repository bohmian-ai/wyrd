---
id: TASK-007-R1
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 35
requirements: [REQ-139, REQ-142, REQ-146, REQ-147, REQ-148, REQ-149, REQ-150, INV-007, AC-029, AC-030, AC-031]
depends_on: []
parent_task: TASK-007
remediates: [FIND-TASK-007-1, FIND-TASK-007-2, FIND-TASK-007-3, FIND-TASK-007-4, FIND-TASK-007-5, FIND-TASK-007-6, FIND-TASK-007-7, FIND-TASK-007-8, FIND-TASK-007-9]
---

# TASK-007-R1 — Close Operator Connection and Delivery Findings

## Authority and immutable subject

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Review ledger: `changes/active/verified-change-contract/review/TASK-007-r1/findings-validation.md`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Reviewed candidate: `2bd4ded8f212f7885b0dd78bcd450fa0ff118f3e`

## Outcome

Make the existing Operator-connection and delivery implementation satisfy its
approved production security, availability, typing, and repository boundaries
without changing its public product contract. Preserve the working encrypted
storage, provider protocols, independent durable dispatches, typed shared
client, RBAC/audit, RLS, retry semantics, and PostgreSQL-owned coordination.

## Diagnoses and required corrections

### `FIND-TASK-007-1` — selector-safe failures

`KeyError::Unavailable` carries environment names, key-file paths, and Vault
selectors plus raw causes. Its public conversion and rewrap logging render
those details, and an HTTP test currently requires a selector fragment in the
response. Authorized callers and log readers can therefore learn deployment
secret topology contrary to Scenario 4.

Keep detailed causes internal to the existing `OperatorKeys` owner. Public
errors must retain the stable unavailable code and retry semantics but use a
constant safe detail. Logs may carry bounded source-kind, key-version, and
stable failure-class fields only; they must not contain environment names,
paths, Vault URLs/selectors, tenant-bearing key paths, or raw provider/I/O
text.

### `FIND-TASK-007-2` — production key readiness

Production validation currently proves only that Vault-shaped configuration
exists. Boot constructs `OperatorKeys` without reading an active tenant key,
so an unreachable provider, missing value, malformed base64, or wrong-length
key can become ready and fail only after a write or delivery. Candidate design
text also weakens the approved fail-start rule to write-only refusal.

Restore the approved statement in `architecture/wyrd-design.md`. Before a
multi-tenant production server becomes ready, reuse `OperatorKeys` and the
existing active-tenant directory to resolve and decode the configured active
version for every active provisioned tenant. Any directory, token, provider,
transport, missing-value, decoding, or length failure must refuse startup.
Keep development and explicitly single-tenant env/file allowances unchanged.

### `FIND-TASK-007-3` — screen before credential attachment

The HTTP path validates stored authority and refuses blocked destinations
before sending, but it materializes authentication headers before DNS
resolution, network-policy screening, and pinning. This violates REQ-149's
stronger ordering even though the general fetch protection works.

Preserve the existing registration and per-attempt authority decisions.
Construct only the effective URL and nonsecret request state first. For the
initial destination and every permitted redirect, resolve/screen/pin that URL
and attach the already-authorized credential only to the request sent through
that screened connection. Preserve retry classification, response bounds,
same-origin redirect rules, and idempotency behavior.

### `FIND-TASK-007-4` — remove unrelated workflow policy

Four implementation/review skill files add repository-wide diagnostician
policy that no TASK-007 requirement or consumer needs. Keeping those edits
would couple an unrelated process change to Operator delivery.

Delete only those four base-to-candidate skill-file changes. Any desired
workflow-policy change belongs in a separately approved change.

### `FIND-TASK-007-5` — protect the Vault boundary

Multi-tenant production accepts a plaintext Vault address and sends the Vault
token and receives KEKs over it. The Vault token-file path also bypasses the
owner-only permission rule already applied to mounted KEK files. Either path
defeats the external key boundary.

Require HTTPS for multi-tenant production Vault configuration, with plaintext
HTTP limited to an explicit non-production local fixture. Apply the existing
owner-only secret-file policy to Vault token files. Do not add a second TLS
switch, file policy, or dependency.

### `FIND-TASK-007-6` — exact Python and TypeScript contracts

The TypeScript surface accepts arbitrary update keys and declares a nested
`config` field that the flattened Rust wire view never returns. Python stubs
reduce all requests and views to broad mappings, so neither first-class SDK
faithfully exposes the closed provider variants required by REQ-148/150.

Project the existing Rust/schema contract directly: provider-discriminated
updates and flattened redacted views in TypeScript, and precise `TypedDict`
request/update/view unions in the Python stub source. Regenerate owned
declarations; do not hand-edit generated files or invent another DTO.

### `FIND-TASK-007-7` — nonblocking mounted-secret reads

Async create/update, delivery, and rewrap paths directly execute synchronous
filesystem metadata and reads for KEKs and Vault tokens. A slow secret mount
can block Tokio executor threads outside the configured delivery bounds.

Move the existing owner-only filesystem work through Tokio's filesystem path
or the repository's existing bounded blocking boundary, and reuse it for both
KEK and Vault-token files. Preserve permission validation, zeroizing ownership,
redaction, and current error classification; add no dependency or parallel
reader abstraction.

### `FIND-TASK-007-8` — module-level imports

Three ordinary production/library functions hide `base64::Engine` or Unix
`PermissionsExt` imports locally, violating the repository's mandatory import
layout.

Move those imports to their existing module import blocks, retaining the
platform conditional for Unix permissions. No suppression or helper is needed.

### `FIND-TASK-007-9` — rewrap must not starve delivery

The sole Operator claim loop awaits a cross-tenant rewrap pass before claiming
dispatches. One pass can perform many sequential external reads while holding
tenant work, so slow or unavailable Vault responses can delay unrelated due
dispatches beyond their five-minute deadlines without an attempt.

Keep rewrap owned by `OperatorKeys` and preserve `TenantConn`, `SKIP LOCKED`,
and secret-version fencing, but schedule it independently from delivery claims
with cancellation and finite per-pass and per-tenant elapsed-time bounds.
Within a pass, reuse an old key version already resolved for that pass; do not
cache keys across passes or delivery attempts.

## Constraints and preserved behavior

- Do not change the approved provider set, retry ceilings, public routes,
  permission names, encryption algorithm, AAD, persistence schema, or delivery
  success boundary.
- Do not weaken RLS, transactional audit, SSRF screening/pinning, secret
  redaction, TLS verification, shutdown recovery, or dispatch fencing.
- Do not add a cloud SDK, cipher, broker, key cache, process-local work
  registry, compatibility path, read-secret operation, or checked-in OpenAPI
  artifact.
- Preserve development and explicit single-tenant key-source allowances,
  local mock-provider fast lanes, and gated live-provider release evidence.
- Generated schemas and declarations must change through their owners and
  generators only.

## Non-goals

- Redesigning Operator delivery, adding providers, or revising the approved
  specification.
- Generalizing secret providers or rewrap scheduling beyond the bounded
  behavior required here.
- Shipping the unrelated workflow-skill policy in this candidate.

## Acceptance criteria

| Finding | Required closure |
|---|---|
| `FIND-TASK-007-1` | Sentinel env/file/Vault selectors and secret bytes appear in neither public errors nor captured logs; stable code and retry behavior remain. |
| `FIND-TASK-007-2` | Multi-tenant production refuses readiness for unavailable, missing, malformed, or wrong-size active tenant keys and boots with readable 32-byte keys; development behavior remains approved. |
| `FIND-TASK-007-3` | Blocked/unresolved initial URLs and rejected redirects never reach credential-header attachment or send; allowed pinned requests receive correct credentials after each screen. |
| `FIND-TASK-007-4` | The base-to-remediated-candidate diff under `.agents/skills` and `.claude/skills` contains no TASK-007 changes. |
| `FIND-TASK-007-5` | Production rejects plaintext Vault; owner-only token files succeed and group/other-readable token files fail before network use. |
| `FIND-TASK-007-6` | Static fixtures accept every valid provider shape, reject invalid provider combinations, and runtime journeys confirm flattened provider read fields. |
| `FIND-TASK-007-7` | Mounted-secret success/refusal remains correct and a stalled file source cannot block unrelated Tokio work. |
| `FIND-TASK-007-8` | No changed production/library function retains a local import; format and lint pass. |
| `FIND-TASK-007-9` | Slow/unavailable rewrap cannot prevent another tenant's due dispatch from being claimed and settled within its normal deadline; a repeated old version is resolved once per pass and retry remains safe. |

## Focused and broader proof

Add the smallest owner-level or existing journey assertions that directly
exercise each acceptance criterion. Use exact `mise exec -- cargo nextest run`
selectors for every specifically named Rust test and the repository-managed
Postgres wrapper where required. Then run the existing TASK-007 verification
set from the original task, including SQL/shared/Wyrd families, server/CLI/MCP
and SDK journeys, Python and TypeScript type/integration lanes, code generation,
tenant/client/PyO3/unwrap boundary checks, format, lint, and `git diff --check`.
The gated Slack/PagerDuty live smoke remains release evidence and is not a
credential-free remediation gate.

Route this task directly to `$wyrd-implement`.

## Implementation evidence

Candidate: `vcc/task-007` from `5f6d6f1f` through the commit recording this
evidence. Status: `IMPLEMENTED`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-007-1` | `components/operators/keys.rs`: `KeyError::Unavailable{version, kind, failure: KeyFailure}`, constant `UNAVAILABLE_DETAIL`, `KeyError::log` with bounded fields only | `keys::tests::key_failures_disclose_no_selector` (captured logs + problem JSON with env/file/Vault sentinels); `pg_operator_connection_routes::missing_key_refuses_only_credential_writes` | PASS |
| `FIND-TASK-007-2` | `boot::verify_operator_keys` → `OperatorKeys::verify_active` over `list_active_tenant_ids`, called from `build_state` (multi-tenant production only); `architecture/wyrd-design.md` fail-start text restored | `pg_operator_connection_routes::production_boot_requires_every_active_tenant_key` (unavailable, missing, malformed, wrong-length refused; readable boots; dev and single-tenant defer) | PASS |
| `FIND-TASK-007-3` | `verification/operators.rs`: `HttpRequest` carries no credential; `Credential::attach` runs per hop after `ScreenedHttp::client_for`; Slack/PagerDuty bearer attached in `post_json` after screening | `operators::tests::blocked_and_unresolved_destinations_never_get_a_credential`, `operators::tests::each_screened_hop_attaches_its_own_credential` | PASS |
| `FIND-TASK-007-4` | Skill files reverted to base (`5f6d6f1f`) | `git diff --stat f8811ac5..HEAD -- .agents/skills .claude/skills` empty | PASS |
| `FIND-TASK-007-5` | `config.rs` `OperatorKeysConfig::validate(production, multi_tenant)` rejects `http` Vault in production; Vault token read through `read_owner_only` | `config::tests::production_vault_requires_https`; `keys::tests::vault_reads_require_an_owner_only_token_file` (0o644 token refused before any request reaches the mock) | PASS |
| `FIND-TASK-007-6` | `sdks/wyrd-sdk-ts/wyrd/src/index.ts` closed update union + flattened view; `stubs/operators.pyi` TypedDict unions, `__init__.pyi` regenerated; `py:typecheck` runs with `--error unused-ignore-comment` | `tests/unit/operator-connection-types.test.ts` (`ts:typecheck`), `tests/unit/test_operator_connections_typing.py` (`py:typecheck`); Python and TS journeys assert flattened Slack/PagerDuty/HTTP fields and no `config` | PASS |
| `FIND-TASK-007-7` | `keys.rs` `read_owner_only` via `tokio::task::spawn_blocking`, shared by KEK files and Vault token files | `keys::tests::stalled_key_file_does_not_block_the_executor` (FIFO), `keys::tests::file_keys_seal_open_and_bind_context` | PASS |
| `FIND-TASK-007-8` | `base64::Engine` / `PermissionsExt` imports at module level in `keys.rs` and `wyrd-testing/src/server.rs` | `mise run fmt`, `mise run lints` | PASS |
| `FIND-TASK-007-9` | `OperatorWorker::run` joins the claim loop with an independent `rewrap` loop; `rewrap_pass` has pass and per-tenant `RewrapBudget` timeouts, cancellable on stop; old keys cached per `rewrap_tenant` call only | `pg_operator_delivery::slow_rewrap_never_holds_back_another_tenants_delivery` (stalled Vault tenant; other tenant delivered; one read per old version), `next_attempt_on_another_replica_uses_the_rotated_credential`, `revoked_connection_fails_closed_and_key_outage_retries` | PASS |
| Reuse A (Vault client) | `OperatorKeys::new` returns `Result<_, KeyError>`, calls `wyrd_tls::install_crypto_provider()`, builds the timeout/no-redirect client, and maps build failure to `KeyError::Client`; boot surfaces `ServerBootError::OperatorKeys` | `mise run test:wyrd`, `test:principals:integration`, focused keys tests | PASS |
| Reuse B (shared claim loop) | `verification/claims.rs`: `ClaimLoop` (permits, limits, drain, cancellation, claim round, stop-racing claim, reap) + `LeasedWork`; `VerifierRunner` and `OperatorWorker` implement only due-tenants/claim/process/release-late; rewrap runs beside the loop | `mise run test:bifrost:integration:server` (`pg_verification_runtime`, 80 passed), `mise run test:wyrd`, focused operator delivery tests | PASS |

Commands run in this session, each exit 0: `mise run fmt`, `py:format`,
`lints`, `py:lints`, `py:typecheck`, `ts:typecheck`, `codegen:check`,
`check:tenant-isolation`, `check:client-tier`, `check:pyo3-scope`,
`check:unwrap-audit`, `py:test:unit` (491), `test:shared` (694), `test:sql`,
`test:wyrd` (2152), `test:principals:integration`, `test:wyrdstate:journey`,
`test:platform:journey`, `test:cli:journey`, `test:bifrost:journey:server`,
`test:bifrost:journey:mcp`, `test:bifrost:integration:server`,
`py:test:integration` (61), `ts:test:integration` (21), `git diff --check`;
plus the exact selectors:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::operators::keys::tests::decode_key_requires_32_base64_bytes) | test(=components::operators::keys::tests::file_keys_seal_open_and_bind_context) | test(=components::operators::keys::tests::key_failures_disclose_no_selector) | test(=components::operators::keys::tests::vault_reads_require_an_owner_only_token_file) | test(=components::operators::keys::tests::stalled_key_file_does_not_block_the_executor) | test(=config::tests::production_vault_requires_https) | test(=verification::operators::tests::blocked_and_unresolved_destinations_never_get_a_credential) | test(=verification::operators::tests::each_screened_hop_attaches_its_own_credential)'
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test pg_operator_connection_routes --test pg_operator_delivery -E 'test(=missing_key_refuses_only_credential_writes) | test(=production_boot_requires_every_active_tenant_key) | test(=slow_rewrap_never_holds_back_another_tenants_delivery) | test(=next_attempt_on_another_replica_uses_the_rotated_credential) | test(=revoked_connection_fails_closed_and_key_outage_retries)'"
```

Non-goals held: no provider, route, permission, schema, cipher, dependency,
cross-pass key cache, or checked-in OpenAPI change; live Slack/PagerDuty smoke
remains release evidence.

Limits: the boot gate is proven through `wyrd_server::boot::verify_operator_keys`
(the exact function `build_state` calls) rather than a full production
`build_state`, which needs complete production TLS and peer material. The Python
connection `TypedDict`s are stub-only (runtime import needs `TYPE_CHECKING`),
matching the finding's instruction not to add a parallel runtime shape.
