---
id: SPEC-guided-setup
revision: 5
status: approved
---

# Guided setup, deployment identity, and the ASGI integration

## Problem and proposed behavior

Documentation work exposed three gaps in the path from local setup to a
running deployment: local bootstrap stops before a first verification, each
release needs credentials for a new exact Card version, and Python web
applications must manage evidence writers and telemetry by hand. Missing the
last step can leave evidence queued when a process exits.

This change keeps those related fixes together. `wyrd init` becomes the local
bootstrap command and hands the user a setup prompt or equivalent manual
steps. A deployment key belongs to a Service or Agent across its versions;
each release supplies its exact Card UID separately. An explicit ASGI middleware connects
request Runs to the existing SDK lifecycle and drains evidence on graceful
shutdown. Documentation teaches and verifies these same paths.

For example, CI declares `support-api` version 1, then version 2. Today each
version needs its own key. With this proposal the deployment keeps the same
secret and changes only `WYRD_WORKLOAD_UID`; both versions can run together
and their evidence remains attributed to different exact-version principals.

### Value, costs, and limits

The developer gets a checked first verification; the platform team gets
repeatable releases without routine secret replacement; the operator gets
explicit startup failures and observable drain failures. This is a guided path
with prerequisites, not a claim that one command installs PostgreSQL, chooses
an application language, or completes production deployment. The CLI,
compatible runtime tools, and PostgreSQL must already be available; provider
credentials are unnecessary for the first verification.

A deployment key deliberately authorizes every active version of the same
Service or Agent, identified by its kind, space, and name. Per-version principals and grants remain separate.
Explicit UID selection is preferred to “latest” because rolling releases must coexist;
per-version keys are the simpler current alternative but require secret
management for every release. Existing version-specific deployment keys require one-time replacement;
there is no key migration or silent widening of an existing secret.

The ASGI adapter is opt-in. It reuses WyrdState, Run, Bifrost, and telemetry
ownership rather than introducing another writer or framework instrumentation
engine. Graceful draining cannot guarantee delivery after a hard kill or a
shutdown failure. Users who already own an OpenTelemetry provider keep it and
supply `telemetry=False`.

## Authority and relationship to other changes

- `architecture/wyrd-security-posture.md` owns principals and credentials;
  this change revises API-key targeting and must update it in the same change.
- `architecture/wyrd-design.md` owns observation identity and the CLI;
  `architecture/bifrost-design.md` owns OTLP ingest. Both are updated where
  this change alters them.
- This specification replaces the `wyrd server dev` command name and
  interaction fixed by `SPEC-local-server-bootstrap`. Its server lifecycle,
  input resolution, saved settings, MCP host connection, and gateway behavior
  remain authoritative and move unchanged to `wyrd init`. Approval of revision 5 explicitly supersedes only that packet's command
  name and guided handoff requirements. Its other approved behavior remains
  authoritative; no second approval of this rename is required.
- Documentation follows `docs/AGENTS.md` and builds on
  `SPEC-docsite-incremental-reset`.

## Current behavior and ownership

- Bifrost signal tables (`vala.traces.spans`, `vala.logs.records`,
  `vala.metrics.points`) attribute every row to its publisher through the
  server-stamped `sub_principal_id` and `act_principal_id`. That is Service
  attribution. The optional record-level `wyrd.card_uid` attributes the Card
  that did the work, such as an Agent within the Service. This change keeps
  both rules unchanged.
- `wyrd declare` provisions one principal per exact Service or Agent Card
  version (provisioned idempotently by tenant, principal kind, Card kind, and
  UID), and declaring
  identical content returns the existing Card (`Deduplicated`).
- A principal holds any number of active API keys: `wyrd principal credential
  issue` adds one and leaves the others live, and `credential revoke` retires
  one.
- The client resolves one credential from its chain (explicit credential,
  `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN`, `WYRD_API_KEY`, saved login,
  `credentials.toml`). API keys and workload assertions are exchanged for
  five-minute access tokens; a selected bearer is used directly, and saved
  human sessions retain their existing renewal path.

The existing local bootstrap owns server installation/startup, saved inputs,
MCP host configuration, and gateway setup. The guided prompt extends the
handoff after readiness; it does not replace that orchestration. The registry
and `Cards` own declaration and graph pull. `WyrdState.from_path` loads bundles
locally without network access; `WyrdState.declare` currently declares and pulls
with one client. Production identity validation must respect that distinction.

The auth issuance contract currently returns one `principal_id` and exact
`card_ref`, and credential listing/revocation are principal-addressed. A key
that selects several principals therefore needs an explicit lifecycle contract;
changing only the exchange lookup is insufficient. Existing API-key token
caching is keyed by the credential, so UID selection must also separate caches.

## Definitions

- **Deployment key**: a Wyrd API key for one Service or Agent, identified by
  `kind`, `space`, and `name`, usable across its versions. Each deployed
  process also supplies the UID of the exact version it runs.
- **Workload UID**: the UID `wyrd declare` reports for the exact Card version
  a workload runs. It selects which version's principal a deployment key
  exchange issues a token for.
- **Setup prompt**: the versioned instruction text a coding agent follows to
  add Wyrd to the reader's project and confirm a first verification.
- **Setup path**: one documentation track for one audience: AI engineer,
  platform team, or operator.

## Requirements

### `wyrd init`

- **REQ-001** `wyrd init` replaces `wyrd server dev`. `wyrd server dev` is
  removed with no alias, hidden command, or redirect. Every input, option,
  environment variable, saved setting, default, readiness output, and failure
  of `wyrd server dev` applies to `wyrd init` unchanged unless this
  specification states otherwise.
- **REQ-002** Interactive `wyrd init`, after the server is ready, connects the
  selected MCP hosts as today and then prints, for each connected host that
  accepts an initial prompt on its command line, the exact command that starts
  it with the setup prompt in the current directory. Hosts without a supported
  launch command receive instructions for retrieving and pasting the prompt.
  Command arguments preserve the prompt literally, without shell expansion or
  secrets. It keeps the server in the foreground until interrupted and never starts a coding agent itself.
- **REQ-003** `wyrd init --print-prompt` prints the setup prompt for the
  installed CLI version on stdout and exits 0. It reads no configuration,
  contacts no server, makes no change, and conflicts with setup options
  (`--format` included). Ordinary help and version handling remain available.
- **REQ-004** `wyrd init --format json` readiness output adds the setup prompt
  under the string field `setup_prompt`; stdout still contains only the
  existing single readiness object. The process keeps serving.
- **REQ-005** The Wyrd MCP server exposes the setup prompt as an MCP prompt
  named `setup`, available to every authenticated principal, with no
  arguments. Reading it grants no write permission and never executes a step.

### Setup prompt

- **REQ-006** One source file in the repository owns the setup prompt. The
  CLI, the MCP server, and the documentation all project that file; none
  carries an independently maintained copy. Each surface labels the prompt
  with the release it projects; byte equality applies within that release, not
  between a newer CLI and an older server.
- **REQ-007** The setup prompt confirms the local server through MCP, then
  detects Python, Rust, or TypeScript using the project's manifest. If the
  choice is ambiguous or unsupported, it asks the user to choose a supported
  language before changing project files. It uses that language's package tool
  to install the SDK at the CLI release's exact version; unavailable releases
  stop setup rather than silently choosing another version. It declares a
  minimal Service and bound Verifier, records a Run and observation, obtains
  the judgment, and confirms durable observation and verdict storage through
  MCP reads. The example needs no model or provider credential. It reports the
  exact Card identity, Run id, and verdict; on failure it reports the first
  failed step and a Wyrd error code when present, otherwise the tool diagnostic.
- **REQ-008** The setup prompt contains no credential, no machine-specific
  path, and no step that requires a repository task or environment activation.
  Repeating setup preserves unrelated project files; conflicting existing
  example files are reported before replacement.

### Declaration output

- **REQ-009** `wyrd declare --format json` reports, for every declared Card
  and every outcome (`declared`, `deduplicated`, `idempotent_noop`), its
  `space`, `kind`, `name`, exact `version`, and Card UID. Re-declaring
  identical content reports the existing Card's version and UID.

### Deployment identity

- **REQ-010** An API key issued for a Service or Agent targets that Card's kind,
  space, and name, across versions. `wyrd auth issue-key` accepts those three
  coordinates and refuses a version. The same Service or Agent can have
  multiple active deployment keys; issuing or revoking one leaves the others
  live.
- **REQ-011** Exchanging a deployment key requires a Workload UID. The server
  issues a token for that Card's principal only when the UID names an active
  Card in the key's tenant with the same kind, space, and name, and its
  principal is active. The token's subject, Card reference, and Card scope are exactly what that Card's principal receives today.
- **REQ-012** A deployment key exchanged without a Workload UID, or with one
  that is unknown, inactive, or of another Service or Agent, is refused with
  a stable error. Unknown and cross-tenant UIDs are indistinguishable and disclose no
  foreign Card or principal metadata. Only an authenticated key holder receives
  details about its own Service or Agent. A Workload UID sent with any
  credential other than a deployment key is refused. Invalid keys retain the existing
  indistinguishable credential error. The server never substitutes another Card.
- **REQ-013** Each Card version keeps its own principal, roles, and Card
  scope; a role granted to or revoked from one version's principal does not
  change another version's.
- **REQ-014** The client reads the Workload UID from `WYRD_WORKLOAD_UID` and
  sends it as `workload_uid` in the existing API-key token-exchange form.
  The OAuth endpoint retains its standard success/error envelopes; the SDK
  preserves the existing Wyrd error mapping. The credential chain and its order are
  otherwise unchanged. Validation applies to the credential actually selected:
  a selected bearer, workload token, or saved login with a Workload UID fails
  client construction with a stable configuration error. An unused lower
  priority saved login does not invalidate an explicit deployment key. A
  malformed UID fails locally. An unbound API key plus UID is refused at
  exchange; the client cannot infer key ownership from a secret.
- **REQ-015** A stock OTLP exporter authenticating with `x-wyrd-api-key` and a
  deployment key sends its Workload UID in one additional request header,
  named `x-wyrd-workload-uid`, settable through
  `OTEL_EXPORTER_OTLP_HEADERS`, on both HTTP and gRPC OTLP routes. It is a
  selector validated against the key, never an authority supplied by a row.
  Bearer authentication plus this header is refused. Without it the export is
  refused under REQ-012.
- **REQ-016** Registration and deployed loading stay separate.
  `WyrdState.declare` keeps its existing register/pull/load meaning with
  registration credentials. A deployment-key client attempting registration
  is refused before any registration write, even when content is unchanged.
  Deployed applications use the existing read-only `Cards.pull` workflow with
  the exact UID selector, then `WyrdState.from_path` (or their language
  projections). CI may instead include that complete bundle in the image.
  No new state loader or local authoring-file comparison is required.
  `from_path` remains offline. At online startup, before the state's first
  write, the shared client checks that the bundle's root UID equals
  `WYRD_WORKLOAD_UID`; mismatch fails with a stable error. No path substitutes
  latest, declares missing Cards, or grants permissions to repair a refusal.
- **REQ-017** `wyrd pull` remains separate from `wyrd declare` and optional in
  the release flow.
- **REQ-018** Credentials for principals bound to no Card (users, tenant
  administrators, CI automation) are unchanged.
- **REQ-019** The API-key deployment flow is a required, tested scenario:
  - once: a CI automation principal with an API key in the CI system's secret
    store, and one deployment key for the Service across its versions in a deployment
    secret;
  - per release: CI declares with `wyrd declare --format json` and reads the
    Card UID; builds the image; deploys with `WYRD_API_KEY` from the secret
    and `WYRD_WORKLOAD_UID` set to that UID; no credential is issued or
    changed;
  - two releases run concurrently as a rolling update, each attributed to its
    own version's principal, then the older one stops;
  - the deployment key is rotated with no downtime: a second key is issued,
    workloads restart on it, the first is revoked, and the first then fails
    to exchange.
- **REQ-020** The unused `wyrd.service.card_uid` trace attribute constant is
  deleted. Service attribution is the publisher principal; Card attribution is
  `wyrd.card_uid`; no third key exists.

### ASGI integration

- **REQ-021** The Python SDK ships `wyrd.integrations.asgi.WyrdMiddleware`, a
  pure ASGI middleware usable as `app.add_middleware(WyrdMiddleware,
  state=state)` in FastAPI and Starlette, and by wrapping any ASGI
  application.
- **REQ-022** On ASGI lifespan startup it starts the state's Bifrost writer
  and, unless constructed with `telemetry=False`, the state's telemetry. A
  startup failure sends `lifespan.startup.failed` with a scrubbed diagnostic
  (including the Wyrd code when present), and never reports startup complete.
  Already-started resources are cleaned up without hiding the primary error.
  Missing telemetry dependencies retain the existing actionable extra-install
  error; no global provider is replaced.
- **REQ-023** On ASGI lifespan shutdown it shuts the state down, draining
  queued rows, without blocking the event loop. A shutdown failure is logged
  with its Wyrd error code and reported as `lifespan.shutdown.failed`; the
  server can still terminate. The wrapped application's lifespan also runs,
  and its startup and shutdown failures remain failures. Application shutdown
  runs before the final Wyrd drain so its final evidence can be retained.
  Cleanup uses the existing bounded lifecycle and never promises delivery on
  hard kill, timeout, or failed drain.
- **REQ-024** For each `http` and `websocket` scope it opens one Run around the
  application exchange, including streaming and application-owned completion
  work, and closes it when the ASGI call ends on success, exception, disconnect,
  or cancellation. The Run observes the root Service Card unless the optional `alias` callable,
  given the ASGI scope, returns a Card alias. An application exception still
  ends the Run and propagates unchanged. Concurrent scopes never share Run
  identity. An invalid alias fails before application execution with the SDK
  alias error; unrelated scope types pass through.
- **REQ-025** The middleware produces no HTTP spans of its own. When the
  application installs OpenTelemetry's ASGI or FastAPI instrumentation, those
  spans started inside the Run carry its `wyrd.card_uid` and `wyrd.run_id`.
  The documented middleware order places Wyrd outside that instrumentation;
  the adapter cannot retroactively tag spans created by outer middleware.
  `telemetry=False` leaves provider installation and flushing to the
  application, which uses the existing Wyrd exporter/correlation hooks.
- **REQ-026** Under Gunicorn with Uvicorn workers, each worker runs the
  lifespan independently and owns its own started writer and telemetry.
  Running SDK resources are created after worker startup, never shared across
  a process fork. Lifespan support must be enabled; the guide states this
  prerequisite and does not claim automatic startup/draining with it disabled.

### Exchange isolation

- **REQ-033** Access-token caches and shared authenticated transports separate
  different selected Workload UIDs even when the deployment key and endpoint
  are identical. Refresh preserves the UID. Disk reuse cannot return another
  version's token; every issuance resolves that version's current principal
  grants, without regranting revoked roles.
- **REQ-034** OTLP API-key authentication uses the same key/UID validation and
  permission resolution as token exchange, before payload decoding, without
  minting a token. Revocation refuses the next exchange and the next direct-key
  export. Already-issued bearer tokens retain their existing bounded lifetime.

### Deployment-key management and replacement

- **REQ-035** Deployment keys are managed by the existing server credential
  owner and projected through the existing `Principals` client handle.
  Issuance takes Service/Agent `kind`, `space`, and `name`, optional label and
  TTL, and requires an existing registered Card of that identity. Version and
  UID are rejected as issuance selectors. Issue and revoke require the
  existing `service_accounts:write` permission; metadata reads retain the
  credential owner's authorization. Reads disclose only credential ID,
  tenant-owned Card coordinates, prefix, label, timestamps, expiry and
  revocation metadata, never a secret or an arbitrary version principal.
  Plaintext is returned once on issuance. Multiple keys and independent
  revocation retain the existing credential lifecycle.

  `POST /auth/issue-key` uses these version-independent coordinates; its
  response keeps the existing key/lifecycle fields, replaces `card_ref` with
  `kind`, `space`, `name`, and removes `principal_id`. Metadata listing is
  `GET /v1/credentials` with required `kind`, `space`, `name` query parameters;
  inspection and revocation are `GET` and `DELETE
  /v1/credentials/{credential_id}`, restricted to deployment keys in the
  caller's tenant. A successful DELETE returns 204; a foreign or absent ID
  returns the same not-found error. Existing principal-addressed credential
  operations continue for unbound principals and refuse Card-bound issuance;
  they never reinterpret a version principal as all versions.

  The CLI retains `wyrd auth issue-key` with `--kind`, `--space`, `--name`,
  and adds `auth list-keys` with those selectors and `auth inspect-key` /
  `auth revoke-key` with `--credential-id`. The shared `Principals` handle
  exposes `issue_deployment_key`, `list_deployment_keys`,
  `get_deployment_key`, `revoke_deployment_key`, projected with the SDK's
  established casing. MCP exposes the corresponding
  `principals.issue_deployment_key`, `principals.list_deployment_keys`,
  `principals.get_deployment_key`, `principals.revoke_deployment_key` tools.
  Read tools remain discoverable; write tools require explicit scopes and
  server authorization. Every projection uses the same typed wire contracts.
- **REQ-036** Existing exact-version keys for Card-bound principals are not
  migrated or broadened. After the release they cannot mint new tokens or
  authenticate direct OTLP exports; their non-secret metadata and audit history
  remain retained. Operators receive actionable replacement instructions and
  issue a new deployment key once. Unbound user, administrator and CI keys
  remain usable. Existing bearer tokens retain their ordinary expiry. Schema
  changes preserve accepted evidence and follow the operations migration
  contract; deployments reject old/new serving overlap that cannot honor the
  new credential contract. This does not introduce an old-key compatibility path.

### Documentation

- **REQ-027** Get started opens with `wyrd init` as the single entry command,
  after installation and PostgreSQL prerequisites, with two tabs:
  "Agent-assisted" (start the agent with the setup prompt) and
  "Manual" (the same steps the prompt gives, journey-tested).
- **REQ-028** A "Choose your path" page offers exactly three setup paths:
  AI engineer (SDK in an application), platform team (deploy and attribute
  workloads), and operator (deploy Wyrd for a team). Each path lists its pages
  in order, at most five.
- **REQ-029** A platform-team guide shows the REQ-019 flow with no CI vendor
  named: the one-time keys, the per-release `wyrd declare --format json` step
  and where its UID goes, a Kubernetes Deployment reading `WYRD_API_KEY` from
  a Secret and `WYRD_WORKLOAD_UID` from the release, and key rotation. It
  shows a stock OTLP exporter configured with a deployment key
  and the Workload UID header, and states that one exporter shared by several
  Services attributes every record to the authenticated exporter principal,
  not separately to the Services that produced it. OBI remains an explicitly
  scoped example; its deployment instructions must be verified before publication.
- **REQ-030** `docs/AGENTS.md` adds an Integration page shape: What you
  capture → Before you start → Install → Usage → Verify it worked → If it
  fails → Advanced → Reference → Next, with a copy target. Integration is a
  Guide shape within the existing page taxonomy; optional sections appear only
  when needed.
- **REQ-031** The first Integration page covers FastAPI through the ASGI
  integration. A Gunicorn section states the worker lifespan rule of REQ-026.
- **REQ-032** No published page, generated `llms*.txt`, CLI help, or error
  remediation mentions `wyrd server dev` after this change.

## Invariants and prohibited outcomes

- **INV-001** Attribution never guesses: deployment-key authentication selects
  only the exact version named by Workload UID. Row values never set the
  publisher principal. Unchanged administrative credentials need no UID.
- **INV-002** During a rolling update, each record lands on the exact version
  its own workload runs, with no credential change.
- **INV-003** A deployment key reaches only versions of its own Service or Agent,
  in its own tenant.
- **INV-004** Releasing a new Card version requires no credential issuance or
  secret change.
- **INV-005** A deployed workload never declares; the declaring identity (CI)
  and the selected exact-version runtime identity are different principals.
  Local developer declaration with an administrative identity remains supported.
- **INV-006** The deployment flow depends only on Wyrd API keys; no CI vendor,
  cloud, or trusted issuer is required.
- **INV-007** The setup prompt has one source; the CLI, MCP, and docs project
  the same content for the same release.
- **INV-008** `wyrd init` never starts or drives a coding agent.
- **INV-009** Every feature works identically on macOS arm64, Linux x86_64,
  and Linux arm64. Kubernetes and OBI are documented deployment examples, not
  platform requirements.

## Security, tenancy, durability, compatibility

- A deployment key holder can authenticate as any active version of its
  Service or Agent in its tenant. Each token receives the selected principal's
  current grants, initially the built-in `workload` role. A key never grants
  or restores roles, and never reaches a different Service, Agent, or tenant.
- A revoked deployment key mints no new token and cannot authenticate the
  next direct OTLP export; existing bearer tokens expire within five minutes.
  The same Service's or Agent's other keys keep working. Revocation is not instant bearer withdrawal.
- Authentication and authorization retain their existing audit rules. Accepted
  key exchanges record the credential and selected principal; permission
  decisions are audited at their existing boundaries. Invalid keys never
  manufacture an authenticated tenant or principal for auditing.
- Inactive/deleted versions, suspended principals, suspended tenants, expired
  keys, and revoked keys are never issued a token. Audit staging failures keep
  the existing non-blocking behavior and are logged and counted.
- The maintainer explicitly requires no key migration. Existing Card-bound
  keys require replacement; no persisted key is silently widened.
- Recorded in `CHANGELOG.md`: `wyrd server dev` removed; version-independent targeting
  for `issue-key`; deployment keys require `WYRD_WORKLOAD_UID`.

## Expensive-to-reverse decisions and required boundaries

- `wyrd init`, `--print-prompt`, and the `declare` output in `wyrd-cli`.
- The `setup` MCP prompt in the Wyrd MCP server. It is instruction text, not
  an Agent or Prompt Card registration and not a workflow executor.
- Version-independent Service/Agent API-key targeting and the Workload UID exchange parameter in the
  auth wire contract (`wyrd-spec`), owned by the server.
- The OTLP Workload UID header on the server's OTLP routes.
- `WYRD_WORKLOAD_UID` and the startup Card check in `wyrd-client`, projected
  by all three SDKs.
- `wyrd.integrations.asgi.WyrdMiddleware` in `sdks/wyrd-sdk-python`, with
  authored `.pyi` stubs.
- Security, Wyrd, Bifrost, and operations deployment/release authorities must
  describe the same identity, local-command, and compatibility behavior.
- Deployment-key metadata names `kind`, `space`, and `name` separately from
  the versioned `CardRef` used for evidence. `CardRef.version` remains required.
  This creates no new Card kind or aggregate principal. Management stays with
  the existing credential owner and follows the management contract above.

## Acceptance and evidence

These are required observations, not a prescribed test-file layout. Every
changed shipped surface receives real journey coverage against
`WyrdTestServer` and repository Postgres; shared Rust logic does not substitute
for Python or Node lifecycle and public SDK projection evidence.

| ID | Situation and expected observation | Obligations | Evidence class |
|---|---|---|---|
| AC-001 | Start locally interactively, without prompts, and as JSON; saved settings, MCP hosts, gateway setup, readiness, and failure behavior are preserved. The old command is rejected. | REQ-001–002, INV-008 | CLI journeys and existing packaged local-development journeys in all three languages |
| AC-002 | Read the prompt offline, through authenticated MCP, from readiness JSON, and from the matching documentation release; content agrees and version labels identify mismatches. Offline printing has no side effects and conflicting options fail. | REQ-003–006, INV-007 | CLI/MCP journeys and release-projection checks |
| AC-003 | Starting with a supported project and documented prerequisites, manual steps in each language produce an observation and verdict retrievable through MCP. Ambiguous language choice or conflicting files stops before overwrite. No provider key is used. | REQ-007–008 | Journey per language, plus prompt-content and failure-path checks; live coding-agent behavior is not a deterministic release gate |
| AC-004 | Declare new, identical, and replayed content; every result names its exact version and UID, and replay preserves identity. | REQ-009 | CLI declaration journey |
| AC-005 | Missing, malformed, unknown, inactive, other-Service/Agent, and other-tenant UIDs fail closed. Invalid keys and foreign version selectors reveal no foreign metadata; no fallback Card or write occurs. Suspended principal/tenant and expired keys also fail. | REQ-010–013, INV-001, INV-003 | Auth/HTTP/MCP journeys and tenant-isolation integration evidence |
| AC-006 | Exercise explicit and ambient credential selection with a UID. Selected non-deployment credentials fail; an unused saved login does not override an explicit deployment key. | REQ-014, REQ-018 | Client resolution checks and Rust/Python/TypeScript public SDK journeys |
| AC-007 | Load a mismatched bundle offline without network access; its first online startup fails before writing. Pull by exact UID and load the registered bundle without declaration; attempts to register with a deployment key fail before writes. Registration credentials still register normally. | REQ-016–017, INV-005 | Shared-client integration evidence and positive/negative journeys through each SDK |
| AC-008 | Export stock OTLP over HTTP and gRPC with key plus UID; rows have the selected principal. Missing/conflicting selectors fail before decoding. Revoking the key refuses the next direct export. | REQ-015, REQ-034 | OTLP journeys and malformed-body negative coverage |
| AC-009 | CI declares two versions, then runs both with one deployment key and different UIDs. Their traces, logs, metrics, and observations identify their respective principals. Rotate the key without downtime; the revoked key cannot mint again, while issued bearer tokens retain their ordinary expiry. | REQ-019, INV-002, INV-004, INV-006 | Full CLI/Python release journey plus version-selection journeys through Rust and TypeScript |
| AC-010 | Execute the documented Kubernetes rolling release: one Secret, separate release UIDs, both revisions observable with their own principal attribution. | REQ-029 | Gated real kind-cluster deployment evidence; portable SDK acceptance does not depend on kind |
| AC-011 | Unbound automation/user credentials retain their existing behavior, without a UID. Version-specific role revocation survives redeclaration and affects only that principal's next token. | REQ-013, REQ-018 | Auth regression journeys |
| AC-012 | Concurrent HTTP requests, streaming responses, WebSockets, exceptions, disconnects, cancellation, and invalid aliases close or refuse the appropriate Run without leaking identity. Correctly ordered OTel instrumentation correlates spans without duplicate HTTP instrumentation. | REQ-021, REQ-024–025 | Python ASGI journeys, including FastAPI/Uvicorn and application-owned provider cases |
| AC-013 | Two Gunicorn/Uvicorn workers each start resources in worker lifespan and deliver independently attributable evidence, then drain at graceful exit. | REQ-026 | Python multiprocess journey with documented compatible worker installation |
| AC-014 | Writer/telemetry startup failure sends startup-failed and cleans partial startup; application startup failure remains visible. Shutdown drains final application evidence without blocking the loop; drain failure logs its code and reports shutdown-failed. Existing provider plus default telemetry fails; telemetry=False leaves it untouched. | REQ-022–023 | Python lifespan journeys with real SDK/server, fault coverage, and provider-conflict checks |
| AC-015 | The three audience paths and integration guide use the tested steps; integration shape remains compatible with Guide rules, and generated documentation matches. | REQ-027–031 | Documentation/example journeys and `mise run docs:check` |
| AC-016 | Published docs/generated text, CLI help and remediation contain only the new command; the unused service-attribute constant is gone. Historical change records can still describe the rename. | REQ-020, REQ-032 | Scoped repository checks and public help snapshots |
| AC-017 | With one key and endpoint, concurrent clients selecting different UIDs, disk-cache reuse, transport reuse, and token refresh never borrow another version's subject or permissions. | REQ-033 | Shared-client integration checks and public SDK refresh journeys |
| AC-018 | Issue, inspect, list, and revoke deployment keys through the approved management surfaces; metadata names kind, space, and name rather than an arbitrary version principal, tenant isolation holds, and revoking one leaves another usable. | REQ-010, REQ-035 | HTTP, MCP, CLI and SDK credential-management journeys |
| AC-019 | Rehearse rejection and one-time replacement of persisted exact-version credentials; no secret gains authority silently and unsafe release skew is refused before readiness. | REQ-036, INV-003 | Migration/rollout rehearsal and release compatibility evidence |

## Scope and non-goals

Scope includes local CLI/MCP setup, declaration output, Service/Agent deployment
credentials and version selection, shared-client and SDK projections, Python
ASGI integration, and the documentation and authority updates needed to teach
those paths. ASGI and state-loading requirements apply to the existing Service
root supported by WyrdState; Agent key support does not add a new standalone
Agent state-loader contract.

Excluded:

- Changes to trusted issuers, workload bindings, or `WYRD_WORKLOAD_TOKEN`.
  They stay supplementary and outside the deployment flow.
- Attributing records from one exporter shared by several Services, such as a
  node-level OBI DaemonSet, to individual Services. That needs delegated
  collector identity and its own specification.
- Coupling `wyrd declare` with `wyrd pull`.
- GenAI semantic-convention mapping of OBI spans.
- Launching or controlling a coding agent from `wyrd init`.
- HTTP span instrumentation of our own, framework integrations other than
  ASGI, and TypeScript web-framework integrations.
- Changing how Card attribution (`wyrd.card_uid`) or Run correlation works.

## Decisions confirmed during review

The maintainer approved the proposed scope and two parallel implementation
tasks, confirmed that deployment keys work across versions, required no key
migration, and approved keeping registration separate from deployment loading.
Revision 5 records those decisions and the credential-management projections.
Kind, space, name, version and UID remain the domain vocabulary.

The existing pull and offline bundle-loading workflow supplies the separate
read-only loading path. It avoids introducing a second state loader or making
`declare` depend on the credential for its meaning. The one-time cost is
replacing old Card-bound keys and deploying the matching UID configuration.

## Open material decisions

None. Explicit maintainer approval applies to the behavior and decisions
recorded in revision 5. Task planning and the independent comprehension pass do
not constitute implementation readiness or permission to commit.

## Research basis and authority links

Repository inspection was static; existing behavior is evidence of owners and
gaps, not authority over the proposed design.

| Source | Finding relevant to this proposal |
|---|---|
| [Wyrd design](../../../architecture/wyrd-design.md), Doctrine, SDK construction, authentication, and trace context | Exact-version scope, offline local loading, the current online declare/pull workflow, and application-owned instrumentation remain constraints. |
| [Security posture](../../../architecture/wyrd-security-posture.md), principal lifecycle and API keys/tokens | Roles belong to principals; invalid keys have indistinguishable errors; revocation does not withdraw tenant bearer snapshots. |
| [Bifrost design](../../../architecture/bifrost-design.md), system boundary and row identity | Network authentication remains server-owned; publisher identity is server-stamped before admitting payloads. |
| [Deployment and release](../../../architecture/operations/deployment-and-release.md), local bundles, migrations, and rollout | Local bootstrap has external prerequisites; persisted identity changes need rollout and compatibility decisions. |
| [State owner](../../../crates/shared/wyrd-client/src/state.rs), `from_path`, `declare_with` | Offline loading does not construct a client; online declaration currently owns declare, pull, and load with one client. |
| [Auth owner](../../../crates/shared/wyrd-client/src/auth.rs), `AuthMiddleware` | API-key caching and renewal already exist; selected UID must participate in their identity. |
| [Issuance contract](../../../crates/wyrd-spec/src/auth/issue_key.rs) and [principal routes](../../../crates/wyrd/wyrd-server/src/components/principals/routes.rs) | Current responses and management target a single principal; deployment-key lifecycle is a required boundary change. |
| [Local bootstrap](../../../crates/wyrd/wyrd-cli/src/server/dev.rs) and [approved bootstrap spec](../local-server-bootstrap/spec.md) | Extend the readiness handoff while preserving bootstrap semantics; reconcile approved overlapping requirements. |
| [Python state](../../../sdks/wyrd-sdk-python/src/state/mod.rs), telemetry and shutdown | Reuse existing lifecycle, provider-conflict errors, and authored Python public contracts. |
| [Docs rules](../../../docs/AGENTS.md) and [docsite spec](../docsite-incremental-reset/spec.md) | Tested examples, reader tools, and Guide shape apply; integration is a shape within that taxonomy. |
| [ASGI lifespan protocol](https://asgi.readthedocs.io/en/stable/specs/lifespan.html) | Startup must signal startup-failed explicitly; an exception alone can be treated as unsupported lifespan. Lifespan resources belong to each worker/event loop. |
| [Starlette middleware](https://www.starlette.io/middleware/) | Pure ASGI middleware avoids BaseHTTPMiddleware context propagation limitations; middleware order still matters. |
| [OTLP exporter specification](https://opentelemetry.io/docs/specs/otel/protocol/exporter/) | Standard header configuration supports the explicit workload selector without a Wyrd-specific exporter. |

The imported skill's old agent-rules, doctrine, reference-router and workflow
paths do not exist in this checkout. Their current repository owners are
[AGENTS.md](../../../AGENTS.md), [architecture authority map](../../../architecture/README.md),
Wyrd design's Doctrine, [agent topic guides](../../../agent_docs/index.md), and
[the change workflow](../../spec-driven-development.md). Maintainer-writing and
independent-comprehension guidance was also consulted from the fetched skill
branch's workflow; no unrelated architecture or workflow files were imported.

## Revision history

| Revision | Status | Change |
|---|---|---|
| 1 | draft | Initial specification. |
| 2 | draft | Drop derived OTLP attribution, which conflated Service and Card attribution. Add deployment identity with deployment keys shared across versions and `declare` UID output; nothing has shipped, so no credential migration. The CI and deployment release flow is a required tested scenario, proven by one Python journey because the flow is language-agnostic. |
| 3 | draft | Deployment identity uses Wyrd API keys only: one deployment key per Service across versions, selected to an exact version by `WYRD_WORKLOAD_UID` from the CI declare output. Trusted issuers, workload bindings, and token files leave the flow. Deployed workloads never declare. Key rotation without downtime joins the required scenario. `pull` stays separate and optional. |
| 4 | draft | Research and structure review: explain approach and costs before contracts; retain the combined scope; preserve offline loading; specify ASGI failure signaling, cleanup and context isolation; separate UID-aware caches and role resolution; restore per-surface evidence; record the maintainer's cross-version deployment-key decision and remove the extra terminology; replace unsupported “no open decisions” with credential lifecycle, deployed loading, and compatibility decisions. |
| 5 | approved | Maintainer approved the scope and two parallel tasks, then confirmed reusable deployment-key management, no migration of existing Card-bound keys, and explicit separation of registration from deployed loading. Preserve declare semantics; reuse exact-UID pull and offline bundle loading; fix credential-management projections and one-time key replacement. |
