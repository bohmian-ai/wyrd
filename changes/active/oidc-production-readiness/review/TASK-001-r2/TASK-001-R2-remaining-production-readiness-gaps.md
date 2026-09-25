---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-003, REQ-004, REQ-005, REQ-016, INV-004, AC-003, AC-007, AC-009]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-5, FIND-TASK-001-8, FIND-TASK-001-12, FIND-TASK-001-14, FIND-TASK-001-15, FIND-TASK-001-16, FIND-TASK-001-17, FIND-TASK-001-18, FIND-TASK-001-19]
---

# Close remaining tenant-connection production-readiness gaps

## Authority and immutable review subject

- Approved spec: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-001-r1/TASK-001-R1-production-readiness-gaps.md`
- Reviewed base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Reviewed candidate: `bb4895d8e630ee8fb2ba075d6c3eeaa348e49414`
- Validated ledger: `changes/active/oidc-production-readiness/review/TASK-001-r2/findings-validation.md`

Implement this task through `$wyrd-implement`. A later review must reassess the
complete cumulative base-to-candidate range, not only this remediation diff.

## Outcome

Finish the existing tenant OIDC administration capability by closing the
remaining callback, migration, SQL-owner, typed-boundary, secret-file,
provider-network, documentation, and rotation-proof gaps. Reuse the current
owners and dependencies; do not add product scope or another framework.

## Issue diagnoses and required corrections

### Provider qualification and network bounds

`FIND-TASK-001-1` — `callback_redirect_qualifies` accepts a response containing
both `code` and `error` because it joins independently valid arms with `||`.
That ambiguous response can stamp a candidate despite R1 requiring exactly one
standard response arm. Keep the helper and accept only one nonempty code with no
error or one recognized error with no code; reject mixed and duplicate arms.

`FIND-TASK-001-18` — discovery validates the configured issuer's scheme but
allows discovered authorization, token, and JWKS endpoints to be cleartext.
`ScreenedHttp` screens addresses without screening schemes, so production token
exchange can send a client secret and authorization code over HTTP. Enforce the
scheme once in `ScreenedHttp::client_for`: `BlockInternal` permits HTTPS only;
`AllowInternal` preserves the existing local-provider/test HTTP path. Add no
new policy switch or caller-specific checks.

`FIND-TASK-001-19` — discovery, JWKS, candidate-probe, and real token-response
paths use reqwest `json()` or `bytes()` and can buffer unlimited decoded,
chunked, or compressed provider data. Add one fixed 1 MiB decoded-response
limit in the shared OIDC crate using reqwest's installed chunk stream. Route
all four JSON/error paths through it while preserving timeouts, cancellation,
and existing public error mapping. Do not add a dependency, configuration
surface, or per-caller limits.

### Session provenance and rotation proof

`FIND-TASK-001-5` — the upgrade migration assigns every unrevoked legacy user
refresh row to whichever Human connection it just made Active, although the
legacy row records no family-level issuer provenance. A session created under
deleted issuer A can therefore renew as if it authenticated under replacement
B. Delete that inference update and leave legacy user refresh rows unbound so
the existing fail-closed refresh path requires re-login. Do not infer
provenance from tenant, user, email, or current issuer; preserve machine rows
and fresh post-migration binding.

`FIND-TASK-001-12` — the rotation journey performs its claimed post-roll pass
while replica A remains live with K1 as its write key and uses A afterward.
After the deliberate late K1 write, remove every K1-write-capable replica from
service before the final K2-write/K1-retained pass. Then prove the late value is
current under K2, start K2-only serving, and route the remaining journey through
K2 writers. Do not change the CAS rewrap engine or add coordination state.

### Repository ownership and public boundary

`FIND-TASK-001-14` — materially changed `PgIssuerResolver` still stores
`Arc<PgPool>`, accepts it in its constructor, and calls `TenantConn::acquire`
directly. Make that resolver own the existing cloned `WyrdPostgres`, acquire
through `WyrdPostgres::tenant_conn`, and update all constructor sites. Do not
convert the untouched workload resolver, introduce a trait/wrapper, add a
feature, or add an allowlist entry.

`FIND-TASK-001-15` — the candidate PUT route advertises `ConnectionInput` in
OpenAPI but actually extracts `Json<Value>`. A plain typed Axum extractor would
also move semantic parsing before the required authorization decision and can
lose the stable `PrivateKeyJwt` refusal. Use Axum's native bounded raw-body
extraction so authorization still occurs before semantic interpretation, then
move byte-to-`ConnectionInput` decoding into the existing contract owner. That
decoder must preserve the specific unsupported-client-auth error before normal
validation. Remove `Json<Value>`; add no generic extractor framework or second
public DTO.

### Documentation, imports, and secret files

`FIND-TASK-001-8` — materially changed resolver fields, `IssuerDecodeError`
variants, `open_secret`, and `decode_secret` remain undocumented. Add
substantive rustdoc only to the changed resolver surface, including ownership
and failure invariants and `# Errors` for both fallible helpers. Do not add lint
allowances or sweep unrelated code.

`FIND-TASK-001-16` — the new ambient-proxy regression test places ordinary
`wiremock` imports inside the test function. Move exactly those imports to the
existing test-module import block without changing behavior or adding helpers.

`FIND-TASK-001-17` — active and retained sealing-key file loaders call
`read_to_string` directly and accept permissive, non-regular, or oversized
files. Reuse the existing `wyrd_gateway::read_secret_file` owner for both
`WYRD_SEALING_KEY_FILE` and `WYRD_SEALING_RETAINED_KEYS_FILE`, map its redacted
static refusal into the existing configuration error, and document owner-only
permissions plus atomic replacement. Do not create another loader or broaden
this task to unrelated signing-key code.

## Constraints and preserved behavior

- Preserve the approved route set, wire fields, callback path, stable error
  codes, one-Active/one-Candidate lifecycle, and 15-minute test stamp.
- Preserve authorization before provider IO and before semantic body errors are
  exposed; preserve fail-closed transactional mutation audit.
- Preserve bearer-derived tenancy, RLS, caller-owned transactions, workload
  issuer behavior, migration preflight, tombstones, refresh replay containment,
  and five-minute access-token behavior.
- Preserve provider-secret and recovery-key redaction, versioned ciphertext,
  keyless startup for secret-free stores, and the current CAS rewrap owner.
- Preserve the existing local-provider HTTP mechanism only under
  `AllowInternal`; production remains HTTPS-only.
- Reuse `HumanConnections`, `PgIssuerResolver`, `WyrdPostgres`, `ScreenedHttp`,
  reqwest, the existing connection-input contract owner, refresh-token owner,
  and `read_secret_file`.

## Non-goals

- No UI, hosted signup, commercial hook, social login, new authentication
  method, second trust store, compatibility route, or public contract redesign.
- No new HTTP client, provider-policy knob, response-limit configuration,
  dependency, database abstraction, DTO hierarchy, extractor framework, secret
  loader, rewrap engine, lease, or rotation coordinator.
- No conversion of untouched SQL owners and no unrelated rustdoc, signing-key,
  configuration, or test cleanup.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-1` | Callback qualification accepts exactly one valid code or recognized error arm and rejects mixed/duplicate arms. |
| `FIND-TASK-001-5` | Legacy refresh rows without exact provenance remain unbound and cannot rotate under a replacement; fresh post-migration login binds and renews. |
| `FIND-TASK-001-8` | Every materially changed issuer-resolver item has required meaningful rustdoc and fallible helpers have `# Errors`. |
| `FIND-TASK-001-12` | The final rotation pass starts only after every K1 writer is out of service, then K2-only serving succeeds. |
| `FIND-TASK-001-14` | `PgIssuerResolver` contains no raw pool field/signature or direct `TenantConn::acquire`; it uses `WyrdPostgres`. |
| `FIND-TASK-001-15` | Candidate PUT has no `Json<Value>`, stays authorized before semantic parsing, and preserves valid, malformed, unauthorized, and unsupported-auth behavior. |
| `FIND-TASK-001-16` | The proxy test has no ordinary function-local imports. |
| `FIND-TASK-001-17` | Active and retained sealing files accept only bounded owner-only regular files and never expose key material in refusal text. |
| `FIND-TASK-001-18` | Production screening refuses HTTP before sending; the existing local-provider path continues under `AllowInternal`. |
| `FIND-TASK-001-19` | Every provider JSON/error response path stops above 1 MiB decoded data, including chunked and compressed bodies. |

## Focused proof

Extend the smallest existing tests named by `findings-validation.md`:

1. callback qualification table cases for mixed and duplicate response arms;
2. migration coverage for a provenance-free legacy refresh family and a fresh
   post-migration B login;
3. the existing rotation journey with no live K1 writer before the final pass;
4. resolver compile/tests plus static absence of raw pools in that owner;
5. authorized valid, unsupported-auth, malformed, and unauthorized candidate
   PUT cases, including refusal before semantic error exposure;
6. owner-only, permissive, non-regular, and oversized active/retained sealing
   files with redacted errors;
7. production HTTP endpoint refusal with retained `AllowInternal` local proof;
8. oversized declared, chunked, and compressed decoded provider bodies, plus a
   candidate that remains untested after rejection; and
9. the existing ambient-proxy test, format, lint, and direct rustdoc/import
   inspection.

Run each specifically named Rust test with its exact `mise exec -- cargo
nextest run --locked` selector and repository-managed setup. Then run the
original task and R1 broader lanes: all filtered identity journeys and the
unfiltered identity journey, `test:principals:integration`, `test:sql`,
`test:platform:journey`, `codegen:check`, `check:tenant-isolation`,
`check:from-pools-allowlist`, applicable client/PyO3/unwrap boundaries,
`docs:check`, `fmt`, `lints`, and `git diff --check`. Keep identity journeys on
default features and prove exact test selection.
