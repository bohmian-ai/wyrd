# TASK-010 R3 independent findings validation

## Immutable subject and validation completeness

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- R1 remediation:
  `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Current remediation authority:
  `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`

The candidate resolved to the stated commit before and after validation. The
repository has no `.codegraph/` directory, so validation used the immutable Git
range and current source directly. I read the complete cumulative diff, the R1
and R2 verdicts and validated ledgers, both prior-finding directions, the R1
remediation, the superseded R2 task, every R3 discovery report, the applicable
repository and architecture authorities, and the recorded narrow verification
evidence. All required R3 reports are present. No discovery report proposed a
material finding, no material claims conflict, and the source traces below
leave no reachable task path unexplored; a `followup-rev` was therefore not
needed.

The controlling human directions are applied as authority, not proposals:

- `FIND-TASK-010-1` remains routed to TASK-011. A second TASK-010-only
  `openid-client` driver would duplicate the production BFF proof and is not
  reopened here.
- The `FIND-TASK-010-10` lead direction supersedes
  `TASK-010-R2-public-edge-device-admission.md`. Closure is deletion of the
  image-local limit and its assertions plus one operator instruction to limit
  `POST /auth/device` per client address at the public ingress. It expressly
  excludes an application limiter, forwarded-header parser, edge manifest,
  setting, or persistent state.
- Placement, naming, structure, and wording observations have no blocking
  force. Full journeys remain change-review evidence; this validation assesses
  the task's narrow owner-lane evidence.

## Independent source and reachability trace

I independently traced the finding union, including the empty-union claim,
through these producer-to-consumer paths:

1. `authorize` uniquely resolves `wyrd-ui` and its registered redirect before
   parsing non-binding parameters. It stores `ClientAuthorization` through the
   existing human-connection/login-state owner. The provider callback consumes
   that state once, verifies the provider result, fences the exact active
   connection in the final tenant transaction, writes either a digest-only
   authorization code or device approval, appends `auth.login`, and commits.
   `AuthorizationCodeExchange::redeem_code` is the only code consumer: it
   deletes by digest under RLS, verifies the stored client, exact redirect,
   expiry, and S256 challenge, and issues through `TenantTokenIssuer`.
2. `device_authorization`, `device_decision`, the callback, and the device-code
   token grant all share `CliLogins` and one SQL row. The only writers are
   create, guarded approval, denial, cadence-aware locked poll, and deletion.
   The callback writes no token. `redeem_in` locks and revalidates the row,
   deletes it, issues one session, appends canonical audit, and commits in one
   tenant transaction. The deterministic denial and expiry/delete overlap
   tests exercise the producer race; the exactly-once and RFC refusal tests
   exercise the sibling terminal consumers.
3. The token, device-authorization, and revocation handlers share `OAuthForm`
   and `OAuthClients`. The former rejects repeated or non-form parameters and
   performs the RFC 8693 unsupported-audience classification before generic
   decoding. The latter has three real handler callers, accepts the standard
   case-insensitive Basic scheme for the confidential client, and admits the
   public client only by `client_id`. `ClientForm<T>` is used by all three
   served OpenAPI operations solely to project those standard alternatives; it
   creates no second runtime parser.
4. Refresh reaches one owner: token handler -> `TokenGrants::refresh` ->
   `RefreshTokens::execute`. `refresh_by_hash` identifies the immutable family
   under RLS, `lock_refresh_family` serializes classification and rotation,
   `active_refresh` leaves a live confidential-client row unconsumed,
   `consume_active_refresh` rotates the public client, and only a predecessor
   marked `rotated` enters `revoke_refresh_chain`. RFC 7009 logout is the only
   sibling chain writer. Administrative principal revocation deliberately uses
   the separate family-wide owner and is not a replay fallback.
5. Callback, code/device redemption, refresh containment, revocation,
   API-key exchange, delegation, and issuance all reach the existing
   `append_auth_audit` -> `vala.audit_staging` owner on their `TenantConn`.
   No alternate audit table, relay, log sink, tenant selector, raw pool, or
   client-owned durable path was added.
6. The final remediation delta deletes the device `map`, `limit_req_zone`,
   `limit_req_status`, `limit_req`, and their one-image assertions. The
   bundled NGINX remains a local reverse proxy behind the operator's public
   ingress. The self-hosting page names the exact anonymous user-code entry
   route, RFC 8628 section 5.1, the per-client-address key, and the operator's
   public-ingress boundary. Repository search finds no replacement limiter,
   trusted-header mechanism, edge artifact, option, state, or rate-limit
   dependency.
7. The private BFF route owner, browser-session owner and query module,
   unreleased browser-session migrations, sealed completion, and narrowed
   rewrap columns remain deleted. The remaining UI source/test references are
   the expressly routed TASK-011 consumers. Restoring a compatibility route or
   adding a duplicate TASK-010 client would violate the task rather than close
   it.

## Ponytail and drift validation

The cumulative change survives the deletion-first ladder:

- The custom BFF/session/completion protocol and its persistence were deleted
  in favor of RFC 6749 authorization code + PKCE, RFC 7009 revocation, and the
  existing refresh-token authority.
- The application/image rate-limit mechanisms and their dependencies are
  deleted. Operator ingress documentation is the only conventional mechanism
  retained by the controlling direction and comparable-project practice.
- `oauth2 = 5.0.0` is an exact, default-feature-free server dev dependency with
  a real identity-journey caller; it supplies the task-required off-the-shelf
  device and refresh proof and adds no production abstraction.
- `OAuthClients`, `TokenGrants`, and the existing auth-domain owners each own
  real state or dependencies and multiple operations. No new trait, generic
  extension point, feature, factory, retry layer, compatibility path, or
  configuration knob exists without a current caller.
- Postgres RLS, row/advisory locks, transactions, uniqueness, and database time
  enforce the durable invariants. No lease, cache, recovery worker, secondary
  tenant predicate on the new confidential lookup, or parallel state machine
  was introduced.
- OAuth/OIDC/JWT behavior remains limited to the approved RFC profiles and the
  installed `openidconnect`, `oauth2`, and shared external-verifier owners. No
  extra grant, document, token profile, verification rule, or application
  security mechanism entered the candidate.

I found no mechanism, check, file, setting, or option unsupported by the named
standards or comparable widely used projects that remains in the task's
implementation and should be deleted as `DRIFT`.

## Disposition of discovery claims and non-blocking concerns

| Source claim or concern | Disposition | Independent source-based reason |
| --- | --- | --- |
| All nine discovery reports: no material proposed finding | **CONFIRMED** | Complete caller, sibling-consumer, writer, persistence, deployment, and verification traces found no reachable behavioral, security, tenancy, durability, public-contract, regression, or deletable-drift defect. |
| `task-review-invariants.md`: old TASK-011 UI consumers remain | **REJECTED as a TASK-010 finding** | The task assigns those consumers to TASK-011 and forbids a compatibility server path here. The deleted server owners remain absent. |
| `standards-review.md` / `domain-review-security-oauth.md`: trailing blank line in the prior R2 verdict | **REJECTED** | Review-artifact formatting has no executable or contract consequence; placement and wording do not block. |
| `standards-review.md`: pre-existing fully qualified signatures and older explicit SQL tenant predicates | **REJECTED** | The changed confidential `active_refresh` lookup relies on RLS alone. Unmodified source shape elsewhere is neither introduced by this task nor required for its acceptance. |
| `maintainer-review.md`: `auth_router` rustdoc says the gateway sees a fleet-wide client budget | **REJECTED** | This is wording only. Runtime and deployment artifacts own no such gateway mechanism, while the controlling operator documentation accurately assigns it. Wording findings cannot block or justify new machinery. |
| `maintainer-review.md`: fully qualified `HumanSessionBinding::client` and `ServerAuth::oauth_clients` field types | **REJECTED** | The types and owners are unambiguous and behaviorally correct; this is a placement/structure observation with no accepted consequence. |
| `maintainer-review.md`: `ClientForm<T>` is separate from the runtime extractor | **REJECTED** | It has three served-contract consumers and exists because the runtime accepts mutually exclusive form and Basic client identification. Deleting it would make the public contract incomplete; turning it into another runtime parser would duplicate behavior. |
| `maintainer-review.md`: long callback and refresh methods | **REJECTED** | Each method deliberately keeps one transaction's lock, state-transition, issuance, and audit order visible on its cohesive owner. Splitting by line count would add delegation without a second responsibility. |
| `maintainer-review.md`: operator note is brief | **REJECTED** | It contains the endpoint, RFC section, key, and owner required by the lead direction. Adding examples, manifests, settings, or Wyrd edge policy would be unapproved drift. |
| R2 discovery proposals to restore a Vault/multi-tenant startup topology | **REJECTED; remains closed** | The startup lane provisions exactly one tenant and now selects the supported single-tenant file-KEK profile it actually exercises. Existing focused config/Postgres tests own the separate multi-tenant fail-start invariant. |

No proposal is revised into a new finding. Rejected placement, naming,
structure, wording, optional-hardening, and unrelated-debt observations are
omitted from the final ledger.

## Prior-finding closure validation

| Stable finding | Independent closure evidence | Result |
| --- | --- | --- |
| `FIND-TASK-010-1` | Human direction assigns the one production `openid-client` 6.8.8 BFF journey to TASK-011/change review. This candidate adds no duplicate driver or compatibility route. | **ROUTED — not reopened** |
| `FIND-TASK-010-2` | Guarded `approve_device_authorization`, locked terminal transitions in `redeem_in`, and deterministic `a_denial_during_approval_wins` / `an_expiry_deleted_during_approval_wins` tests prove terminal state defeats a racing callback. | **CLOSED** |
| `FIND-TASK-010-3` | `OAuthForm::token_request` detects a syntactically present unsupported token-exchange `audience` before generic decoding and returns `invalid_target`; malformed exchanges remain `invalid_request`. | **CLOSED** |
| `FIND-TASK-010-4` | `authorize` first extracts one `client_id` and exact registered `redirect_uri`; only then can duplicate non-binding parameters redirect as `invalid_request`, while duplicated or mismatched binding fields stay local. | **CLOSED** |
| `FIND-TASK-010-5` | `CallbackQuery::response` accepts exactly one provider code or error. `AuthorizationCodeExchange::complete` consumes the same state once, verifies response issuer, maps refusal without reflecting provider detail, and uses the stored downstream redirect/state. | **CLOSED** |
| `FIND-TASK-010-6` | `OAuthClients::identify` splits the Authorization value once and compares the scheme with `eq_ignore_ascii_case("basic")` before the existing credential decoder and digest comparison. | **CLOSED** |
| `FIND-TASK-010-7` | The previously cited tuple field, extractor rejection type, fallible extractor, and test parser now have substantive rustdoc and required error documentation. No suppression replaces the rule. | **CLOSED** |
| `FIND-TASK-010-8` | `ACTIVE_REFRESH_SQL` binds only the token hash and depends on the caller's forced-RLS `TenantConn`; its focused cross-tenant same-hash test exercises that authority. | **CLOSED** |
| `FIND-TASK-010-9` | `ClientForm<T>`, the `oauthClientBasic` security scheme, and the token/device/revoke annotations expose public-form and confidential-Basic alternatives in the served document; the owning OpenAPI integration test asserts all three. | **CLOSED** |
| `FIND-TASK-010-10` | The superseding lead direction is implemented exactly: image-local limiter and assertions deleted, operator ingress responsibility documented, and no replacement Wyrd mechanism, option, state, or edge artifact added. | **CLOSED BY LEAD DIRECTION** |
| `FIND-TASK-010-11` | `RefreshTokens::execute` treats only a stored predecessor whose reason is `rotated` as reuse and calls `revoke_refresh_chain`; expiry, logout, administration, and already-contained rows refuse without principal-wide containment. | **CLOSED** |
| `FIND-TASK-010-12` | Final callback completion takes the principal-family lock, then `lock_human_connection_slot`, and verifies the exact active revision before roles, code/approval, login audit, and commit. A losing lifecycle race rolls the entire transaction back. | **CLOSED** |
| `FIND-TASK-010-13` | Every successful non-test callback appends `auth.login` after its code/approval and before the same transaction commits, regardless of role change; the injected audit-failure proof demonstrates full rollback. | **CLOSED** |

## Final deduplicated finding ledger

The independently validated ledger is **empty**. There is no retained
`FIND-TASK-010-*` implementation finding and no decision-complete remediation
recommendation to issue.

## Verification assessment

The task and prior review records provide green focused evidence for the
authorization, callback, device, refresh, revocation, SQL/migrations, served
OpenAPI, generated contracts, client-tier and tenancy boundaries, official
image startup, documentation, formatting, and lint owners. R3 reviewers also
record direct green focused Postgres tests, `docs:check`, boundary checks,
codegen, and the startup lane. Those are the narrowest credible lanes for the
task and final correction write sets. The absence of full unfiltered identity,
every-language, and integrated TASK-011 journeys is intentional change-review
sequencing, not a verification limitation or a task finding.

No required reviewer or report is missing, no source or caller trace is
incomplete, and no unresolved authority disagreement requires `BLOCKED`.

## Validation result

**COMPLETE — EMPTY LEDGER.** All discovery claims and non-blocking concerns
were independently resolved against source; every prior finding is closed or
explicitly routed under current human authority.
