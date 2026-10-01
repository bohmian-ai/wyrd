# TASK-001 r3 task implementation review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `eb4142cc95fa24b0525c71f66bfc978577cd4b7c`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Prior remediation tasks: `TASK-001-R1-production-readiness-gaps.md` and `TASK-001-R2-remaining-production-readiness-gaps.md`

The complete base-to-candidate range was reviewed. Prior reports were used only
to identify closure obligations; repository source, the cumulative diff, and
committed tests were the implementation evidence. The candidate remained at the
supplied commit throughout this review.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: OIDC is optional; connectionless startup and machine credentials do not require an IdP or sealing key | Optional `HumanConnections` construction and keyless boot inventory in `wyrd-server/src/boot`; active lookup returns no connection rather than inventing trust | Connectionless cases are present in `identity_e2e`; the task records green identity/platform journeys, not rerun in this static review | PASS |
| REQ-002: at most one Active and one Candidate per tenant; same issuer remains tenant-isolated | Partial unique indexes and forced RLS in `20260925000000_auth_human_connections.sql`; lifecycle uses the tenant slot lock and `TenantConn` | Two-tenant and concurrent activation cases are present in `tenant_connection_admin_journey` and `tenant_connection_rotation_journey` | PASS |
| REQ-003: authorized typed headless list/stage/test/activate/deactivate/remove lifecycle, durable across replicas | Six routes in `wyrd-server/src/components/admin/identity.rs`; bearer-derived tenant; `ConnectionInput::from_slice` decodes the advertised contract only after authorization; lifecycle owner reads durable Postgres state | Route/OpenAPI assertions and two-replica lifecycle cases are committed; recorded lanes were not rerun | PASS |
| REQ-004: exact configured callback, supported client authentication, issuer/discovery validation, and production-safe provider endpoints | Runtime callback is deployment-derived; callback and client-auth probes now fail closed; unsupported auth and secret shape are typed; token/JWKS server calls pass through production scheme/address screening | Focused callback/client-auth and screening tests cover mixed/duplicate response arms, wrong auth, HTTP refusal, proxy bypass, and exact callback | **FAIL — `TASK-REV-R3-001`**: live login returns a newly discovered cleartext authorization endpoint without applying production scheme policy |
| REQ-005: provider secrets are validated, sealed, redacted, rotatable, and absent from response/audit artifacts | Versioned `SealingKeyring`; restrictive active/retained key-file loader; redacted views/errors; CAS rewrap across all provider-secret stores | Rotation journey now removes the K1 writer before the final pass and serves from K2-only; config tests cover restrictive files | PASS |
| REQ-017: authorized/refused mutations use canonical redacted audit and audit failure rolls back | Handlers decide before input/provider IO; mutation owners append in their tenant transaction; candidate test repeats the real decision after network IO and stamps with that audit | Admin/rotation journeys contain allowed, denied, and injected audit-failure cases | PASS |
| Replacement, deactivation, and removal immediately stop old login and refresh families | Login state and refresh rows carry exact connection id/revision; initial issuance and refresh successor recheck Active state under the slot lock; provenance-free legacy rows remain unbound | `tenant_connection_session_cutoff_journey` covers cross-replica refresh and in-flight callback cutoff; migration test covers legacy A-to-B provenance and a fresh B family | PASS |
| Migration preserves safe legacy configuration and workload bindings, and fails before mutation on ambiguous/unsafe Human trust | Migration preflight covers multiple issuers, roles, audience, auth method, and missing secret; Human rows backing workload bindings become Workload; other Human rows leave the old trust store | `pg_tests::human_connection_upgrade_preflight` contains success and atomic refusal cases plus legacy refresh provenance | PASS |
| Tenant SQL work stays behind repository-owned `WyrdPostgres`/`TenantConn` capabilities | `HumanConnections`, `PgLoginStateStore`, and materially changed `PgIssuerResolver` own `WyrdPostgres`; no raw pool remains in those owners | Pool-boundary check source and resolver tests are committed; no Cargo lane rerun here | PASS |
| Provider IO is screened, proxy-free, DNS-pinned, timeout-bounded, and response-bounded | `ScreenedHttp::client_for` disables proxies, pins screened DNS, refuses redirects and production HTTP; `read_bounded_body` caps discovery, JWKS, candidate-token, and real-token bodies at 1 MiB decoded | Focused tests cover ambient proxy, cleartext policy, declared/chunked/compressed oversize bodies, and an untested oversized candidate | PASS for server IO; browser authorization redirect failure is separately recorded under `TASK-REV-R3-001` |
| Sealing-key retirement has a credible post-roll zero-reference proof | Rotation journey stages a deliberate late K1 value, shuts down the final K1 writer, starts the K2 final pass, verifies both values, then starts K2-only serving | `tenant_connection_rotation_journey` source directly checks the ordering and K2-only read/login | PASS |
| Legacy Human administration paths refuse while Workload paths and references remain intact | Trusted-issuer API/CLI reject Human with `HUMAN_CONNECTION_REQUIRED`; migration retains bound workload issuer rows as Workload | Admin journey and migration cases exercise both refusal and preservation | PASS |
| Public responses, audit, logs, schemas, and docs expose no provider secret or recovery key | Read contracts omit secrets; secret-bearing types are redacted; audit payloads use typed summaries; OpenAPI asserts redacted view shape | Admin/OpenAPI assertions and generated error-code change are committed; codegen/docs lanes are recorded only | PASS WITH LIMIT |
| AC-003 / AC-007: tenant isolation, provider failures, rotation, concurrent lifecycle, audit failure, and replica visibility have journey proof | Real-server journey source covers the packet-prescribed admin, rotation, and session-cutoff paths | Long Postgres/Keycloak/Dex journeys were not rerun by this reviewer | PASS WITH LIMIT |
| AC-009: public contract, CLI behavior, generated errors, and operator docs agree | OpenAPI registrations, typed contracts, CLI Human refusal, TypeScript error codes, and OIDC/rotation docs are changed in the cumulative range | Contract/docs checks are recorded in task evidence but not rerun | PASS WITH LIMIT |
| INV-003: platform, tenant human, and workload planes remain distinct | Tenant connections authorize only tenant `User` sessions; platform login and workload issuer stores remain separate; recovery requires tenant permission | Source and journeys preserve distinct routes and principals | PASS |
| INV-004: discovery is metadata, not trust; TLS/SSRF/DNS/JWKS/audit/tenant controls fail closed | Server-side discovery, token, and JWKS traffic use screened clients; active trust comes only from durable tenant connection state | **FAIL — `TASK-REV-R3-001`** for the live authorization redirect downgrade path |
| INV-006 and packet non-goals: no hosted signup, commercial hook, UI, second human trust store, compatibility route, or unsupported auth method | Complete cumulative name/diff inspection found only the tenant administration capability and its required consumers | N/A | PASS |
| Prior r1 findings `FIND-TASK-001-2`, `-3`, `-4`, `-6`, `-7`, `-9`, `-10`, `-11`, `-13` remain closed | Their corrected shared owners and tests remain in the candidate | Static cumulative inspection | PASS |
| Prior r2 remediation closes `FIND-TASK-001-1`, `-5`, `-8`, `-12`, `-14` through `-19` | Exact-arm callback qualification; unbound legacy refresh; resolver rustdoc/ownership; K1-writer shutdown; authorized raw typed decode; module imports; restrictive key files; HTTPS screening; bounded bodies | Focused proofs named by R2 are present in source; execution results are recorded but were not independently rerun | PASS |

## Proposed findings

### TASK-REV-R3-001 — INCORRECT: live tenant login can redirect a browser to a cleartext authorization endpoint

- **Violated obligation:** REQ-004 requires discovered provider endpoints to be
  validated; INV-004 requires TLS and provider networking to remain fail
  closed. The R2 production-TLS remediation also intended production screening
  to protect discovery-derived authorization, token, and JWKS endpoints, not
  only the candidate-test snapshot.
- **Exact location:** `crates/wyrd/wyrd-auth/src/login.rs:129-154`, especially
  the direct use of `provider.metadata.authorization_endpoint` at lines
  137-150; compare the production scheme policy in
  `crates/shared/wyrd-auth-oidc/src/screening.rs:130-165` and the candidate
  probe's screened use at `crates/wyrd/wyrd-auth/src/connections.rs:717-739`.
- **Evidence:** `HumanConnections::begin_login` is the production tenant-login
  owner called by `wyrd-server/src/auth/login.rs:97-110`. It re-fetches the
  discovery document on every login, takes the returned authorization URL, and
  immediately builds and returns it as `LoginInitResponse`/HTTP redirect. It
  never calls the production `ScreenedHttp` policy for that endpoint. The
  contract's `AbsoluteUrl` intentionally accepts both HTTP and HTTPS. Candidate
  testing screens the endpoint when the candidate is tested, but that is only
  a prior snapshot: an active provider can later publish an HTTP
  `authorization_endpoint`, and live login accepts it without another test or
  restart.
- **Observable consequence:** A tenant whose issuer still serves trusted HTTPS
  discovery can cause Wyrd to redirect a user's live production login,
  including OAuth state, nonce, PKCE challenge, client id, and callback, to a
  cleartext authorization endpoint. A network attacker can then observe or
  alter the login page and authorization exchange even though production
  screening purports to require HTTPS.
- **Required testable correction:** Reuse the existing production scheme policy
  at the live login boundary before persisting state or returning a redirect.
  Under `BlockInternal`, refuse a discovered non-HTTPS authorization endpoint;
  retain the existing `AllowInternal` HTTP path for repository-managed local
  providers. Do not add a policy knob, second client, or cached metadata. Keep
  callback, token/JWKS screening, and platform-login behavior unchanged unless
  their separately owned contract requires the same correction.
- **Focused closure proof:** A tenant-login test using HTTPS discovery that
  advertises an HTTP authorization endpoint must fail before a login-state row
  is inserted or a redirect is returned under `BlockInternal`; the existing
  local-provider login must continue to succeed under `AllowInternal`.

## Prior-finding closure

All 19 stable findings from r1/r2 are closed in the cumulative candidate. The
new proposed finding is distinct from `FIND-TASK-001-18`: that remediation
correctly protects server requests through `ScreenedHttp`, while this path
returns the discovered URL to the browser without invoking that policy after
activation.

## Verification limits

- Static review only, as assigned. No Cargo or `mise` Cargo lane was run because
  reviewers share the checkout.
- The cumulative diff, live call paths, migrations, route contracts, and
  committed focused/journey tests were inspected. Recorded implementation
  results were treated as available execution evidence, not as proof of source
  behavior.
- Long provider/Postgres journeys, served OpenAPI, codegen, format, lint, docs,
  and boundary lanes remain independently unverified in this Wave 1 report.
- CodeGraph was unavailable because this checkout has no `.codegraph/` index;
  ordinary repository search and direct source inspection were used.

## Overall result

**FAIL** — the prior remediation ledger is closed, but
`TASK-REV-R3-001` leaves production tenant login able to accept a cleartext
authorization endpoint discovered after activation.
