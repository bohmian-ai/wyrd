# Security Domain Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `cd002ab1394cdc1679d95f957313a7f697d7e0a5`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Remediation inputs: TASK-007-R1 through TASK-007-R5 and their prior validated ledgers

The candidate remained the stated repository `HEAD` throughout this review.
The uncommitted R6 review artifacts are outside the immutable subject.

## Reviewed boundary

This cumulative review traced the security-sensitive path from typed connection
inputs through HTTP, MCP, CLI, and SDK projections; permission evaluation and
canonical audit; tenant-derived `TenantConn` access and forced RLS; envelope
sealing, opening, and rewrap; deployment key-source validation and readiness;
registration-time and attempt-time connection-authority checks; bounded failure
context; durable claim and retry settlement; and effective-URL DNS resolution,
screening, pinning, redirect handling, and credential attachment.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| Identity, RBAC, tenancy, and audit | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md`; REQ-145, REQ-148, INV-007, AC-030/031 | `components/operators/{routes,service}.rs`, `mcp/operators.rs`, `audit/mod.rs`, permission definitions, migration 32, connection SQL, and `pg_operator_connection_routes.rs` | PASS — caller-derived tenant identity, `operators:read`/`operators:write`, allow/deny audit, same-transaction write audit, forced RLS, and indistinguishable cross-tenant misses are retained |
| Secret input and redaction | Security-posture secret rules; REQ-138/139/147/148/150; TASK-007 scenarios 1, 2, and 4 | `wyrd-spec/src/operator_connection.rs`, body decoders, shared client and language projections, CLI body-file path, connection service/SQL, key error mapping, and journey assertions | PASS — secrets use redacted types, never enter read views, errors, audit payloads, or diagnostic text, and Postgres receives only ciphertext/wrapped-key material |
| Cryptography and key lifecycle | Security-posture cryptography rules; revision-36 REQ-147/150; TASK-007 scenario 3 | `wyrd-crypt/src/lib.rs`, `components/operators/keys.rs`, `config.rs:1776-2018`, `boot/mod.rs:1442-1474`, migration and rewrap SQL/tests | PASS — fresh AES-256-GCM DEKs, distinct domain-separated canonical AAD, tenant/connection/provider/name/version binding, external KEKs, exact versions, fenced rewrap, and selector-free failures are present |
| Deployment key-source policy | Revision-36 REQ-147; TASK-007-R5; `FIND-TASK-007-18` | `OperatorKeysConfig::validate` at `config.rs:1895-1963`, its `WyrdServerConfig::validate` caller, readiness gate, and `operator_key_source_follows_deployment` | PASS — env is development-only, production single-tenant permits owner-only file or HTTPS Vault, and production multi-tenant remains HTTPS Vault-only with active-tenant readiness |
| Registration and per-attempt authority | REQ-139/149/150; TASK-007 scenarios 5 and 7 | `card/operator.rs`, `components/cards/resolve.rs:112-148`, `verification/operators.rs:250-384`, provider modules, and connection-authority journeys | PASS — every attempt reloads the tenant's latest active connection, rechecks provider/origin/auth authority, then decrypts only that attempt's credential |
| SSRF and outbound credential handling | Agent-rules resolve-screen-pin rule; security-posture outbound boundary; REQ-142/149 | `wyrd-auth-oidc/src/screening.rs:94-217`, `verification/operators.rs:500-857`, focused initial/redirect tests, and real-server local-provider journeys | PASS — all DNS answers are screened and pinned, metadata/link-local is always blocked, production blocks internal ranges, redirects are manual and same-origin, and sensitive credentials attach only after each effective URL passes screening |
| Durable retry and security-relevant bounds | REQ-142/146/152, INV-015; TASK-007-R5; `FIND-TASK-007-19` | `verification/operators.rs:386-420,938-980`, `operator_dispatches.rs:99-118`, and `maximum_retry_after_settles_at_the_deadline` | PASS — provider-directed delay is bounded before interval construction, PostgreSQL retains the clock/deadline decision, and lease fencing, attempt ceilings, and terminal status behavior are unchanged |

## Prior-finding closure

| Findings | Cumulative status |
|---|---|
| `FIND-TASK-007-1` through `FIND-TASK-007-17` | **CLOSED** — the selector-free error boundary, production readiness, screen/pin ordering, bounded Vault/file reads, rewrap budgets, exact version handling, redacted debug, provider-local wire handling, required documentation/import shape, approved revision-36 key-source decision, and truthful MCP descriptor remain present. |
| `FIND-TASK-007-18` | **CLOSED** — `OperatorKeysConfig::validate` now rejects `OperatorKeySource::Env` for every production deployment while preserving development env, production single-tenant file/Vault, multi-tenant Vault/HTTPS, and deferred single-tenant provider reads. |
| `FIND-TASK-007-19` | **CLOSED** — `RETRY_SQL` bounds `$5` with the existing deadline operand `$6` before interval multiplication; the focused Postgres regression covers the maximum accepted decimal `Retry-After` and proves durable settlement at the database-owned deadline with the lease cleared. |

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None material to the approved task.

### Positive Controls

- Tenant identity is derived from the authenticated caller; tenant CRUD and delivery reads use `TenantConn`, the connection table forces RLS, and privileged discovery receives only nonsecret tenant/key-version columns.
- Connection writes audit the allowed permission decision in the mutation transaction; denied and uncommitted evaluated decisions use the canonical fail-closed audit path. Engine claims, retries, and rewraps correctly emit lineage/telemetry rather than false authorization audit rows.
- Postgres stores only authenticated envelope ciphertext, nonces, wrapped DEKs, versions, redacted authority, and lineage metadata. Plaintext credentials exist only in zeroizing values during sealing or one delivery attempt.
- Public errors, key logs, `Debug` implementations, request-decode failures, response shapes, and audit payloads do not expose credentials, KEKs, provider selectors, filesystem paths, Vault addresses, or raw provider responses.
- Authenticated HTTP delivery cannot use templates or redirects to change credential origin; forbidden transport and credential headers are rejected case-insensitively, and attached credential header values are marked sensitive.
- Retry, timeout, response-size, redirect, permit, claim, lease, and dispatch-deadline ceilings remain server-owned and cannot be raised by a Card.

## Verification limits

- This was a time-bounded, review-only cumulative static audit. I did not rerun broad or credentialed test lanes; the R5 implementation record reports passing focused config and Postgres regressions, `test:sql`, `test:wyrd`, format, lint, tenant-isolation, unwrap-audit, and `git diff --check`. I independently ran only `git diff --check` for the immutable range.
- Credentialed Slack and PagerDuty live smoke remains gated and was not available. Local mock-provider coverage proves request construction, credential ordering, status classification, and durable worker behavior, not vendor-side acceptance.
- No live production Vault, production secret mount, hostile DNS service, or multi-replica deployment was exercised in this wave. Their TLS/redirect policy, file-mode enforcement, DNS pinning, readiness, RLS, fencing, and rotation behavior were inspected in source and existing focused tests.
- CodeGraph was unavailable because this checkout has no `.codegraph/` index; caller tracing used the immutable source and `rg`.

## Overall result

**PASS** — no material security, RBAC, tenancy, cryptography/key-source,
redaction, audit, or SSRF/pinned-delivery finding remains in the cumulative
candidate. The R5 corrections close `FIND-TASK-007-18` and
`FIND-TASK-007-19` at their existing owners without weakening adjacent
security controls.
