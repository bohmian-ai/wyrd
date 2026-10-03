# Focused Follow-up Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `134f605367e65b41f1977d6c70ac8ca8b277a69e`
- Candidate: `323ce32118ec72752a7736b8d42dd957abf6a094`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`

The repository has no `.codegraph/` directory. I used the immutable cumulative
diff, the remediation diff, `rg`, and direct source inspection. `HEAD` remained
the stated candidate throughout this follow-up.

## Uncertainty investigated

The repeat review raised several apparently separate documentation defects. I
investigated whether they arise from one endpoint/client taxonomy source, and
whether each is a reachable semantic inaccuracy rather than a duplicate,
evidence-only discrepancy, or wording/style preference.

## Inspected paths

- Authority and task inputs:
  - `changes/active/oidc-production-readiness/spec.md`
  - `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
  - `changes/active/oidc-production-readiness/review/TASK-005-r1/TASK-005-R1-doc-contract-accuracy.md`
  - `architecture/wyrd-design.md`
  - `architecture/wyrd-security-posture.md`
- Server route and wire owners:
  - `crates/wyrd/wyrd-server/src/components/auth/routes.rs`
  - `crates/wyrd/wyrd-server/src/components/platform/routes.rs`
  - `crates/wyrd/wyrd-server/src/auth/authorize.rs`
  - `crates/wyrd/wyrd-server/src/auth/callback.rs`
  - `crates/wyrd/wyrd-server/src/auth/cli_login.rs`
  - `crates/wyrd/wyrd-server/src/auth/oauth.rs`
  - `crates/wyrd-spec/src/auth/human_connection.rs`
- Public documentation and generated owners:
  - `docs/scripts/generate_api_docs.py`
  - `docs/src/content/docs/api/errors.md`
  - `docs/src/content/docs/api/openapi.md`
  - `docs/src/content/docs/for-agents/error-remediation.svx`
  - `docs/src/content/docs/concepts/identity-and-auth.svx`
  - `docs/src/content/docs/self-hosting/sso-and-oidc.svx`
  - `docs/src/content/docs/get-started/client-configuration.svx`
- Shared-client source:
  - `crates/shared/wyrd-client/src/config.rs`
- Verification record:
  - `changes/active/oidc-production-readiness/tasks/TASK-005-qualification-and-docs.md`
  - `changes/active/oidc-production-readiness/review/TASK-005-r1/verdict.md`

## Shared-source analysis

There is not one universal source for all proposals. The semantic proposals
fall into four correction boundaries.

1. **OAuth endpoint-shape taxonomy** — `MAINT-R2-001`,
   `STD-TASK-005-R2-1`, and `STD-TASK-005-R2-2` share the same root cause:
   prose treats every OAuth-facing route as though it were one of the four
   form endpoints, then treats every refusal of those endpoints as `400` or
   `401`. The implementation instead has standard endpoint-specific shapes:
   `GET /auth/authorize` uses query parameters and either redirects with an RFC
   6749 section 4.1.2.1 error or renders an HTML refusal; the four form
   endpoints use OAuth JSON; and `OAuthError::status` also emits `500` and
   `503`. These proposals are related, but their cited consumers are not
   duplicates: the design authority, generated/global error guidance, and
   operator guide each publish the incorrect generalization.
2. **Tenant versus platform credential taxonomy** — `BHV-R2-001` and
   `SEC-R2-001` are one defect with two affected documentation consumers.
   Tenant grants are dispatched by `/auth/token` after `OAuthClients`
   identification. `/auth/platform/token` is mounted separately and directly
   exchanges the presented platform API-key subject through
   `PlatformSessions`; it accepts no `OAuthClients` dependency. The concept
   page and OAuth module rustdoc both erase that distinction.
3. **Tenant IdP registration taxonomy** — `BHV-R2-002` is separate. It concerns
   the external provider application's client-auth method, not Wyrd's own
   `wyrd-ui`/`wyrd-cli` OAuth clients or the platform exchange.
4. **Shared-client public field name** — `INV-R2-001` is separate from the
   OAuth endpoint taxonomy. It is a directly unusable Rust example.

`STD-TASK-005-R2-3` belongs to none of these groups. It is a verification-record
and whitespace discrepancy only.

## Claim resolution

### `MAINT-R2-001` — confirmed semantic inaccuracy; consolidate under the endpoint-shape source

`architecture/wyrd-design.md:570-573` says without qualification that “The
OAuth endpoints use the RFC 6749 form and JSON wire format.” The immediately
preceding table includes `GET /auth/authorize`, and the task explicitly places
the authorize endpoint within the documented OAuth surface. The actual
authorize handler at `auth/authorize.rs:31-137` accepts a query, redirects a
validated client with an RFC 6749 section 4.1.2.1 error, and renders HTML when
the client or redirect URI cannot be trusted. Revocation success is an empty
`200`, not JSON. The narrower, accurate form-endpoint description already
exists at `architecture/wyrd-security-posture.md:208-215`.

This is not a wording preference. The design file is active authority and the
sentence assigns the wrong request and response contract to reachable routes.
It shares a correction source with the global error and status claims below:
describe browser authorization separately from the four form endpoints and
retain endpoint-specific success and refusal shapes.

### `BHV-R2-001` / `SEC-R2-001` — one confirmed semantic defect with two consumers

`docs/src/content/docs/concepts/identity-and-auth.svx:86-92` says all
machine-facing issuance goes through `/auth/token`. That is false for a
platform administrator credential, whose only exchange route is
`/auth/platform/token`. `components/auth/routes.rs:55-68` and
`components/platform/routes.rs:54-60` mount the routes under separate owners.

`crates/wyrd/wyrd-server/src/auth/oauth.rs:3-7` also says all four form
endpoints identify a client through `OAuthClients`. The tenant token handler
does so at `components/auth/routes.rs:113-133`; the platform handler at
`components/platform/routes.rs:94-142` accepts only `OAuthForm`, constructs
`PlatformSessions`, and exchanges the platform API-key subject directly.

These are not two independent findings. They are sibling effects of one
collapsed security-plane taxonomy and should be one correction boundary that
qualifies both descriptions. A correction must preserve the existing separate
routes and RFC 8693 API-key exchange; it needs no alias, fallback, platform
OAuth-client registration, or shared abstraction.

### `STD-TASK-005-R2-1` — confirmed semantic inaccuracy; related to `MAINT-R2-001`, not duplicate text

The generator at `docs/scripts/generate_api_docs.py:60-66,123-125` and its
generated `api/openapi.md` and `api/errors.md` outputs say every operation has
Problem Details and that the only exception is the four form endpoints. The
agent guide repeats the same global split at
`for-agents/error-remediation.svx:15-17`. The served authorize contract at
`auth/authorize.rs:61-70` explicitly publishes a `303` client redirect carrying
the standard authorization error and a `400` that may be HTML. Those outcomes
are reachable from malformed or refused authorization requests and are neither
Problem Details nor RFC 6749 section 5.2 JSON.

The proposal is therefore not an optional completeness improvement. Raw HTTP
and generated-contract consumers are told to expect the wrong error shape.
It should be reconciled with `MAINT-R2-001` at the shared endpoint-taxonomy
boundary, while updating generated pages only through their generator. No new
error envelope or catalog is warranted.

### `STD-TASK-005-R2-2` — confirmed semantic inaccuracy; same endpoint-shape source

`docs/src/content/docs/self-hosting/sso-and-oidc.svx:152` says refusals from the
four form endpoints use `400`, or `401` for `invalid_client`. The shared wire
owner at `auth/oauth.rs:65-76` maps `server_error` to `500` and
`temporarily_unavailable` to `503`. The tenant token OpenAPI declaration at
`components/auth/routes.rs:97-105` exposes both, and the platform route exposes
`500` at `components/platform/routes.rs:81-86`. These paths are reachable when
stores, issuance, or audit fail; the token rustdoc at
`components/auth/routes.rs:81-91` names those conditions.

This is not a request for exhaustive wording. The current sentence positively
excludes shipped standard statuses. The smallest correction is to name the
existing `500 server_error` and `503 temporarily_unavailable` cases alongside
`400` and `401`, without changing runtime behavior.

### `BHV-R2-002` — confirmed semantic inaccuracy; independent provider-client boundary

The generic provider procedure is introduced as applying to every provider,
then `self-hosting/sso-and-oidc.svx:61` requires a confidential OIDC client.
The same procedure later allows `Public` at lines 63 and 65, and the shipped
contract explicitly defines `HumanClientAuth::Public` with no secret at
`wyrd-spec/src/auth/human_connection.rs:73-112`. Its validation and tests also
accept that variant, including `human_connection.rs:350-442`; the server's
candidate path consumes the typed choice rather than forcing a secret.

The contradiction is reachable for an operator registering a supported public
tenant IdP client and is material because REQ-018 requires accurate setup
inputs. It is not a preference about calling the application “Web.” The
generic step must allow the shipped confidential or public choice, while the
provider examples may continue to describe the conventional confidential
settings those examples use. This does not reopen Wyrd's own confidential
`wyrd-ui` client.

### `INV-R2-001` — confirmed semantic inaccuracy; independent shared-client example

The Rust example at `get-started/client-configuration.svx:91-99` assigns
`config.api_key`. `ClientConfig` has no such field; its explicit public field is
`credential` at `crates/shared/wyrd-client/src/config.rs:39-61`, and the same
page describes that field at lines 109-118. A reader cannot compile the shown
configuration. This is neither terminology-only nor a preference between
`api_key` and `credential`; it is a nonexistent public API. The smallest
correction uses `ClientConfig::credential` and preserves the established
credential precedence.

### `STD-TASK-005-R2-3` — factual evidence discrepancy, but non-blocking style/housekeeping

`git diff --check
134f605367e65b41f1977d6c70ac8ca8b277a69e..323ce32118ec72752a7736b8d42dd957abf6a094`
exits `2` solely for
`changes/active/oidc-production-readiness/review/TASK-005-r1/verdict.md:99: new
blank line at EOF`. The same result occurs over the remediation-only range.
The task evidence nevertheless records `git diff --check` among checks that
exited zero.

That makes the recorded result literally inaccurate, but the only underlying
condition is an extra blank line in a prior review artifact. It changes no
shipped behavior, public contract, security boundary, or required task
documentation; it is exactly a whitespace/style issue under the standing human
direction. `git diff --check` is also not one of TASK-005's specified `mise`
verification lanes. Accordingly this should be recorded as a non-blocking
evidence correction or housekeeping note, not retained as a material finding
and not used by itself to produce `FIX_REQUIRED`. No journey or aggregate is
needed.

## New proposed findings

None. The source inspection resolves and consolidates the supplied proposals;
it did not reveal a distinct additional reachable defect within this focused
uncertainty.

## Resolution

**RESOLVED**

The material semantic union is: one endpoint-shape documentation correction
covering `MAINT-R2-001`, `STD-TASK-005-R2-1`, and `STD-TASK-005-R2-2`; one
tenant/platform plane correction covering `BHV-R2-001` and `SEC-R2-001`; one
provider-registration correction for `BHV-R2-002`; and one Rust public-API
example correction for `INV-R2-001`. `STD-TASK-005-R2-3` is a non-blocking
whitespace/evidence note. No placement, naming, structure, phrasing preference,
new protocol mechanism, journey suite, or aggregate is required.
