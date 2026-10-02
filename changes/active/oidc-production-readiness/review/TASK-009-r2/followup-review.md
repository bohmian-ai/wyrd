# TASK-009 round-2 focused follow-up

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-009-r1/TASK-009-R1-relying-party-corrections.md`

The candidate remained at the stated commit. `.codegraph/` is absent, so this
pass used repository search, commit-qualified source, and direct caller
tracing. `FIND-TASK-009-5` is withdrawn and was not reopened.

This follow-up addresses only the conflict around `PERSIST-TEN-001` and the
conflict between `PERSIST-TEN-002` and `SYS-R2-001`. It is a discovery pass,
not a vote on unrelated findings.

## Inspected paths

### Platform configuration and process-owned relying party

- `crates/wyrd/wyrd-server/src/components/platform/identity.rs:191-276`
  (`configure_connection`), plus `:641-739` (served begin/callback owner lookup)
- `crates/wyrd/wyrd-server/src/components/admin/routes.rs:308-327,735-775`
  (workload caller and shared metadata-only helper)
- `crates/wyrd/wyrd-server/src/boot/issuer.rs:123-169,220-264`
  (workload boot caller)
- `crates/wyrd/wyrd-auth/src/platform_login.rs:84-334`
  (`PlatformLogin`, `begin`, and `complete`)
- `crates/shared/wyrd-auth-oidc/src/relying_party.rs:194-247,343-550`
  (`ScreenedHttp::provider_metadata` and `RelyingParty::{cached,discover,redeem}`)
- `crates/wyrd/wyrd-server/src/boot/mod.rs:1562-1604` and
  `crates/wyrd/wyrd-server/src/components/auth/state.rs:14-47`
  (one process-owned `PlatformLogin` in `ServerAuth`)
- `crates/wyrd/wyrd-auth/src/connections.rs:326-383`
  (nearby tenant human configuration/test pattern)
- `crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs:1396-1710,1731-1810`
  (served platform login/configuration proof)
- Base versions of the platform configure helper and `OidcProvider::discover`
  at `35a53faa2`

### Audience schema evolution

- `crates/wyrd/wyrd-sql/migrations/20260601000023_platform_identity.sql:38-55`
- `crates/wyrd/wyrd-sql/migrations/20261002000001_platform_oidc_client_audience.sql:1-4`
- Candidate and base versions of
  `crates/wyrd/wyrd-sql/src/queries/platform/identity.rs:21-124`
- `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:592-624`
- `crates/wyrd/wyrd-sql/migrations/20260925000000_auth_human_connections.sql:20-98`
  (existing audience-mismatch preflight)
- `architecture/operations/deployment-and-release.md:138-173,175-218`
  (migration ordering and expand-and-contract authority)
- `changes/active/oidc-production-readiness/review/TASK-004-r2/lead-direction-FIND-TASK-004-11.md`
  (unreleased-migration rule; not applicable to the already-mainline
  `20260601000023` schema)

## Uncertainty 1: platform configuration discovery

### Reachable path and ownership

The authenticated `PUT /platform/oidc/connection` route reaches
`configure_connection`. At `components/platform/identity.rs:235-237` it calls
`admin::routes::discover_jwks_uri`, the same helper used by workload trusted-
issuer creation. After remediation that helper deliberately calls
`ScreenedHttp::provider_metadata`, which reads and types only the discovery
document and never requests JWKS (`admin/routes.rs:735-775`;
`relying_party.rs:194-247`). The route then commits the platform row at
`components/platform/identity.rs:255-267`.

The same `AppState` already owns one `PlatformLogin`, composed once at boot and
shared by served begin and callback requests. Its `RelyingParty::cached` path
does full `openidconnect` discovery plus JWKS and may retain an entry for five
minutes (`platform_login.rs:84-169`; `relying_party.rs:364-435`). Platform
configuration does not call or invalidate that owner.

This is reachable in two ordinary cases:

1. a new platform connection can be acknowledged while its advertised JWKS is
   unavailable or unusable, and its first login then fails; and
2. after the process cache has been primed, reconfiguring the same issuer can
   commit new discovered endpoints while served begin/callback continues with
   the prior cached provider until expiry or an unknown-key refresh.

The second case is a regression from the base path, which had no platform
provider cache and rediscovered on every begin. It is also unlike the tenant
human candidate path: `HumanConnections::begin_test` calls the same owned
`RelyingParty::discover`, validates usable keys, and replaces the cache entry
before that candidate can become active (`connections.rs:326-383`).

### Authority reconciliation

`PERSIST-TEN-001` is **real but should be narrowed**.

- REQ-004 says setup discovers provider endpoints and JWKS and validates the
  issuer. TASK-009 requires human discovery/JWKS to use `openidconnect` and
  keeps the process-local human provider cache. The current platform write
  performs only the workload-specific half of that flow.
- R1 `FIND-TASK-009-3` expressly restores metadata-only discovery for the two
  workload owners: admin trusted-issuer creation and boot seeding. Platform
  human configuration is neither owner; sharing their helper is collateral
  behavior, not the intended closure.
- REQ-003's immediate hosted-change text is about tenant administration. It
  does not justify a new cross-replica cache-invalidation promise for platform
  configuration. TASK-009 also prohibits distributed cache coordination.
  Therefore the finding must not demand simultaneous refresh on every replica.

The conventional correction stays inside the existing owner: have platform
configuration perform a fresh full discovery through the process-owned
`PlatformLogin`/`RelyingParty::discover` before committing and use that
provider's `jwks_uri`. This validates discovery plus keys and replaces the
same process cache used by begin/callback. Keep the workload helper and both
workload callers metadata-only. No second cache, invalidation service, option,
probe, retry system, provider branch, or cross-replica mechanism is needed.

Focused proof should drive the served platform configuration route: an
unavailable/unusable JWKS refuses the replacement and leaves the previous row
intact; after priming an issuer, changing its metadata/keys and reconfiguring
that same issuer makes the next begin/callback on that process use the freshly
discovered entry. Existing workload tests must continue proving zero JWKS
requests during admin creation and boot seeding.

## Uncertainty 2: audience migration and rolling compatibility

### Migration ordering and old/new access

`20260601000023_platform_identity.sql` is already in mainline and creates
`expected_audience TEXT NOT NULL` independently from `client_id`. The base
reader selects both fields and the base upsert writes both. The base platform
verifier consumes `expected_audience`.

The candidate adds the next ordered migration,
`20261002000001_platform_oidc_client_audience.sql`, which immediately drops
`expected_audience`. Its reader and writer omit the column, and its verifier
derives the audience from `client_id`.

Consequently:

- migration first breaks every overlapping base read and write because the
  named column no longer exists;
- application first breaks candidate writes against the base schema because
  the omitted column remains `NOT NULL`; and
- for a pre-existing mismatch, a successful drop silently changes accepted
  trust from the stored audience to `client_id` and destroys the evidence
  needed to repair or diagnose that change.

The existing tenant human migration handles the same legacy mismatch
conventionally: it aborts with a non-secret repair instruction before moving
to the client-ID-derived representation
(`20260925000000_auth_human_connections.sql:57-66`). The deployment authority
also requires overlap writers to emit a representation both versions can read
and defers destructive contraction to a later release
(`deployment-and-release.md:163-168`). A separate new compatibility service,
toggle, migration mode, or public audience alias would be drift.

### Claim reconciliation

Both reported defects are **real and share one schema-evolution cause**, but
neither proposed correction is sufficient alone:

- `PERSIST-TEN-002` correctly identifies the silent trust reinterpretation and
  the need for mismatch preflight. Its proposed immediate drop would still
  violate the rolling deployment contract.
- `SYS-R2-001` correctly identifies that the immediate drop makes base and
  candidate SQL mutually incompatible. Merely retaining the column and making
  only the candidate writer derive it is incomplete: an overlapping base
  writer can still store a mismatch after migration because its public request
  carries both values.

The smallest conventional correction boundary is one expand release:

1. keep `expected_audience` physically present for the overlap window;
2. preflight existing rows and abort with the established non-secret repair
   instruction when `expected_audience <> client_id`;
3. enforce the overlap representation at the database boundary with the
   native equality invariant, so both base and candidate writers can produce
   only `expected_audience = client_id` after the preflight;
4. make the candidate upsert write that retained compatibility column from
   `client_id`, while the candidate request/view and verifier continue to have
   no independent audience input and consume `client_id`; and
5. drop the compatibility column only in a later contract release after the
   supported old application has drained.

This preserves R1 `FIND-TASK-009-6`: the candidate exposes no independent
human audience setting, and the retained column is a derived overlap
representation, not a second trust choice. The preflight, native database
constraint, dual-write, and later contraction are established migration
mechanics rather than Wyrd-specific machinery. No specification revision is
required.

Focused proof should cover both axes in one migration test: an equal-valued
base row upgrades, remains readable through the base projection, is readable
through the candidate projection, and receives candidate writes with the
retained value equal to `client_id`; a mismatched base row makes the migration
fail atomically with its prior row intact; and a post-migration base-shaped
mismatched write is refused by the database invariant. Contract/codegen and
served platform tests must continue proving the candidate wire has no
`expected_audience` input.

## Resolution

| Uncertainty | Resolution |
|---|---|
| `PERSIST-TEN-001` versus task/invariant PASS reports | **RESOLVED:** retain the finding, narrowed to platform setup/full discovery and same-process cache coherence. Do not infer or require cross-replica invalidation. Correction reuses the boot-owned `PlatformLogin` and its existing `RelyingParty::discover`; workload admin and boot stay metadata-only. |
| `PERSIST-TEN-002` versus `SYS-R2-001` | **RESOLVED:** consolidate them as one audience-schema evolution defect. Preflight-plus-drop is unsafe for overlap; retain-and-candidate-write alone does not constrain the base writer. Use preflight, a native equality invariant, a client-ID-derived compatibility write, and defer the drop to the later contract release. |
| Specification impact | **No specification revision required.** Both corrections preserve the approved client-ID-derived human audience, existing process-local cache boundary, workload verifier ownership, and public contract. |
| New proposed findings | **None.** This pass revises and consolidates the already proposed findings; it did not discover an unrelated defect. |

**Result: RESOLVED.**
