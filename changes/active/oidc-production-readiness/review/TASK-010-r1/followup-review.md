# Focused Follow-up Review — TASK-010-r1

## Immutable subject and scope

- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Routed authority: `changes/active/oidc-production-readiness/review/TASK-004-r2/lead-direction-routing.md` and the validated `FIND-TASK-004-12` diagnosis

The candidate remained `HEAD` and resolved to the immutable candidate throughout
this pass. `.codegraph/` is absent, so I used the cumulative diff, `rg`, and
direct source/caller inspection. I read all eight discovery reports and
investigated only the four assigned uncertainties. The approved API-key shape
is RFC 8693 token exchange with
`subject_token_type=urn:wyrd:oauth:token-type:api_key`; no
`grant_type=wyrd_api_key` alias was considered. Deleted `/internal/bff/v1`
consumer failures remain TASK-011 scope.

## Source paths inspected

- Callback, identity, connection lifecycle, issuance, and audit:
  `crates/wyrd/wyrd-auth/src/{callback,connections,issuance,audit,cli_logins}.rs`,
  `crates/wyrd/wyrd-sql/src/queries/auth/{human_connections,login_state,device_authorizations,role_assignments,users}.rs`,
  `crates/wyrd/wyrd-server/src/auth/callback.rs`, and
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs`.
- Device regression proof: the task and routed TASK-004 r2 finding,
  `wyrd-auth/src/cli_logins.rs`, `device_authorizations.rs`, and the device and
  connection-cutoff identity journeys.
- Rate limiting and topology:
  `crates/wyrd/wyrd-server/src/components/auth/routes.rs`,
  `crates/wyrd/wyrd-server/src/app/serve.rs`,
  `crates/wyrd/wyrd-server/src/http/router.rs`,
  `docker/official/extras/nginx/nginx.conf.template`,
  `architecture/operations/deployment-and-release.md`, `tower_governor` 0.8.0
  source, [RFC 8628 §5.1](https://www.rfc-editor.org/rfc/rfc8628#section-5.1),
  [NGINX `limit_req`](https://nginx.org/en/docs/http/ngx_http_limit_req_module.html),
  and [Keycloak distributed-cache documentation](https://www.keycloak.org/server/caching).
- Rustdoc dispute: the complete diff and current bodies of
  `crates/wyrd/wyrd-server/src/auth/oauth.rs` and
  `crates/wyrd-spec/src/auth/token.rs`, plus `AGENTS.md` §16 and
  `architecture/agent-rules.md`.

## Uncertainty 1 — callback lifecycle fencing and canonical audit

### TA-001: confirmed

The callback has a reachable post-check race, and the existing identity journey
already constructs it.

1. `AuthorizationCodeExchange::complete` consumes and commits login state
   before provider IO (`callback.rs:192-217`). After provider verification,
   `finish_id_token_exchange` calls `bound_connection`
   (`callback.rs:342-364`). `bound_connection` calls
   `HumanConnections::active_connection`, which opens and commits its own
   tenant transaction (`connections.rs:642-666`). That check therefore holds
   no lock into the callback's write transaction.
2. The callback then opens a different `TenantConn`, may create the User and
   identity binding (`callback.rs:365-378,585-609`), takes only the principal
   refresh-family lock, replaces the complete durable role set, conditionally
   appends `auth.user.roles.sync`, attaches an authorization code or writes a
   device approval, and commits (`callback.rs:379-442`). It neither takes
   `lock_human_connection_slot` nor rechecks the exact active connection in
   that transaction.
3. The connection writers serialize on that omitted lock. Candidate staging
   enters `begin_locked` (`connections.rs:273`); test stamping locks directly
   (`connections.rs:454-506`); activation, deactivation, and removal enter
   `begin_locked` (`connections.rs:530-628`); and `begin_locked` takes
   `lock_human_connection_slot` before the audited mutation
   (`connections.rs:703-719`). Human issuance uses the established
   principal-family-then-connection-slot order and checks the exact revision
   while both locks are held (`issuance.rs:702-755`). The callback is the
   sibling writer that does not participate.
4. `tenant_connection_session_cutoff_journey` deliberately locks
   `wyrd.auth_user_roles`, waits until the callback is blocked after its
   independent active-connection check, commits deactivation on the other
   replica, releases the callback, and then successfully extracts a newly
   issued code (`identity_e2e.rs:3422-3565`). The later redemption check proves
   only that this code mints no refresh row. It does not prevent or inspect the
   User creation, shared `(issuer, subject)` role replacement, conditional
   role-sync audit, or code write that the stale callback already committed.

This violates REQ-016's immediate cutoff of new login through a retired
connection and the task's prohibition on changing role semantics. A stale
callback can overwrite the shared User's current roles from the retired
mapping; a session established through the replacement can then observe those
roles on later issuance. The smallest correction is TA-001's existing-owner
correction: in the callback's final tenant transaction, after the existing
principal-family lock, take the existing connection-slot lock and recheck the
exact active binding before User/role/code/device-approval writes can commit.
No new lock, state, option, or protocol is needed.

### TA-002: confirmed

The callback transaction appends an audit row only when
`replace_user_roles` reports a mutation (`callback.rs:385-395`). With unchanged
roles it can commit a new authorization code or device approval with no
canonical audit append (`callback.rs:396-442`). The token-exchange and
device-grant audit rows occur only at later redemption
(`callback.rs:539-552`; `cli_logins.rs:358-383`), so an abandoned or expired
code/approval never produces them.

This is a candidate regression from moving session issuance out of the
callback: the base callback called `issue_human_session` in this transaction,
whose shared issuer appended the token-exchange audit. The candidate correctly
removed that premature issuance but did not replace its audit coverage with
the successful login-outcome record REQ-017 expressly requires. The existing
`changed_roles_are_audited_once_and_unchanged_roles_never` and injected
role-sync failure tests prove only conditional role-change auditing; they do
not prove a successful unchanged-role authorization or device login outcome,
or rollback when that outcome cannot be recorded.

The canonical append and callback transaction already exist, and comparable
identity systems audit successful logins, so TA-002 requires no novel
mechanism. Record the successful provider-login outcome in the same callback
transaction independently of role mutation, preserve the distinct role-sync
event when roles change, and prove both authorization-code and device approval
roll back when that append fails.

**Resolution:** the passing connection-revision and canonical-audit evidence
in the other reports covers redemption and issuance, not the callback's
separate pre-grant writes. `TA-001` and `TA-002` are both reachable and
task-required.

## Uncertainty 2 — routed device interleaving proof

`BEHAVIOR-TASK-010-2` is confirmed as a focused task-proof gap, not a full-
journey verification limit.

The source correction is credible: callback approval updates only a still
unexpired, undenied, undecided device row (`device_authorizations.rs:58-69,
208-227`), while redemption locks that row, deletes it, issues, audits, and
commits once (`cli_logins.rs:310-384`). Current tests prove sequential
denial/expiry and direct approval/exactly-once redemption
(`cli_logins.rs:760-883`; `identity_e2e.rs:2016-2138`). They do not exercise
the still-present gap between `CliLogins::approve`'s initial lookup/commit and
`begin_bound` (`cli_logins.rs:192-215`): no test pauses there, ends or
poll-deletes the device authorization from another actor, resumes provider
completion, and inspects approval and refresh/session state.

That exact interleaving is not an optional full sweep. TASK-010 Scenario 2
names it as RED, its acceptance criterion expressly absorbs
`FIND-TASK-004-12`, and the routed finding's focused closure proof prescribes
it. The standing narrowest-lane direction therefore requires one focused
Postgres/server regression proof here; it defers only the broad unfiltered and
every-language journeys to change review. The connection-deactivation journey
above is a different interleaving and cannot close device terminal-state
behavior.

**Resolution:** source closes the known production mechanism, but TASK-010
cannot claim PASS without the focused deny/delete/expiry-versus-in-flight-
approval proof already required by the task. `BEHAVIOR-TASK-010-2` remains a
material `MISSING` proof finding; it requires no new harness, lease, marker,
store, or state machine.

## Uncertainty 3 — shared auth governor and deployed rate limiting

The system report's reachability diagnosis is confirmed, but its
classification should be revised to **DRIFT** under the standing direction.
Its correction boundary is viable only as a two-part correction: deleting the
server-local governor alone would leave the task's RFC 8628 obligation open.

- `auth_router` applies one `GovernorLayer` to authorization, metadata, device
  authorization and verification, callback, token/refresh/device polling,
  revocation, and API-key issuance (`routes.rs:46-84`). This is broader than
  RFC 8628 §5.1, which calls for limiting attempts to verify the short
  `user_code`; it does not define one admission bucket for unrelated OAuth
  operations.
- `GovernorConfigBuilder::default()` in the locked `tower_governor` 0.8.0
  source selects `PeerIpKeyExtractor`. `serve` supplies the TCP peer through
  `ConnectInfo<SocketAddr>` (`app/serve.rs:15-24`). Behind the official NGINX
  gateway the peer is therefore the gateway, not the client. NGINX forwards
  `X-Real-IP` and `X-Forwarded-For` (`nginx.conf.template:44-58`), but this
  extractor intentionally ignores them. All external clients consequently
  share one bucket on each selected backend replica.
- Each `auth_router()` constructs its own in-memory governor, so backend
  replicas have independent buckets. Requests routed to a different replica
  bypass the previous bucket, while traffic sharing one replica can exhaust
  the bucket and refuse unrelated tenants' token, revoke, callback, metadata,
  and issuance requests. The candidate journey helper retries every such
  `429` (`identity_e2e.rs:1229-1258`), so its green paths mask the admission
  behavior.
- The supported topology explicitly places replicas behind one gateway, and
  that gateway owns rate-limit integration
  (`deployment-and-release.md:8-24`). The current official gateway config has
  no rate limiter. NGINX's native `limit_req_zone` is a shared-memory,
  route-scoped conventional mechanism, and comparable clustered identity
  servers such as Keycloak keep brute-force state in the database or a
  distributed cache rather than independent per-replica counters.

The current all-auth, TCP-peer, replica-local mechanism is therefore unearned
DRIFT and has the stated reachable denial/bypass consequences. The existing
gateway boundary can meet the approved task without a new architecture
decision: repository authority already assigns rate-limit integration there,
and the official gateway has a native shared limiter. The correction must
remove the shared server governor and apply the conventional limiter only to
device user-code verification at the gateway's real-client-address boundary,
with proof through that gateway across two backend replicas. It must not add a
new public option, server-local replacement, distributed Wyrd store, or common
bucket for unrelated auth routes. Deployment-specific hosted gateways provide
the same existing integration boundary.

**Resolution:** `FIND-SYS-1` is reachable and retained with classification
revised from `INCORRECT / REGRESSION` to `DRIFT`. Its prescribed gateway owner
is real and already authoritative; deletion without the route-specific gateway
limit would be incomplete, but no specification revision or new architecture
decision is required.

## Uncertainty 4 — mandatory rustdoc

`REPO-TASK-010-1` is confirmed at all four cited sites. The maintainer report's
contrary statement is not supported by the diff.

- `crates/wyrd/wyrd-server/src/auth/oauth.rs` is wholly new. Its public tuple
  struct `OAuthError` documents the struct but not its public tuple field
  (`oauth.rs:30-32`).
- The new `FromRequest` implementation leaves its associated
  `type Rejection` undocumented (`oauth.rs:186-187`).
- The new fallible `from_request` has only a one-line operation description
  and no `# Errors` section despite refusing content type, body extraction,
  and form parsing (`oauth.rs:189-208`).
- The new fallible test helper `parse` has rustdoc but no `# Errors` section
  (`wyrd-spec/src/auth/token.rs:249-264`).

AGENTS §16 explicitly includes fields, associated types, functions, test
helpers, and tests, and requires `# Errors` on every fallible function;
`architecture/agent-rules.md` makes omissions `BLOCK_BEFORE_MERGE`. These are
not merely placement, naming, structure, or wording observations. The skill
also explicitly preserves missing rustdoc as blocking. No other rustdoc site
is added by this focused dispute.

**Resolution:** `REPO-TASK-010-1` remains a blocking repository-rule finding.
The correction is documentation-only and requires no behavior or abstraction.

## New proposed findings

None. This pass confirms or revises only the assigned discovery claims.

## Overall result

**RESOLVED** — all four uncertainties resolve from the immutable source and
approved authority: TA-001 and TA-002 are valid callback findings;
BEHAVIOR-TASK-010-2 requires the task-named focused interleaving proof;
FIND-SYS-1 is reachable DRIFT with the existing gateway as the complete
correction boundary; and every site in REPO-TASK-010-1 violates the hard
rustdoc rule.
