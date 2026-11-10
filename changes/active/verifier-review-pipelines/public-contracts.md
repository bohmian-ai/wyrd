# Verification reads and repository contracts

These are shared wire and authorization decisions for Tasks 001 and 004.
Contracts live in wyrd-spec; SDKs project the same server behavior.

The GitHub connection sections implement the approved revision-24 amendment.
The deprecated per-tenant App contract is historical; other approved execution
contracts retain their meaning.

## Execution authorization

| Public operation | Required permission |
|---|---|
| Fetch a VerifierCard definition through Cards | Existing cards:read |
| Execute/enqueue or cancel | Existing verifier:run, scoped to the exact Verifier UID or all |
| Read execution status, invocation progress, publications/manifests or downloads | New verifier:read, scoped to the exact Verifier UID or all |
| Arbitrary Bifrost query | Existing bifrost_query:read for every resolved table |

Repository execution reads additionally require repositories:read; submit/cancel
requires repositories:run. Retained execution evidence uses these feature
permissions. Direct access through the original Card/Artifact/source APIs keeps
those APIs’ existing permissions; verifier:read does not grant direct access to
them. Public reads use the requesting caller, not the execution principal.

Extend the existing Verifier permission scope to allow read as well as run:

```json
{"resource":"verifier","action":"read","scope":{"verifier":"<verifier-card-uid>"}}
```

There is no inspector role or additional discarded-content permission. Authorized
execution readers can inspect all retained dispositions. Downstream mounts still
exclude withheld/discarded/blocked content. Inspection cannot release it.

Engine-internal definition loading remains authorized by verifier:run; it does
not require cards:read. Dedicated execution reads use fixed tenant/execution-
scoped server queries after feature authorization, including narrowly scoped
internal Bifrost reads when needed. Public Bifrost SQL keeps caller table grants.
Amend the owning design/security System-reader allowances before implementation;
no broad SYSTEM bypass or caller-supplied SQL is authorized.

Existing execution reads switch from cards:read to verifier:read; do not retain
an alternate cards:read allow path. Update built-in role grants, schemas, SDK
projections, fixtures and documentation together. Existing role grants are not
silently broadened beyond their intended Verifier scope.

## Execution source ownership

The approved [execution-state contract](execution-state-and-durability.md) owns
persistence and recovery. Task 001 replaces active-PostgreSQL/retained-Bifrost
selection with one Bifrost execution-state source and the engine's running state.
Public method, response, error, permission and status vocabulary stays unchanged.

Final success follows acknowledged selected state and required result records.
Cancel 200 retains its committed cancellation guarantee; repeated terminal cancel
returns unchanged terminal state. No source switch, backend selector or public
freshness parameter is added. Current principal/registry/security IO is permitted.

Invocation/publication pages retain their first accepted cut, original ordering,
producer tuple and receipt identity across completion and restart. Admission keys
keep their scope/digest/replay behavior under existing lifetimes. Unavailable
required Bifrost state is explicit unavailability, not absence or permission to
create a new execution. Terminal cancellation does not rehydrate the engine.

Dedicated reads authorize current grants against recorded exact identities,
including after source Card deletion. Physical File descriptors come from the
accepted/recovered run state; download plans, public manifests/values and producer
selectors remain unchanged. Agent tools and UI use these same server contracts.

## Execution APIs

The approved revision-22 TASK-003 amendment adds no acceptance-time limits
snapshot. Existing configured limits remain enforced; enqueue still returns
`{execution_id}`.

The shared client's wait operation maps a terminal `cancelled` execution to
`WYRD_SDK_409_VERIFICATION_CANCELLED` (HTTP-equivalent status 409), with
`execution_id` and `status: cancelled` in error details. This catalog error is
projected unchanged through Rust, Python and TypeScript; it is neither the wait
deadline error nor an upstream failure. Remediation is to inspect retained
evidence and submit a new execution if further work is wanted. The server's
stored `error.code: cancelled`, inspection response and cancel response remain
unchanged. No generic transport alias for non-catalog server codes is added.

All paths below start with /v1/verification. Existing request fields and Card
result JSON stay unchanged. SDK/CLI/MCP methods use these same server contracts.

| Method/path | Request | Success response |
|---|---|---|
| POST /execute | Existing request, extended with the repository target arm below | 200 existing VerificationResult, with Card or repository subject |
| POST /enqueue | Same target choice; preserve existing Idempotency-Key behavior | 202 {execution_id} after durable admission |
| GET /executions/{execution_id} | Existing execution ID | 200 existing typed execution state; authorization changes to verifier:read |
| GET /executions/{execution_id}/invocations | limit/cursor | 200 {execution_id,status,items,next_cursor}; items contain the checkpointed invocation records, including step/item/iteration/attempt identity, status/reason and speculation disposition |
| GET /executions/{execution_id}/publications | limit/cursor | 200 {items,next_cursor}; each item contains name, kind, producer and speculation disposition |
| GET /executions/{execution_id}/publications/{step_id}/{name} | Producer query fields below | 200 publication read shape below |
| POST /executions/{execution_id}/publications/{step_id}/{name}/download | Producer query fields; directory file path when applicable | 200 existing File download-plan shape |
| POST /executions/{execution_id}/cancel | No body | 200 {execution_id,status} after the cancellation fence commits |

Producer query fields are required attempt (one-based), optional item_index and
iteration_index (zero-based; omission means null). They select the existing
producer identity, not the latest attempt. No new authored publication ID or
filename convention is introduced. Lists supply the exact producer fields.

A publication read returns name, kind and producer plus exactly one of:
value for JSON (including null), or manifest for a file/directory. The manifest
is the same immutable manifest retained by the publication backend, using the
existing File metadata and safe paths; do not create a second manifest format.
producer has step_id, item_index, iteration_index and attempt. A download for a
file needs no path. A directory download selects an actual regular-file path
from that manifest, relative to its root; traversal or missing paths refuse.
JSON is read through value, not a new file-export mechanism. Directory consumers
use the manifest and existing per-file download plans; no archive service is
required.

Use items and nullable next_cursor, with limit (default 50, maximum 200) and
cursor. A cursor fixes the listing cut and is bound to tenant, execution and
filters; malformed/mismatched cursors return 400. Omitted filters are not a
request to mix executions. Invocation metadata remains available while Bifrost
projection is pending. Lists/read responses include retained discarded work for
authorized execution readers, independently of downstream mount admission.
No file bodies or JSON publication values are loaded just to list progress.

Cancellation of unfinished work returns cancelled after its durable fence;
repeats return current state. Already-terminal work stays terminal without
rewriting its result/publications. Caller disconnection does not undo it.
Authorization is verifier:run, not ownership by the original submitter; apply
repository run when applicable.

Use existing problem+json errors: 400 invalid input/cursor/producer selector,
401 unauthenticated, 403 RBAC denial, the same execution-not-found 404 for
absent/foreign execution, and existing visibility-unavailable 503 for temporary
inventory lag. Unknown publication within an authorized execution is a typed
publication-not-found 404. Failed verification remains HTTP success with its
ordinary failed verdict. No additional payload permission, route alias,
inspection-only publication or public arbitrary-path access is introduced.

## Repository requests and results

Both POST /v1/verification/execute and /v1/verification/enqueue use this same
request target contract. Existing Card execution requests keep card_uid; add a repository alternative.
Exactly one arm is accepted, never both or neither. Other existing request fields
retain their meanings.

```json
{
  "verifier_card_uid": "<uid>",
  "repository": {
    "host": "https://api.github.com",
    "repository_id": "123456",
    "pull_request_number": 42
  }
}
```

Wyrd verifies the PR and pins its actual target and commits. Repository results
use this subject arm; existing Card results retain the raw CardRef unchanged:

```json
{
  "subject": {
    "repository": {
      "host": "https://api.github.com",
      "repository_id": "123456",
      "pull_request_number": 42,
      "target_ref": "refs/heads/main",
      "base_sha": "<full-commit-id>",
      "head_sha": "<full-commit-id>"
    }
  }
}
```

Host is the canonical API base (including /api/v3 for GHES). GitHub IDs on these
new public contracts are positive decimal strings validated against the library
ID range. PR numbers are positive integers. Commit IDs use the library's full
Git-object ID validation, not Wyrd's abbreviated CommitSha. Convert identifiers
at the existing library boundary; do not duplicate provider validation.

Repository identity is tenant + host + repository_id. Owner/name are labels;
integration ID and context/tree IDs are acquisition provenance. Repository
binding views retain binding_id, verifier_card_uid, active/readiness and existing
lifecycle fields, replacing Card-only owner_card_uid/card_uid with repository:
{host,repository_id,target_ref}. Preserve the existing Card arm unchanged.
No Repository Card, fake Card UID or synthetic application Run is introduced.

## Repository and integration permissions

| Resource | Actions | Scope |
|---|---|---|
| integrations | read, write | all, within the authenticated tenant |
| repositories | read, run, write | all or one exact host/repository_id |

```json
{
  "resource": "repositories",
  "action": "read",
  "scope": {
    "repository": {"host":"https://api.github.com","repository_id":"123456"}
  }
}
```

Repository read covers source evidence and review inspection; run covers submit
and cancel; write covers Wyrd Check Run publication, not repository file writes.
No name patterns, branch scopes or integration-ID-dependent repository identity.
A run requires repository run plus existing verifier:run. Installation access
and repository allowlisting are additional constraints, not grants.

Integration read covers redacted configuration and repository access settings.
Integration write covers verified connection, tenant principal configuration,
disable/disconnect and repository selection/revocation. App credential rotation
is operator configuration, never a tenant permission. Reuse the existing permission
validator/matcher and audit owner; do not add a policy engine or role bypass.

## GitHub App installation and tenant connections

One deployment-configured App serves many tenant installations. SaaS uses the
operator's Wyrd App; self-hosting uses the operator's own registration through
identical code. One configured GitHub host/App is supported per deployment in
this change. Operator settings include App ID, slug, API/web origins, private
key, webhook secret, OAuth client ID/secret and exact public setup/callback URLs.
Use existing secret providers and network policy. No tenant accepts or returns
App secrets, registers another App, or selects an arbitrary host. An unconfigured optional
App makes GitHub connection explicitly unavailable without disabling unrelated
capabilities; invalid configured values follow existing typed startup validation. App registration/secret rotation is documented
operator work; App Manifest automation is excluded.

### Browser onboarding

The selected protocol is Setup URL followed by separate GitHub user OAuth.
Do not enable automatic OAuth-on-install at the same time. Both entry points
share these server-owned stages:

1. Wyrd-first Connect starts under `integrations:write`, selects the existing
   tenant and its publishing/execution Service principals, and returns GitHub's
   installation URL with random correlated state.
2. GitHub-first installation arrives at the Setup URL with no trusted tenant
   context. Existing Wyrd sign-in/onboarding establishes a session and explicit
   tenant choice; the server then collects the same principal configuration.
   Neither the callback nor a webhook creates a tenant or chooses one implicitly.
3. Setup validates any presented installation state. An unsolicited visit carries
   only an untrusted installation hint. Start an expiring, one-use GitHub OAuth
   transaction bound to the exact App, browser, current Wyrd actor and tenant.
4. The OAuth callback validates state and supported PKCE, exchanges the code
   server-side, verifies the installation using the App and the authenticated
   user's paginated installations, and obtains that user's accessible repositories
   for the selected installation. Verify required App permissions and suspension.
   An inaccessible installation or insufficient consent never activates anything.
5. Show the verified GitHub repository selection for confirmation. Confirmation
   connects selected repositories using the existing tenant allowlist and enables
   PR checks in the same operation. Empty/invalid selection refuses. This is the
   normal product flow, not a sequence of manually composed administration calls.

Setup and OAuth transactions expire after five minutes on the database clock.
Bind them to the selected current Wyrd administrator and browser; a different
signed-in actor, changed tenant, replay or stale transaction refuses. GitHub-first
users who need longer to sign in restart the bounded connection after login.
A completed confirmation is safely repeatable with the same selected set;
conflicting reuse refuses. Returning after GitHub approval is not proof that
requested organization permissions were granted: pending requests remain pending.

The OAuth token and code are transient, redacted, never Wyrd login credentials,
and never persisted as background user impersonation. Confirmation uses a bounded
server-held verified selection for that transaction; it grants no extra Wyrd
permissions. Additional repository selections use this same verified connection
flow. Reconciliation may remove access or list installed repositories but cannot
allow new repositories automatically. There is no second persistent repository
snapshot/grant system, continuing personal-user delegation or user-token refresh
service. Background work uses installation tokens under current installation,
allowlist, Wyrd-principal and binding authority. User access to an installation
is not described as organization ownership; no new org-owner-only policy is added.

### Tenant API and common fields

All tenant operations require the existing verified tenant context and catalog
errors. Identifiers are shared typed IDs; new GitHub numeric IDs are positive
decimal strings. Browser transaction material is never an App credential.

| Method/path | Input and authority | Success |
| --- | --- | --- |
| POST /v1/integrations/github/connect | Publishing/execution principal IDs in this tenant; configured host only; `integrations:write`. Begin or reconnect the tenant's current connection without replacing a live association before confirmation. | 201 `{integration_id,authorization_url,expires_at}`; URL is GitHub install or OAuth according to the server-owned stage. |
| GET /v1/integrations/github | `integrations:read` | 200 `{items}` containing this tenant's redacted connection views. |
| GET /v1/integrations/github/{integration_id} | `integrations:read` | 200 connection view. |
| GET /v1/integrations/github/{integration_id}/selection | Current bound browser/admin and `integrations:write`; bounded setup transaction | 200 `{integration_id,items,next_cursor,expires_at}`; repository items have `host,repository_id,full_name`; keyset page defaults 50, max 200. No tokens. |
| POST /v1/integrations/github/{integration_id}/confirm | Bound unexpired verified selection plus `repository_ids`; current `integrations:write` | 200 connection view after atomic association/selection. Each ID must be in that transaction's verified selection. Existing selected access is preserved; newly selected repos become allowed and PR-enabled. Removing access uses revoke. |
| POST /v1/integrations/github/{integration_id}/disable | `integrations:write` | 200 disabled connection view; cancel affected unfinished work, keep provenance. |
| DELETE /v1/integrations/github/{integration_id} | `integrations:write` | 200 disabled connection view; disconnect the tenant association, retaining provenance and releasing its live installation claim. No GitHub uninstall or deletion of operator credentials. |
| POST /v1/repositories/reconcile | `integrations:write` | 200 repository access page; installed state is refreshed, no automatic allowlist expansion. |
| POST /v1/repositories/revoke | Exact `{host,repository_id}` and `integrations:write` | 200 resulting repository access view; stop affected work. |
| POST /v1/repositories/pull-requests | Exact repository key plus `enabled`; `integrations:write` | 200 access view; enabling requires an already allowed/installed repository. |
| GET /v1/repositories | `integrations:read`; keyset cursor/limit | 200 `{items,next_cursor}` repository access page. |
| GET /v1/repositories/bindings | Exact repository key, optional target_ref and cursor/limit; `repositories:read` on that repository | 200 `{items,next_cursor}` branch-scoped bindings. |
| GET /v1/repositories/checks | Exact repository key, optional PR number and cursor/limit; `repositories:read` | 200 `{items,next_cursor}` Checks and delivery state. |

Connection view fields are exactly `integration_id,host,app_id,installation_id,
publishing_principal_id,execution_principal_id,status,created_at,updated_at`;
status is pending/active/disabled and installation_id is null until verified.
Publishing/execution principals differ and must be active tenant Service principals;
reconnecting can update this pair only after successful confirmation. A failed
reconnect keeps existing connection/principal/access settings unchanged. Allowed
repository reads use existing keyset limits (default 50, max 200); malformed or
foreign cursors return 400. Never list another tenant's installation or repositories.

One tenant has at most one active installation; one host/App/installation has at
most one connected tenant. Another tenant's claim refuses without disclosure.
Disable retains the claim, while disconnect releases it; a new claimant must
independently complete GitHub proof. Both preserve retained evidence and cancel
only affected work, including queued/manual repository executions. Explicit
reconnect of the same disabled installation can restore it after current proof.
A different installation requires disconnecting the old association first.

Repository access view fields are `host,repository_id,full_name,allowed,
pull_requests_enabled,publishing_principal_id,execution_principal_id`. Binding
views use the existing lifecycle fields and the repository arm fixed above.
Check view fields are `binding_id,name,subject,execution_id,state,conclusion,
delivery,check_run_id`; configuration Checks have null binding/execution IDs.
Check state is queued/in_progress/completed; delivery is pending/delivered/failed.
Conclusions are GitHub's success/failure/action_required/cancelled/timed_out.
Execution inspection adds optional `repository_check` containing this same Check
view for a linked execution. It is absent for Card/manual executions without a
Check; delivery failure never changes execution status or verdict. This exposes
existing delivery control data without another result format/store.

Use existing problem+json catalog behavior: 400 invalid input/selection/state or
cursor, 401 missing/expired Wyrd login, 403 grants/GitHub association refusal,
404 unknown tenant resource, 409 association or conflicting completion, and 503
retryable provider/store unavailability. Missing App configuration is an explicit
503 GitHub-unavailable response. GitHub browser errors show a bounded safe error
and a restart action; never expose authorization codes, tokens, secrets or foreign account data.

The new domain errors use these stable codes through the normal derive-backed
catalog; authorization and authentication retain their existing shared codes:

| Condition | Code/status |
| --- | --- |
| Invalid domain input, selection, transaction or cursor | `WYRD_GITHUB_400_INVALID_INPUT` / 400 |
| GitHub proof refuses association | `WYRD_GITHUB_403_ASSOCIATION_REFUSED` / 403 |
| Unknown tenant connection | `WYRD_GITHUB_404_CONNECTION_NOT_FOUND` / 404 |
| Unknown/inaccessible repository in this connection | `WYRD_GITHUB_404_REPOSITORY_NOT_FOUND` / 404 |
| Claimed installation or conflicting completion | `WYRD_GITHUB_409_CONNECTION_CONFLICT` / 409 |
| Missing App configuration or temporary provider/control-store failure | `WYRD_GITHUB_503_UNAVAILABLE` / 503 |
| Invalid webhook authentication/association | `WYRD_GITHUB_401_WEBHOOK_REJECTED` / 401 |
| Webhook body exceeds the configured ingress bound | `WYRD_GITHUB_413_WEBHOOK_TOO_LARGE` / 413 |

SQL invariant failures still use the existing SQL catalog. A setup transaction
cannot be reused across an actor/tenant/browser boundary; return the same generic
association refusal without revealing whose transaction or installation it is.

### Protocol ingress and projections

Register `GET /auth/github/setup`, `GET /auth/github/callback` and
`POST /v1/webhooks/github` on the Rust server's configured public origin. Setup
and OAuth callback require their protocol correlation, not a Wyrd bearer token;
mutating confirmation still requires the authenticated browser/admin and current
Origin/CSRF checks. Preserve existing Wyrd session/login routes and fixed login
redirect policy. Webhook authentication is mandatory raw-body HMAC verification,
including when configuration is missing; never fail open. Unassociated events
cannot create tenant review work or infer a tenant from repository names.

The shared Rust client, Python, TypeScript, CLI and MCP expose the tenant
operations above. CLI/MCP start the same browser flow and inspect status; they
never accept GitHub OAuth codes, user/App tokens or secret fields. Browser-only
setup/selection/confirmation retain their browser-bound authorization; a machine
credential cannot impersonate that human completion. The UI uses the same Rust
contracts and existing BFF/session handling. Exact tool counts from the rejected
task are retired. HTTP/OpenAPI, SDK types, native exports, stubs and CLI/MCP
schemas must agree on the same fields, status codes and permission failures.

### Independent GitHub library

Keep `9fdf6dc77a329f34d96b7ad4f397331d2d18216b` as the generic external baseline;
no revert, merge or release is implied by this decision. The library accepts
consumer-signed App JWTs and supplies bounded App/installation/Check/webhook
protocol. Extend its existing GitHubClient/transport for generic OAuth exchange
and authenticated-user installation/repository queries. It must not import Wyrd,
serve Wyrd callbacks, configure Wyrd's App, create tenants, enforce Wyrd grants,
or own Wyrd's webhook/verification/delivery lifetimes. Pin a verified immutable
descendant when delivered; do not pin a branch/path or copy its REST client.

### Protocol references

- [Install an existing third-party App](https://docs.github.com/en/apps/using-github-apps/installing-a-github-app-from-a-third-party).
- [Install URL and state](https://docs.github.com/en/apps/sharing-github-apps/sharing-your-github-app).
- [Setup URL and spoofable installation IDs](https://docs.github.com/en/apps/creating-github-apps/registering-a-github-app/about-the-setup-url).
- [Setup URL versus automatic OAuth-on-install](https://docs.github.com/en/apps/maintaining-github-apps/modifying-a-github-app-registration).
- [OAuth callback, code exchange, state and PKCE](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-a-user-access-token-for-a-github-app).
- [User-accessible installations/repositories](https://docs.github.com/en/rest/apps/installations).
- [Check Run permissions and event subscriptions](https://docs.github.com/en/rest/guides/using-the-rest-api-to-interact-with-checks).
- [Branch deletion webhook](https://docs.github.com/en/webhooks/webhook-events-and-payloads#delete).

These sources support the protocol; tenant cardinality, exact Wyrd paths and
principal configuration above are explicit Wyrd decisions, not claims that
GitHub mandates Wyrd's product policy.
