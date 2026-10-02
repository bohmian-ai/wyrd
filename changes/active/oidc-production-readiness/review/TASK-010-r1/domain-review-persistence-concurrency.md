# Persistence and Concurrency Domain Review

## Immutable Subject

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Routed prior finding: `FIND-TASK-004-12`

The candidate remained at the stated commit while this review was performed.

## Reviewed Boundary

I reviewed the complete cumulative diff, then traced the persistent grant-state
boundary through every writer, reader, terminal transition, transaction owner,
and sibling consumer relevant to this domain:

| Durable authority | Writers and transitions reviewed | Readers and consumers reviewed | Result |
|---|---|---|---|
| `wyrd.auth_login_state` | login insertion, callback consumption, authorization-code attachment, code redemption, expiry purge | OIDC callback, `AuthorizationCodeExchange`, device login binding | PASS |
| `wyrd.auth_device_authorizations` | create, deny, callback approval, poll timestamp, terminal deletion | verification-page approval, token-endpoint polling and redemption | PASS |
| `wyrd.auth_refresh_tokens` | first issuance, CLI rotation, UI non-rotating renewal, replay containment, logout/revocation | refresh and revoke endpoints, connection replacement/deactivation | **FAIL** (`PERSIST-CONC-001`) |
| Human connection revision and slot state | active-connection replacement/deactivation and revision binding | callback completion, authorization-code redemption, device redemption, refresh | PASS |
| Canonical audit staging | grant, rotation, replay containment, refusal, and device-grant audit appends | transaction commit/rollback and durable replay tests | PASS except for the overbroad event caused by `PERSIST-CONC-001` |
| Auth migrations and RLS | clean schema creation, constraints, forced tenant policy, unreleased migration editing | current SQL query shapes and clean-database migration tests | PASS |

This included the changed migrations and row types; `login_state.rs`,
`device_authorizations.rs`, `refresh_tokens.rs`, and human-connection queries;
`callback.rs`, `cli_logins.rs`, `refresh.rs`, `issuance.rs`, `revoke.rs`, and
`exchange_api_key.rs`; the token-grant routes; and their PostgreSQL and identity
tests. I also searched all callers of the reviewed query capabilities rather
than treating their local modules as complete coverage.

## Authority and Source Coverage

| Authority | Domain obligation checked |
|---|---|
| Spec revision 11, REQ-011 and REQ-012 | mint device credentials only at live redemption; public-client refresh rotation and family-scoped reuse containment; confidential-client bounded non-rotating refresh |
| TASK-010 scenarios 1-5 and acceptance criteria | one-use bound codes, terminal device behavior, exactly-once device issuance, client-specific refresh behavior, transactional audit, deletion-only consumer boundary |
| TASK-004 r2 routing and `FIND-TASK-004-12` diagnosis | preserve the device authorization as the sole authority through token-endpoint redemption and leave no session/refresh authority after a terminal grant |
| `AGENTS.md`, `architecture/agent-rules.md`, Rust/SQL/testing references | `TenantConn`/RLS ownership, database timestamps, transactional durable writes, smallest scoped verification, and no new nonstandard machinery |
| `architecture/wyrd-security-posture.md:141-151` | digest storage, rotation on successful public-client use, and reuse of a **rotated** token revoking its token family |
| RFC 6749 §4.1 and §6; RFC 7636 §4.6 | code binding/single use and refresh grant behavior |
| RFC 8628 §3.4-§3.5 | the token endpoint owns device-code validation, terminal outcomes, and successful token response |
| RFC 9700 §4.14.2 | rotation retains the relationship between tokens; replay of an invalidated rotated token revokes the active token(s) belonging to that authorization grant |

The approved API-key decision was applied as an input boundary: the retained
API-key path is RFC 8693 token exchange with
`subject_token_type=urn:wyrd:oauth:token-type:api_key`; no legacy
`grant_type=wyrd_api_key` alias is required. Failures caused only by deletion of
`/internal/bff/v1` were excluded as directed because TASK-011 owns those
consumers. I did not treat placement, naming, structure, or wording alone as a
blocking issue.

## Prior Finding Closure

`FIND-TASK-004-12` is **closed at its root cause** in this candidate.

- The provider callback now records only an approval. Its update requires the
  device row to remain unexpired, undenied, and undecided
  (`crates/wyrd/wyrd-sql/src/queries/auth/device_authorizations.rs:58-69`), and
  the callback commits no Wyrd token or refresh row
  (`crates/wyrd/wyrd-auth/src/callback.rs:396-442`).
- The token endpoint locks the durable device row with `FOR UPDATE`
  (`crates/wyrd/wyrd-sql/src/queries/auth/device_authorizations.rs:71-89`),
  classifies terminal state, and deletes terminal grants
  (`crates/wyrd/wyrd-auth/src/cli_logins.rs:322-356`).
- A live approved grant is deleted, the human session and refresh row are
  issued, the allowed device-grant audit is appended, and the transaction is
  committed as one unit (`crates/wyrd/wyrd-auth/src/cli_logins.rs:358-384`).
  An issuance, audit, cancellation, or database failure before commit rolls the
  delete and all issuance writes back. Concurrent polls serialize on the row;
  after the winning transaction commits, the loser can no longer find it.
- `an_approved_device_code_issues_exactly_once`,
  `device_codes_poll_approve_deny_and_expire`, and
  `device_grant_refusal_journey` exercise the one-use and terminal outcomes.

The earlier gap between the approval-page lookup and beginning provider login
can still leave a bounded login-state row if another actor ends the device
grant in between. It cannot produce session or refresh authority: the callback
must update the still-live device row in the same tenant transaction before it
commits, and the failed predicate rolls back the associated user/role/audit
writes. That satisfies the prior finding's deliberately narrowed authority
boundary without adding a lease, journal, cleanup service, or parallel grant.

## Material Proposed Findings

### PERSIST-CONC-001 — DRIFT: refresh replay containment revokes unrelated grants and treats ordinary expiry/revocation as theft

- **Classification:** `DRIFT`
- **Violated obligation:** TASK-010 lines 36-48 require exactly the listed RFC
  behavior and nothing more; Scenario 3 requires RFC 9700 §4.14.2 rotation
  and family revocation on reuse for the public client. REQ-012 and
  `architecture/wyrd-security-posture.md:149-151` likewise scope containment to
  reuse of a rotated refresh token and its token family. RFC 9700 §4.14.2
  retains a relationship between rotated tokens and describes revocation of
  active refresh tokens belonging to the affected authorization grant, not all
  renewable sessions owned by the principal.
- **Exact location:**
  `crates/wyrd/wyrd-auth/src/refresh.rs:136-146,166-212` and
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:245-273`. The
  candidate's own assertions of the extra behavior are at
  `crates/wyrd/wyrd-auth/src/refresh.rs:638-688,772-817`.
- **Evidence:** `refresh_by_hash` deliberately returns inactive rows. After
  taking the principal advisory lock, every row that
  `consume_active_refresh` cannot consume is classified as `Reused`, including
  a row expired by time, revoked by logout, administratively revoked, or
  already revoked by earlier containment. `revoke_refresh_family` then updates
  every active row with the same `(principal_kind, principal_id)`. It does not
  follow the stored `rotated_from` relationship and does not restrict by client
  or login. The test `reuse_detection_revokes_family` constructs an unrelated
  active sibling with no chain relationship and requires its revocation; the
  test `expired_token_is_rejected` requires a merely expired row to enter the
  theft path. By contrast, the already-existing
  `revoke_refresh_chain` capability at
  `crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:207-243` follows
  `rotated_from` and explicitly preserves the principal's other browser, CLI,
  and SDK login chains.
- **Observable consequence:** replaying a stale CLI token from one login
  revokes every unrelated active CLI login and every non-rotating `wyrd-ui`
  refresh token for that User. Because inactive rows remain addressable by
  hash, presenting an old expired or logged-out token after a later fresh login
  can repeatedly revoke that new login too. A bearer holding one dead token can
  therefore cause durable denial of renewal outside the compromised grant,
  while the server records an RFC-style theft event even when no rotation
  replay occurred.
- **Testable correction:** keep the existing per-principal advisory lock so a
  concurrent rotation cannot escape containment, but classify the persisted
  row after acquiring it. Only a predecessor invalidated by successful
  rotation (`revoked_reason = 'rotated'`) enters replay containment. Reuse the
  existing `revoke_refresh_chain(stored.id, "reuse_detected")` relationship
  walk to revoke the affected grant's current descendants. Expired,
  logout-revoked, administratively revoked, or otherwise inactive non-rotated
  rows return the ordinary invalid-grant/`NotFound` refusal without a
  containment write or theft audit. Do not add a family identifier, table,
  option, background job, lease, or new state machine.
- **Focused closure proof:**
  1. Create CLI chain `A -> B` and independent CLI/UI session `C`; replaying
     `A` revokes `B`, while `C` remains refreshable.
  2. Present a never-rotated expired token and a logout-revoked token; each is
     refused without revoking an independent current or subsequently created
     login and without a replay-containment audit.
  3. Retain the existing concurrent ancestor-replay-versus-rotation proof:
     replay of `A` waits for `B -> C` to commit, then revokes `C`, with exactly
     one containment audit naming `A`.

## Verification and Limits

- I inspected the candidate's recorded narrow evidence for
  `mise run test:principals:integration`, `mise run test:sql`, the exact
  `wyrd-auth` callback/device/refresh/issuance suites, identity journeys,
  codegen, client-tier, unwrap audit, formatting, and lints. The task records
  those lanes as green.
- I did not rerun Cargo-backed commands during the parallel review because all
  reviewers share the repository target directory and repository instructions
  require those commands to be serialized. This review therefore validates
  the recorded results against test source rather than claiming an independent
  execution.
- Full unfiltered and every-language journeys are intentionally deferred to
  change review under the task's narrowest-lane rule. The `openid-client` BFF
  journey and consumers broken only by removal of `/internal/bff/v1` remain
  TASK-011 scope.
- The device tests prove terminal and exactly-once outcomes, but there is no
  separately named deterministic two-poller or pause-after-approval-lookup
  test matching every interleaving from the earlier diagnosis. The row lock,
  guarded approval update, single transaction, and rollback path are directly
  visible in source, so this is a verification limit rather than a material
  finding.
- Green refresh tests do not resolve `PERSIST-CONC-001`; two of them explicitly
  assert the nonstandard overbroad behavior.

## Overall Result

**FAIL** — `FIND-TASK-004-12` is closed and the authorization-code/device
transaction boundaries are durable, but `PERSIST-CONC-001` is a reachable,
behavioral RFC drift in the task's refresh-grant boundary.
