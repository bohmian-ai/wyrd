# TASK-010 R3 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Prior reviewed candidate: `7e8cbd4ff3f1d1a476508988a75e0721e2d9ce29`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- R1 remediation:
  `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Superseding R2 direction:
  `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`
- Routed finding direction:
  `changes/active/oidc-production-readiness/review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md`

The candidate remained at the stated commit throughout this review.
`.codegraph/` is absent, so I used the immutable Git ranges and repository
source directly. I reviewed the complete base-to-candidate range, both prior
verdicts and validated ledgers, the R1 remediation, and the latest correction
diff. The lead direction supersedes
`TASK-010-R2-public-edge-device-admission.md`: Wyrd must not ship an in-image
or application limiter or edge manifests; the operator-owned public ingress
owns the conventional limit and Wyrd documents the endpoint to protect.
`FIND-TASK-010-1` remains routed to TASK-011 and is not reopened here.

## Invariant traces

- **Authorization request to credential:** the registered client and exact
  redirect are resolved before non-binding parameter errors can be redirected;
  the callback consumes server-owned state, fences the active connection in
  the final tenant transaction, records the User and roles, attaches only a
  hashed one-use code or device approval, appends `auth.login`, and commits.
  Code and device redemption remain the only credential-minting consumers.
- **Device lifecycle:** the callback stores approval only. Denial or
  expiry/deletion that wins the approval interleaving leaves no authority;
  redemption locks and consumes the live row, issues one session, appends its
  audit event, and commits once. The latest correction changes none of these
  owners.
- **Refresh lifecycle:** the client binding is checked before the family lock;
  `wyrd-ui` reuses a still-active bounded row, while `wyrd-cli` atomically
  consumes and rotates. Only a predecessor marked `rotated` triggers replay
  containment, and `revoke_refresh_chain` confines it to that login chain.
- **Tenant and audit authority:** tenant work continues through `TenantConn`
  and RLS. Callback connection fencing, role replacement, code/approval
  attachment, login audit, and commit remain one transaction. No second
  principal, role, issuer, audit, token, or tenant-selection path entered the
  latest diff.
- **Device user-code admission:** the invalid per-replica NGINX
  `limit_req` state and its one-container assertions are deleted. The existing
  deployment authority already assigns rate-limit integration to the
  operator's public gateway, and the self-hosting documentation now tells the
  operator to limit only `POST /auth/device` per client address there. No
  application state, option, header parser, edge manifest, cache, or second
  limiting mechanism remains.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| RFC 6749 authorize errors preserve exact registered client/redirect binding | `crates/wyrd/wyrd-server/src/auth/authorize.rs:75-143,213-245` validates the unique client and redirect before parsing the remaining request and redirects only after that binding | `duplicate_parameters_redirect_back_to_the_registered_client`; `an_unregistered_redirect_is_never_followed`; recorded principals lane | PASS |
| Provider denial consumes server-owned state and completes the downstream authorization attempt without authority | `CallbackQuery::response`; callback refusal path; one-time login-state owner | `callback_query_carries_exactly_one_provider_response`; `a_provider_error_consumes_state_and_refuses_to_the_client`; recorded callback journey | PASS |
| REQ-009 / AC-007: a hashed, at-most-60-second, single-use authorization code remains bound to tenant, principal, client, exact redirect, active connection revision, and S256 verifier | Existing `finish_id_token_exchange` and `redeem_code` owners; `fence_bound_connection` holds the connection-slot lock through final commit | Recorded code refusal/replay and connection-cutoff evidence | PASS |
| RFC 6749 confidential client authentication retains standard Basic behavior without another authentication mechanism | `OAuthClients::identify` uses case-insensitive `Basic` matching and the existing client, decoded secret, digest, and form-consistency checks | `basic_scheme_matches_case_insensitively`; recorded principals lane | PASS |
| RFC 8693 unsupported target remains `invalid_target`, while malformed exchange remains `invalid_request` | `OAuthForm::token_request` classifies a present unsupported exchange audience before generic decoding | `token_exchange_audience_is_classified_before_decoding`; contract response evidence | PASS |
| REQ-011 / AC-007: terminal device state wins over a racing approval and a live approval issues exactly once at redemption | `CliLogins::approve` and `redeem_in`; existing device authorization uniqueness/row locks; no credential is stored by callback | `a_denial_during_approval_wins`; `an_expiry_deleted_during_approval_wins`; exact-once device tests | PASS |
| REQ-012 / RFC 9700: public refresh rotates; only a rotated predecessor triggers reuse containment; containment stays within its chain; confidential refresh does not rotate and remains bounded | `wyrd-auth/src/refresh.rs:103-215`; `consume_active_refresh`, `active_refresh`, and `revoke_refresh_chain` | `rotated_replay_revokes_only_its_chain`; `inactive_rows_are_refused_without_containment`; overlapping-rotation and confidential-client evidence | PASS |
| REQ-016: connection mutation winning before callback commit leaves no User mutation, roles, code, approval, or successful-login audit | `finish_id_token_exchange` takes the User-family lock, then the connection-slot lock, and rechecks the exact active binding in its final tenant transaction | Callback rollback cases and recorded multi-replica connection-cutoff journey | PASS |
| REQ-017: each successful non-test callback commits one canonical login outcome transactionally, independently of whether roles changed | `LOGIN_OPERATION`, `login_event`, and `append_auth_audit` execute before the callback transaction commits | `finish_issues_seals_and_audits_the_session`; unchanged-role device audit; audit-failure rollback proof | PASS |
| INV-001 / INV-007: tenant identity, principal, roles, credentials, and audit remain server-owned with tenant SQL under RLS | Callback, code, device, refresh, and SQL owners continue to use server-bound state and `TenantConn`; `ACTIVE_REFRESH_SQL` has no duplicate tenant selector | Recorded tenant-isolation, SQL, callback, and issuance evidence | PASS |
| Served OAuth contract retains public-form and confidential-Basic client identification alternatives | `ClientForm<T>` and `oauthClientBasic`; token/device/revoke operation alternatives | Served OpenAPI contract evidence and recorded principals lane | PASS |
| RFC 8628 section 5.1 user-code attempt admission follows the superseding lead direction and the conventional operator boundary | `docker/official/extras/nginx/nginx.conf.template:35-62` has no device `map`, zone, status, or `limit_req`; `docs/src/content/docs/self-hosting/sso-and-oidc.svx:19-23` names `POST /auth/device`, RFC 8628 section 5.1, per-client-address limiting, and the public ingress | Current `mise run docs:check` exit 0; recorded `mise run test:server:startup` exit 0 | PASS — prior `FIND-TASK-010-10` closed |
| No unsupported limiter mechanism, check, file, setting, option, or edge ownership is introduced | Latest implementation diff deletes the image-local mechanism and its assertions; source search finds no TASK-010 application limiter or device-limit setting; no edge manifest was added | Latest correction diff inspection and `git diff --check` | PASS |
| The startup lane retains its accepted single-tenant owner-only-key journey without carrying a false device-limit proof | `scripts/server/test-startup.sh:5-14,178-202` retains the official-image cases and removes only the image-limit assertions | Recorded `mise run test:server:startup` exit 0 | PASS |
| Form-only OAuth wire, RFC success/error bodies, metadata, revocation, API-key exchange, JWT bearer, delegation, and workload semantics retain their existing owners | The cumulative source contains no alternate issuer, token store, JSON compatibility route, legacy API-key grant, or extra OAuth document | Recorded focused identity, principals, SQL, codegen, client-boundary, format, and lint evidence | PASS |
| Deleted BFF channel, browser-session store, sealed completion, and associated migrations remain deleted | Cumulative diff and current route/query/module source; remaining UI consumers are expressly TASK-011 work, not a restored server surface | Recorded compile, SQL migration, and codegen evidence | PASS |
| TASK-011 owns the production `openid-client` 6.8.8 BFF journey | Standing lead direction routes `FIND-TASK-010-1`; this candidate adds no duplicate test-only driver | Integrated proof remains assigned to TASK-011 and change review | PASS / routed; not reopened |

## Proposed findings

None.

The latest correction deletes the unsupported and ineffective application-
image mechanism and documents the conventional operator-owned boundary exactly
as directed. It neither weakens an in-server authorization invariant nor adds
a new mechanism that the RFCs or comparable self-hosted authorization-server
deployments do not use.

## Prior-finding closure

| Prior finding | Closure assessment |
|---|---|
| `FIND-TASK-010-1` | Routed to TASK-011 by standing lead direction; not reopened. |
| `FIND-TASK-010-2` | Closed by the deterministic denial and expiry/delete approval interleavings. |
| `FIND-TASK-010-3` | Closed by RFC 8693 `invalid_target` classification. |
| `FIND-TASK-010-4` | Closed by validating the unique registered client/redirect before non-binding request parsing. |
| `FIND-TASK-010-5` | Closed by the provider success-or-error callback wire and one-time state owner. |
| `FIND-TASK-010-6` | Closed by case-insensitive Basic authentication-scheme matching. |
| `FIND-TASK-010-7` | Closed by the required Rust documentation additions. |
| `FIND-TASK-010-8` | Closed by deleting the duplicate tenant predicate from `active_refresh` and relying on RLS. |
| `FIND-TASK-010-9` | Closed by the served public-form/Basic OpenAPI alternatives. |
| `FIND-TASK-010-10` | **Closed by the superseding lead direction:** the image-local limiter and its assertions are deleted, the operator ingress endpoint and key are documented, and no application limiter or edge manifest was added. |
| `FIND-TASK-010-11` | Closed by rotated-predecessor-only classification and existing chain containment. |
| `FIND-TASK-010-12` | Closed by the final-transaction connection-slot fence. |
| `FIND-TASK-010-13` | Closed by transactional canonical `auth.login` evidence. |

## Non-blocking notes

- The cumulative diff includes old TASK-011 UI test and source consumers of
  the deleted private BFF paths. The original task expressly assigns those
  consumer rewrites to TASK-011 and forbids a compatibility route in TASK-010;
  they are not a TASK-010 invariant defect.
- Placement, naming, structure, and wording were not used as blocking review
  criteria. No missing mandatory Rust documentation was found in the latest
  correction.

## Verification assessment

The R1/R2 records supply green narrow owner evidence for principals
integration, SQL, code generation, client tier, tenant isolation, unwrap
audit, format, lints, filtered identity journeys, CLI journey, and the official
image startup lane. Those sources remain unchanged by the latest four-file
correction except for deletion of the invalid NGINX rule and its assertions.

For this review, `mise run docs:check` passed against the immutable candidate,
the latest correction diff passed `git diff --check`, and source inspection
confirmed the absence of the image-local and Rust-local device limiter. The
recorded startup lane is the narrow proof that the official image and retained
startup assertions still work; it is no longer asked to simulate an
operator-owned public ingress. Full identity, every-language, and integrated
deployment journeys remain correctly deferred to change review.

## Overall result

**PASS**

The complete cumulative candidate satisfies TASK-010 under specification
revision 11 and the two standing lead directions. All implementation findings
are closed or explicitly routed, and the invariant review proposes no new
finding.
