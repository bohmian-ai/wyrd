# TASK-001-008-R3 — Close validated admin-principals findings

## Route

Implement this packet with `$wyrd-implement`. Make the smallest cohesive
correction that closes every finding below. A later `$wyrd-task-review` must
review the complete cumulative candidate, not only this remediation diff.

## Authority and reviewed candidate

- Approved specification: `changes/active/admin-principals/spec.md`, revision 7.
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md` through
  `TASK-008-*.md`.
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`.
- Reviewed candidate: `a9a7c9c1e8502ccf3befa74c4283c78e5d70c132`.
- Validated ledger:
  `changes/active/admin-principals/review/whole-branch-03/findings-validation.md`.

All of `FIND-TASK-001-10` is waived. Do not rewrite history, remove AI trailers,
or change author/committer identities. The verified-change-contract commits are
approved cumulative content; preserve them and do not classify them as drift.

## Outcome

Finish the approved administration capability by tightening its existing SQL,
audit, refresh, OpenAPI, OIDC, secret, CLI, documentation, and retained-audit
owners. Reuse the existing mechanisms named below. Do not add a parallel audit
path, pool wrapper, error catalog, refresh service, secret wrapper, schema
framework, transport, or test harness.

## Validated diagnosis and required correction

### `FIND-admin-principals-2` — broad database capability remains in live owners

`components/platform/provisioning.rs:93-116` and
`components/platform/recovery.rs:36-58` retain `WyrdPostgres`, contrary to the
mandatory `TenantConn`/`OperatorPool` boundary and the previous remediation.
Both served workflows can consequently open arbitrary tenant transactions.

Keep pool composition and tenant acquisition at the existing server/route
boundary. Give provisioning and recovery only the acquired `TenantConn` for
tenant work and the existing `OperatorPool` for platform work. Extend the
current boundary check to these fields and constructors; add no wrapper or
provider trait.

### `FIND-admin-principals-3` — public conflicts reveal database constraints

`components/admin/routes.rs:699-705,721-734` inserts
`SqlError::{UniqueViolation,FkViolation}.constraint` into public messages and
details. A physical rename therefore changes the API and authenticated callers
learn internal schema identifiers.

Keep the existing 409 variants and mappers, but make their response text static
and operation-specific. Emit the constraint only through structured server
tracing. Prove duplicate and referenced-delete responses preserve
`WYRD_AUTH_409_ADMIN_CONFLICT` without the sentinel physical name.

### `FIND-admin-principals-13` — generated administration contract is incomplete

`http/openapi.rs:314-365` skips exact stable-code validation for Platform and
Principals operations. `/auth/token` omits reachable refresh, delegation, and
principal-not-found codes. `wyrd-spec/src/auth/admin.rs:100-121,143-152` uses
unconstrained `serde_json::Value` where concrete claim, map/vector, and
`CardRef` types already exist. `auth/revoke.rs:19-24` still generates prose
claiming the allowance commits before the now-atomic effect.

Keep the existing DTO and `utoipa` owners. Replace response-side `Value` fields
with the existing concrete types, correct the revoke documentation, enumerate
the exact reachable stable codes for every administrative operation, and
strengthen the current generator test to compare exact per-operation sets and
problem media across all four administrative tags. Do not create a second
catalog or checker.

### `FIND-005-1` — issue-key allowance commits before its effect

`components/auth/routes.rs:365-393` calls standalone `record_audit`, commits the
allowed decision, and only then opens the transaction that issues and audits
the key. Later failure leaves a durable allowance for a nonexistent effect.

Acquire one existing `TenantConn`, append the returned allowance with the
canonical `audit::append_on`, issue the key and issuance evidence, then commit
once. Preserve independently durable denials. Prove a post-authorization
failure commits neither key nor allowance.

### `FIND-004-5` — operator configuration journey is incomplete

`wyrd-cli/tests/operator_journey.rs:63-117` creates a tenant and jumps to
principal creation without using the shipped tenant trusted-issuer/workload
configuration command. The operator page mirrors the omission and its command
synopsis claims only `init` exists despite the shipped `recover-root` action.

Extend the existing real-server CLI journey and the existing operator page:
tenant create -> tenant OIDC configuration -> restricted principal creation
and use. Add `recover-root` to the current synopsis. Do not add platform-human
identity CLI commands, another page, or another transport owner.

### `FIND-admin-principals-R2-3` — refresh loses credential attribution

`wyrd-auth/src/refresh.rs:118-167` has the consumed durable row id as
`active.id`, but the rotation event and successor access token use
`credential_id: None`. Refresh-authenticated decisions then look like
credential-free federation.

Use `active.id` for the rotation audit and successor access-token context.
Continue using `None` for genuine federated and delegated sessions. Prove a
TenantAdmin refresh and subsequent protected decision carry the consumed
refresh-row id while federation remains null.

### `FIND-admin-principals-R2-4` — refresh replay containment rolls back

`wyrd-auth/src/refresh.rs:179-218` stages family revocation and its canonical
audit, then returns `RefreshError::Reused`. The route commits only `Ok`, so the
error path drops the transaction while returning `WYRD_AUTH_401_REFRESH_REUSED`.
An attacker's successor remains usable.

At the existing route boundary, commit the already-staged transaction only for
the typed `Reused` result before rendering the existing 401. All other errors
retain current rollback behavior. Through `/auth/token`, rotate, replay, and
then prove the successor is refused and exactly one committed family-revocation
event is visible from another transaction.

### `FIND-admin-principals-R3-1` — platform audit names the wrong resource

`platform_authz.rs:104-110,152-172` accepts only an optional tenant and otherwise
records literal `platform`. Identity and credential callers therefore lose
their target. Provisioning audits a proposed id before
`insert_provisioning_tenant` can return a resumed tenant, leaving the committed
row attached to a discarded id.

Pass the exact resource already known by every caller into the existing
authorization owner. Use stable target spellings for connection, principal,
credential, and status operations; use the requested tenant slug for create so
fresh and resumed attempts remain truthful. Keep `target_tenant_id` separate
when the final tenant already exists. Do not mutate staged hash-chain rows or
add a metadata channel.

### `FIND-admin-principals-R3-2` — platform issuer equality is noncanonical

`components/platform/identity.rs:206-246` parses and normalizes an issuer but
persists the raw request. Registration copies that raw value, while login
reparses and searches with the normalized value. A valid trailing slash can
therefore make first login impossible.

Persist, return, and reuse the already-parsed `issuer.as_str()` for both the
connection and identity. Add no normalizer. Prove the served trailing-slash
configuration, preregistration, and first-login pinning path.

### `FIND-admin-principals-R3-3` — tenant issuer secrets are debug-visible

`wyrd-spec/src/auth/admin.rs:65-80` and
`wyrd-cli/src/auth/trusted_issuer.rs:28-50,252-278` retain client secrets as
ordinary strings in debug-derived structures. The repository already provides
`wyrd-spec/src/auth/secret_bearer.rs` for the redacted write-only wire shape.

Reuse `SecretBearer` on the wire and `SecretString` after CLI resolution, with
redacted debug behavior for the argument holder. Keep JSON compatibility as a
string and mark the generated schema write-only/password. Do not introduce a
secret type.

### `FIND-admin-principals-R3-4` — mandatory Rust contracts are incomplete

`wyrd-server/src/main.rs:90-105` adds fallible `recover_root` without an
`# Errors` section. New helpers in `platform_admin_e2e.rs:172-187,3455-3503`
panic through `expect` but omit required `# Panics` sections.

Document the actual failure and panic contracts on the existing items. No
helper, lint suppression, or test is required.

### `FIND-admin-principals-R3-5` — audit credential upgrade breaks history

Adding nullable `credential_id` changes the registered retained
`audit_log` fingerprint. Every publication calls `ensure_builtin`, which
rejects the old registration; migration 27 evolves only Postgres staging. The
same change inserts a zero byte into the hash preimage for every historical
null-credential row, contradicting the migration's reproducibility claim.

Keep the one retained table and publisher. In the existing catalog owner,
recognize only the exact prior `audit_log` fingerprint and perform one
idempotent additive evolution that appends nullable `credential_id`, preserves
all old Iceberg field ids, and advances the control fingerprint. Reconcile the
retry state where the physical schema evolved but control metadata did not;
continue rejecting every unrelated fingerprint mismatch. For hashing, retain
the legacy preimage whenever credential id is absent and append the credential
segment only when present. Do not add a general migration framework, hash
version column, parallel table, or history rewrite.

Seed an exact pre-change physical table, catalog registration, staging row,
chain head, and retained row; upgrade; verify the old hash; publish null and
non-null rows; restart/replay; and read one uninterrupted history with preserved
old field ids.

## Constraints and non-goals

- Preserve the two administrative planes, closed principal kinds, Card-free
  administrative principals, existing permission vocabulary, and canonical
  audit staging/publisher path.
- Preserve independently durable denied decisions and atomic same-plane allowed
  decisions/effects.
- Preserve platform and tenant isolation, refresh rotation, fixed-cost API-key
  verification, last-admin protection, idempotent provisioning, and root
  recovery.
- Do not alter commit provenance or approved verified-change-contract content.
- Do not broaden CLI scope beyond the already approved operator journey.
- Do not widen generic built-in schema evolution or weaken fingerprint checks.
- Do not solve the excluded base-red auth failure, Card-name handoff, stale
  testing comment, or rustdoc-lane policy decision in this packet.

## Acceptance criteria

| Finding | Acceptance |
|---|---|
| `FIND-admin-principals-2` | Live provisioning/recovery owners contain only `TenantConn`/`OperatorPool` capabilities, enforced by the existing boundary check. |
| `FIND-admin-principals-3` | Admin conflict responses retain stable 409 codes and contain no physical constraint identifier. |
| `FIND-admin-principals-13` | Generated OpenAPI has concrete response schemas, exact reachable stable codes, correct media/auth, and truthful revoke prose. |
| `FIND-005-1` | Issue-key effect and allowance commit or roll back together; denial remains durable. |
| `FIND-004-5` | The real CLI journey performs tenant configuration before restricted-principal creation/use, and operator docs consistently include recovery. |
| `FIND-admin-principals-R2-3` | Refresh rotation and successor decisions carry the consumed refresh credential id; federation remains null. |
| `FIND-admin-principals-R2-4` | Reuse returns the existing 401 after durably revoking the family once; the successor cannot rotate. |
| `FIND-admin-principals-R3-1` | Every reviewed platform operation records its exact resource; resumed provisioning never records the discarded proposal id. |
| `FIND-admin-principals-R3-2` | Trailing-slash platform issuer configuration completes first-login pinning. |
| `FIND-admin-principals-R3-3` | Request/CLI debug cannot reveal a sentinel secret; JSON remains a string and schema is write-only/password. |
| `FIND-admin-principals-R3-4` | Every named new fallible or panicking item has the required accurate rustdoc section. |
| `FIND-admin-principals-R3-5` | A pre-change audit table/history upgrades, verifies, publishes both credential shapes, replays, and preserves old field ids. |

## Required proof

Add or tighten the focused tests named in each diagnosis, using the existing
fixtures and journeys. Every specifically named Rust test must be run with an
exact package, target, and `test(=...)` expression through `mise exec --`, with
the repository Postgres wrapper where required.

At minimum run:

- `mise run fmt:check`
- `mise run lints`
- `mise run check:client-tier`
- `mise run check:unwrap-audit`
- `mise run check:clippy-allow-audit`
- `mise run check:tenant-isolation`
- `mise run test:principals:unit`
- `mise run test:principals:integration`
- `mise run test:platform:journey` twice consecutively
- `mise run test:cli:journey`
- `mise run test:bifrost:journey:mcp`
- `mise run test:sql`
- `mise run codegen:check`
- `mise run docs:check`
- strict `wyrd-sql` rustdoc with warnings denied

Do not substitute `mise run gate`; approved `VER-003` excludes the broad gate
for this change. Record exact focused commands and nonzero results in the
implementation evidence appended to this packet.

