# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `e3a47a05d931c010f4c70c75edea2d23c447108b`
- Task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Candidate stability: `HEAD` and the requested candidate both resolved to
  `e3a47a05d931c010f4c70c75edea2d23c447108b` at the end of this review.

## Deployed-path evidence

The cumulative diff changes public/architecture documentation, generated
Python and TypeScript declarations, CLI help, and Rust/TypeScript/PyO3 source
documentation. It does not change a runtime expression, SQL statement, route,
deployment manifest, or process-lifecycle branch. The deployed behavior being
documented remains owned by the pre-existing server, BFF, and shared client:

- Tenant login and connection administration run in every API-serving
  `wyrd-server` replica over shared Postgres. `HumanConnections::active_connection`
  reads the Active connection durably on every call
  (`crates/wyrd/wyrd-auth/src/connections.rs:642-666`), while activation takes
  the connection-slot lock and atomically retires/promotes rows
  (`connections.rs:530-584`). This supports the new claim that committed
  lifecycle changes become visible without restarting replicas.
- The SvelteKit BFF is the confidential `wyrd-ui` client. Its encrypted
  HttpOnly cookie contains the refresh token or recovery API key, while access
  tokens are held only in a bounded per-replica memory cache
  (`crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts:99-106,245-278`).
  On a replica miss or restart, the cookie survives and the BFF renews through
  the server (`browser-sessions.ts:281-331`).
- BFF logout deletes the cookie and local cached access token before attempting
  RFC 7009 revocation; revocation failure is logged without token material and
  does not fail logout (`browser-sessions.ts:333-358`). This matches the
  approved best-effort logout boundary and preserves the documented fact that
  an already issued access token remains usable until expiry.
- Human refresh state is durable in Postgres. `wyrd-ui` presents the same
  client-authenticated refresh row and receives only a new access token;
  `wyrd-cli` consumes and rotates its row, with reuse containment under the
  family lock (`crates/wyrd/wyrd-auth/src/refresh.rs:100-220`). Both paths
  re-check the exact connection revision before issuance
  (`crates/wyrd/wyrd-auth/src/issuance.rs:706-755`), so replacement,
  deactivation, and removal cut off renewal across replicas without taking
  unrelated machine authentication offline.
- Sealing-key rewrap is a boot-time, cross-tenant pass over tenant human
  connections, workload issuers, and the platform connection. It is
  idempotent, uses compare-and-swap for concurrent writers, leaves already
  changed rows for a later pass, and reports only counts and row identity
  (`crates/wyrd/wyrd-auth/src/sealing.rs:1-26,70-118,121-161`). With retained
  keys, a failed rewrap is retried on a later boot while serving continues;
  keyless boot fails readiness when any sealed value remains
  (`crates/wyrd/wyrd-server/src/boot/mod.rs:1525-1559`).
- Shared-client saved logins live in the local credential file. Selection and
  renewal are process-local client concerns; file locking serializes a public
  client's refresh rotation, and logout removes the local record before the
  best-effort server call (`crates/shared/wyrd-client/src/saved_login.rs:1-17`).
  The candidate's Rust, Python, and TypeScript edits only clarify that existing
  behavior.

## Failure and recovery assessment

| Failure or interruption | What stops | What remains available | Recovery and proof |
| --- | --- | --- | --- |
| IdP discovery/JWKS/token endpoint outage | New human login, candidate testing, and any BFF/CLI renewal that needs the provider or Wyrd login path fail closed. | Existing access tokens remain valid until expiry; machine API-key and workload grants remain independent; operator API-key recovery remains available. | Restore the provider and retry login/test. The docs correctly state no fallback to another tenant or platform connection. |
| One BFF replica restarts or loses its access-token cache | That replica incurs a refresh/exchange on the next request. | The encrypted cookie is client-carried and readable by replicas configured with the same secret; server refresh authority remains durable. | Normal request traffic repopulates the cache; no server-side browser-session recovery is needed. |
| Logout revocation cannot reach Wyrd | Server-side refresh revocation may not occur. | Local browser/CLI login is already removed; other logins and unrelated capabilities remain available. | No retry is required for logout completion; the abandoned refresh authority expires or can be administratively revoked. Existing access tokens expire normally. |
| Connection replacement/deactivation commits | New login and subsequent refresh through the retired revision stop on every replica. | Already issued access tokens retain their five-minute snapshot authority; machine identities and explicit recovery credentials remain available. | Sign in through the new Active connection, or use the documented recovery API key. |
| Server crashes during sealing rewrap | The current pass stops; already committed per-value swaps remain. | With the old key retained, serving can still open old and new ciphertext. | The next boot resumes the idempotent pass. Retire the old key only after a post-roll pass reports `remaining = 0`. |
| No sealing key while ciphertext exists | The affected server never becomes ready. | No unsafe plaintext or fallback path is enabled. | Restore a held key or re-enter the affected secret; boot then rechecks the complete sealed-secret set. |

The candidate does not introduce a new retry loop, readiness dependency,
process crash path, durable state transition, or cross-replica cache. Existing
healthy and recovery paths are therefore unchanged. One new operator statement
misdescribes the activation failure boundary, however.

## Affected capabilities

- Human OIDC connection setup, candidate testing, activation, login, refresh,
  logout, and operator recovery documentation.
- BFF multi-replica cookie/session behavior and `wyrd-ui` client-secret
  rotation documentation.
- CLI device login and saved-login selection/renewal documentation across
  Rust, Python, and TypeScript.
- Sealing-key boot/restart/rotation runbooks.
- Machine authentication remains documented as independent and no executable
  machine path changed.

## Material proposed findings

### SYS-001 — INCORRECT: activation documentation promises a provider-liveness gate that does not exist

- **Violated obligation:** TASK-005 requires documentation to match shipped
  standard flows and specifically requires accurate connection lifecycle,
  login-failure, and recovery guidance. The human direction makes wrong or
  misleading documentation blocking.
- **Location:** `docs/src/content/docs/self-hosting/sso-and-oidc.svx:117-119`.
- **Evidence:** The guide says that “a provider outage” makes activation fail
  closed with `WYRD_AUTH_409_CONNECTION_NOT_TESTED`. In production,
  `HumanConnections::activate` performs no provider discovery, JWKS, or token
  request. Under the slot lock it checks only that the candidate revision has
  an unexpired persisted test stamp and that the recovery API key authorizes,
  then promotes it (`crates/wyrd/wyrd-auth/src/connections.rs:530-576`). The
  stamp remains valid for 15 minutes
  (`connections.rs:68,429-494`; `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:323-335`).
  A provider can therefore become unavailable after a successful test and the
  still-current candidate can be activated.
- **Observable system consequence:** An operator following the runbook can
  believe activation freshly checks provider availability, activate during a
  post-test outage, retire a working Active connection, and make routine human
  sign-in unavailable. The required recovery key prevents an unrecoverable
  lockout, but it does not make the documented liveness guarantee true.
- **Testable correction:** Correct this documentation-only statement to say
  that a missing, stale, or failed test stamp blocks activation; a provider
  outage blocks a new test and later login but is not re-probed during the
  15-minute activation window. Preserve the existing tested-revision and
  recovery-key guarantees. Run only `mise run docs:check` (and
  `git diff --check`) for this correction.

## Verification and limits

- Inspected the complete base-to-candidate diff and the runtime owners cited
  above. `git diff --check 134f605367e65b41f1977d6c70ac8ca8b277a69e
  e3a47a05d931c010f4c70c75edea2d23c447108b` passed.
- The task records successful narrow lanes: `docs:check`, `codegen:check`,
  `ts:napi:check`, `fmt`, `lints`, `py:format`, and `py:lints`. Those results
  were available as task evidence and were not rerun by this independent
  static review.
- Per the task and human direction, no journey suite or broad aggregate was
  run or required. Existing owner-journey evidence is adequate for unchanged
  runtime paths, but static generators cannot detect the incorrect operational
  claim in SYS-001.
- The documented BFF client-secret rotation intentionally invalidates session
  cookies because the cookie key is derived from that secret. I did not require
  a dual-key cookie mechanism: the guide discloses reauthentication, and adding
  another session-key lifecycle would exceed this documentation task and the
  standards-first direction.

## Overall result

**FAIL**

The candidate has no executable or deployment-topology regression, and its
runtime/recovery descriptions are otherwise consistent with the inspected
owners. SYS-001 is a bounded but material documentation defect because it
promises a provider-outage activation refusal the deployed system does not
perform.
