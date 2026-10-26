---
id: SPEC-service-identity-and-telemetry
revision: 16
status: approved
---

# Stable service accounts, Card identity, and queryable execution ownership

## Problem and proposed behavior

Wyrd currently couples a machine principal to one registered Service or Agent
version. That association supplies both its component-graph authorization and
the relation from its published evidence to its exact root Card. The deployment
key design in guided setup preserves that coupling by making an exact workload
UID mandatory during authentication. It consequently changes authentication for
callers that only have a valid account credential, including gateway clients.

Separate account identity from execution identity without losing graph checks
or exact-version ownership:

- A stable Service account owns credentials and role assignments. Releases do
  not create new authentication principals.
- An execution may select an exact Service root Card. The server
  validates that selection and derives its component scope from the registry.
- Evidence keeps its exact observation subject in `subject_card_uid` and its exact
  execution root in a first-class `workload_card_uid` column. Neither is replaced
  by a free-form service label.
- Card identity becomes tenant, kind, name, and version. Remove `space`; do not
  replace it with another required namespace or team field.
- Keep the current spans, logs, and metrics tables. Rename `card_uid` to
  `subject_card_uid`; add useful OTel resource columns, explicit workload attribution, and a
  layout optimized for time, service, and environment filters.

For example, `checkout` v1 uses Model M1 and v2 uses M2. Both deployments use
account P and its key K. Their tokens have the same account and permissions,
but select different root Card UIDs and signed component graphs. A span about
M1 records `subject_card_uid=M1`, `workload_card_uid=checkout-v1-uid`, and P as its direct
subject and actor. It cannot be attributed to the v2 graph. The Service page
can include both releases or select one without inventing another principal.

An administrative User is different from a restricted runtime account. An
authorized User may use an existing access token and `WyrdState`, emit evidence,
and invoke verification without first acquiring a machine identity or selecting
a workload root. Root absence alone never denies an otherwise authorized call.

Table definitions also need one identity across ingestion and storage. Native
schema, partition-spec and sort-order IDs do not identify a Bloom-only change.
Use one `table_version` for the complete definition and retain its native
Iceberg metadata. Clearing Blooms from version 1 creates version 2; data
accepted under version 1 still recovers and publishes under version 1, and an
identical accepted retry keeps that binding after a partition edit.

### Value, costs, and limits

The developer configures credentials and permissions once, keeps exact-version
verification, and filters Service evidence without joining a release-specific
authentication identity. Flat Card references remove a required authoring
concept. Operators retain account and credential accountability across releases.

This is a coordinated v0.3.0 contract and schema replacement, not an
optional-field-only auth patch. No historical data or credential migration is
required; development installations may reset to the new baseline. It changes identity references, account lifecycle, signed scope,
verification activity and authority recovery, system/dynamic table schemas,
physical partition identity, layout evolution, and queries.
The new baseline uses one public `bifrost` catalog and creates Cards without
space. Existing development databases, tables, and credentials need no
conversion or backfill. Future runtime layout evolution is a distinct
capability and retains the specified durability guarantees.

Versioning requires an Iceberg SQL catalog fork extension for atomic
definition-reference publication, retained metadata roots and a coordinated
wire/persistence change. It removes copied layout JSON and redundant identity
work without adding PostgreSQL to normal ingest. The existing pre-ACK fence
remains and is batched inside its existing transactions. Throughput is
unmeasured until the benchmark rule below runs; no regression beyond 5% is
allowed.

Selecting a UID is an authorized declaration of execution identity, not
attestation of the executing binary.

## Scope and authority

This packet consists of this specification and three normative supplements:
[authentication contracts and flows](supplements/auth-system.md),
[telemetry schemas and production](supplements/telemetry-schemas.md), and
[unified tables, physical layouts, and correlation](supplements/table-storage-and-correlation.md).
All three supplements and this specification are revision 16. The maintainer
explicitly approved the two telemetry setup paths in revision 16. The existing
Service-only configuration, shared attribution schema, sorting and Bloom
defaults remain unchanged.
TASK-001/TASK-004 are treated as completed and TASK-002 as implemented baseline
work by caller instruction; their historical evidence is not re-certified. TASK-009 supplies
the bounded Service-only/configuration follow-up to TASK-002. TASK-003 owns the
shared schema/default changes over TASK-004's generic storage engine; no
additional storage-engine task is required. Retained single-table-version
publication and tenant configuration remain governed by revision 13's contract.
TASK-004-R2 is part of that assumed completed baseline; its review remains
historical evidence, not a new PASS claim. Remaining task amendments are proposed.

The requested change deliberately revises the current owners:

| Owner | Current contract to replace | Required replacement |
| --- | --- | --- |
| `architecture/wyrd-design.md`, Doctrine and Runtime identity | Registration creates version-bound Service/Agent principals; deployment keys select them. | Stable Service accounts; validated exact root context independent of account identity. |
| `architecture/wyrd-design.md`, Card references, declaration, config | Space participates in identity and authoring defaults. | Tenant/kind/name/version identity and references without space. |
| `architecture/wyrd-security-posture.md` | Separate Agent principal kind, persisted version binding, principal-free deployment credentials. | One nonhuman Service account kind, principal-owned credentials, explicit account restrictions and session root scope. |
| `architecture/bifrost-design.md` | Publisher-to-root attribution depends on version principals; current signal layouts and incomplete composite layout identity. | Shared attribution and table rules, with one complete table version, retained native metadata and atomic version/reference publication under the existing engine owners. |
| Root `AGENTS.md` and SDK/schema documentation | CardRef includes space. | The same new CardRef contract on every public surface. |

These are proposed authority amendments, not claims that current architecture
already permits the new behavior. Implementation must update the owning prose,
schemas, examples, and contributor rules together; no parallel old contract or
compatibility alias remains in the fresh baseline.
The Bifrost authority edits for the table version are part of the same
approved revision.

On approval, this packet supersedes only the conflicting deployment-identity,
credential-retirement, and unchanged-attribution requirements of
`SPEC-guided-setup` revision 5 and TASK-002. Local setup, explicit declaration,
read-only deployment loading, ASGI lifecycle, and graceful draining remain
independent capabilities. The existing packet is not edited or superseded by
this draft alone.

## Definitions

| Term | Meaning |
| --- | --- |
| Principal | The authenticated account, with one ID and existing role-derived permissions. |
| Service account | The stable nonhuman principal used by applications, Agents, CI, or automation. |
| Root | One exact registered Service Card version selected for execution. Agents remain component subjects, never workload roots. |
| Family | A tenant-local Service name across versions. The restriction wire retains `kind: Service`; Agent is not an allowed family kind. |
| Graph scope | The root plus its reachable observation-target Cards, resolved to exact registry UIDs by the server. |
| Observation subject | The exact Card a row concerns; it may be a component shared by several roots. |
| Owner | The exact root whose execution or verification binding the record belongs to, not ownership of another independent Card. |
| Table version | One positive signed 64-bit integer identifying a complete immutable definition within an immutable tenant-bound table identity; unrelated to Card versions, snapshots or publication progress. |

## Required behavior

### Accounts, credentials, and ordinary callers

- **REQ-001** Keep User, TenantAdmin, platform identity, and internal System
  authentication and authority semantics. A tenant administrator remains
  tenant-bound; platform authority never becomes tenant authority.
- **REQ-002** Use Service as the tenant's nonhuman principal kind. Remove the
  Agent principal kind from new public contracts and issuance. Agent Cards,
  their graph relationships, and verification bindings remain first-class.
- **REQ-003** Creating a Service account assigns its principal ID once. Card
  registration and deduplication create no authentication principal or role
  assignment. Registering another release never changes the account ID.
- **REQ-004** Every API key has one owning account, independently of release.
  Issue, list, overlap during rotation, revoke, expiry, and one-time plaintext
  return use the existing principal credential lifecycle. Do not introduce
  principal-free deployment keys. Within the new baseline, registering another
  release does not revoke a valid key or require rotation.
- **REQ-005** Resolve operation permissions from the authenticated account's
  existing role grants. There are no per-release role assignments and no
  version-principal permission lookup. Selecting a root grants no permission.
- **REQ-006** A Service account may be explicitly limited to one workload
  family. This durable restriction is separate from the selected version and
  is returned by account inspection. Deliberately unrestricted automation
  remains representable. The fresh baseline needs no migration-only exact-root
  restriction; exact versions remain execution context. The auth
  supplement defines account restriction creation, inspection, replacement,
  and the unique family-to-runtime-account association.

### Root selection and authorization

- **REQ-007** Keep API-key exchange at the existing `/auth/token` endpoint.
  Its optional `workload_uid` selects execution context, never a different
  principal or tenant. Without that field, ordinary account authentication
  retains ordinary account behavior. Family accounts never silently select
  latest or infer a release from their key.
- **REQ-008** A selected root must be an active, registered Service
  in the authenticated tenant and allowed by the account's restriction. Unknown
  and foreign-tenant roots have indistinguishable refusals. The server resolves
  the exact graph using the existing graph-scope owner and signs the root and
  authoritative member UIDs with the token. Account ID and roles stay fixed.
- **REQ-009** Graph scope continues to bound Card-attributed emission and
  verification for restricted machine execution. Operation permission and the
  applicable Card restriction must both pass. Retain the current
  observation-target kind set and traversal limits; control-plane peers do not
  enter the emit scope merely because they are referenced.
- **REQ-010** Root absence is not a universal authorization failure. A User,
  TenantAdmin, or unrestricted automation account with sufficient grants may
  operate on registered Cards in its tenant without a root. In particular, a
  User with administrative wildcard grants can emit and verify without a
  workload UID. A family-restricted account cannot turn into unrestricted
  automation by omitting its root: Card-attributed writes or verification
  requiring its graph refuse missing context. Otherwise permitted generic
  operations and telemetry without Card/owner attribution may still succeed.
  The signed context must retain an account's restriction even when no root is
  selected; a stateless verifier cannot interpret an absent root as proof of
  unrestricted access.
- **REQ-011** Reuse the optional `x-wyrd-workload-uid` request context on HTTP
  and OTLP for callers using an existing bearer token, including Users. The
  server authorizes that context under the existing identity; it does not
  impersonate a Service account. A root already fixed by a signed token cannot
  be changed by a header. Bearer-only requests without the header remain valid.
  HTTP and gRPC use the same header/metadata spelling; token exchange keeps the
  typed body field `workload_uid`. MCP and other typed invocation surfaces
  expose optional `workload_uid` only where an operation needs explicit root
  context, with the same server checks and no client-supplied principal/tenant.
- **REQ-012** Administrative authority permits operations outside any one
  Service graph; it does not permit false ownership. When a caller claims an
  exact root owner for Card-attributed evidence or a verification binding,
  subject membership and binding occurrence must agree with that root. An
  administrator may instead operate without ownership context or select the
  correct root. Merely attaching context never restricts unrelated authorized
  registry or table-query operations.
- **REQ-013** WyrdState and its immutable Run views supply their loaded
  Service root context automatically when making owned requests. Loading
  a bundle remains offline and does not authenticate or register anything.
  Component views retain the Service root while changing observation `subject_card_uid`.
  Context cannot mutate a shared ambient client or leak between concurrent
  states. Explicit workload configuration remains available to non-state
  clients and raw OTLP callers.
- **REQ-014** Renewal preserves the selected root; token caches distinguish
  rootless and different-root contexts in addition to their existing keys.
  Delegation preserves account subject/actor identities, the exact permission
  intersection, and the acting workload's validated root/scope. Queued work
  preserves its accepted authority and root through restart and retry rather
  than inferring a root from a stable principal ID. No new System token or
  authority fallback is introduced.

### Verification ownership and runtime activity

- **REQ-015** Bindings continue to identify exact owner Card versions and
  exact subject occurrences. Direct, submitted, group-end, and scheduled
  verification preserve `workload_card_uid`, subject `subject_card_uid`, and Verifier UID
  independently. Shared component evidence cannot be assigned to a different
  root merely because that root also references the component.
- **REQ-016** Scheduled verification has an explicitly associated stable
  authorizing Service account for its workload family. That association is
  unique and administratively managed through account binding, not chosen from
  the latest caller. Other authorized accounts and Users may perform direct
  work without replacing scheduled authority. In the fresh baseline, configure
  that association before scheduled activation; no historical owner mapping
  is required and differing grants are never silently combined.
- **REQ-017** Track qualifying runtime activity by exact root and its
  associated account. Activity for v2 cannot keep v1's schedule active. Missing
  or inactive authority accepts no scheduled work, and SYSTEM adds no
  permission. Use the existing accepted-authority snapshot contract for
  queued execution and PostgreSQL-owned coordination time.

### Card identity without space

- **REQ-018** Registered identity and uniqueness become
  `(tenant_id, kind, name, version)`. CardUid remains the exact-version key.
  Kind remains part of identity; similarly named different kinds do not merge.
- **REQ-019** Remove space from Card metadata, CardRef, declaration and pull
  selectors, relationship identities, config/default inheritance, canonical
  reference text, CLI flags, SDKs, HTTP/MCP schemas, and examples. A CardRef is
  kind, name, one version, and optional UID. The canonical reference spelling
  is `Kind/name@version`; paths remain loader-only.
- **REQ-020** Do not introduce a required project, team, environment, or
  qualified-name substitute. Optional OTel `service.namespace` is descriptive
  resource data and does not participate in Card lookup, graph authorization,
  or tenant selection. Existing labels and annotations remain available.

### Signal storage and column production

- **REQ-021** Preserve `bifrost.traces.spans`, `bifrost.logs.records`, and
  `bifrost.metrics.points` as the canonical tables. The supplement enumerates
  every current and proposed field, type, nullability, and production rule.
  Rename existing `card_uid` to `subject_card_uid`; other existing field names
  and nested types remain. No blanket `otel_*` rename,
  unified records view, or backend-specific ingestion vocabulary is added.
- **REQ-022** Use the shared managed `workload_card_uid` on all three signals,
  as part of the system/dynamic envelope in REQ-034.
  Stamp it from validated request/execution context. Public writers cannot
  supply that managed column or establish it through resource labels. Native
  component observations preserve their exact subject under the renamed
  `subject_card_uid`; generic telemetry
  can be unowned; configured application records outside a component Run use
  the Service itself as subject. Unconfigured generic telemetry may lack a subject.
- **REQ-023** Add the specified resource promotions and span/log convenience
  fields. Compute them from canonical raw values using the supplement's
  precedence, type, and duplicate-key rules. Retain source attributes with
  their types; extraction does not delete or stringify them. Every ingestion
  path and raw Arrow admission must produce or validate the same values.
- **REQ-024** Apply the supplemental schemas consistently to SDK preparation,
  OTLP HTTP/gRPC, native Bifrost admission, internal capture, Scribe recovery,
  hot and published reads, compaction, table describe, and generated contracts.
  A schema change cannot bypass server-managed fields, accepted-batch
  idempotency, Variant validation, or tenant isolation.
- **REQ-025** Materialize service/environment as typed query columns. Use the
  storage supplement's nine-table ledger as the system default recommendation.
  System and dynamic tables share layout validation and authorized tenant
  configuration, including partition and Bloom overrides. System defaults
  remain time-only; an approved tenant override may add dimensions. No new
  partition default is implicitly approved by approval of this capability.
- **REQ-026** Support direct Service-name aggregation across versions and exact
  Service-version or UID drilldown on every attributable table. Subject-family
  filters/grouping use `subject_kind`, `subject_name` and optional
  `subject_version`; exact `subject_card_uid` remains available. Combine Service,
  subject and environment predicates independently. No registry join or mandatory
  version-UID discovery is needed for name queries. Registered Service pages use
  validated workload attribution; resource labels alone do not establish membership.
  Preserve existing table permissions and tenant isolation.

- **REQ-027** Install one fresh canonical schema and contract. Existing
  development state may be recreated; do not require historical data,
  principal, credential, field-ID, or queued-work migration. Future field-ID
  and layout evolution within the new baseline remains covered by REQ-038.
- **REQ-028** Produce truthful workload/subject/account attribution for all
  newly accepted data. Unknown nullable correlations remain null; no backfill
  or inferred attribution is required for discarded pre-change state.
- **REQ-029** The fresh registry uses tenant/kind/name/version identity with
  no space. No space compatibility aliases, collision mapping, or rewritten
  old declaration hashes are required.
- **REQ-030** The fresh credential model has principal-owned keys and explicit
  family runtime associations. No principal-free-key conversion, Agent-account
  migration, old-token parser, or credential-owner mapping is required.
- **REQ-031** Ship one coherent baseline across catalog, server, SDKs, public
  schemas, SQL, and validation. Do not implement dual old/new namespaces or
  schemas, compatibility reads, or release-upgrade/backfill machinery for this
  v0.3.0 change. Document development reset/setup; perform no destructive reset
  as part of specification authoring.
- **REQ-032** Rust, Python, TypeScript, HTTP, gRPC, MCP, CLI, runtime schemas,
  and machine-readable docs expose the same contracts and stable error
  semantics. Keep the actual credential chain, including `WYRD_ACCESS_TOKEN`
  and `WYRD_API_KEY`; do not invent `WYRD_API_TOKEN` as an alias.
- **REQ-033** Use the auth supplement's explicit signed Card-access states,
  account reference, per-participant workload UID, and request/response shapes.
  Ordinary operation grants remain in `permissions`; Card-access states only
  bound attribution and verification. A missing or empty scope never silently
  becomes unrestricted access or root-only access. Delegation intersects the
  effective Card limit as well as operation permissions and never derives
  authority from historical `act` entries.

### Unified system and dynamic tables

- **REQ-034** Give all nine system tables and registered dynamic tables the
  storage supplement's seven-field managed envelope. Retain domain fields,
  per-record accepted origin, audit decision identities and unresolved
  sentinels, and nullable unknown correlations. Never replace original actors
  with delivery identities or reinterpret audit targets as observed subjects.
- **REQ-035** Use the exact system ledger and shared writer recipe in the
  storage supplement as defaults; describe/register responses expose the
  resolved schema/layout and matching `table_version`. Use that same version
  in PATCH as `expected_table_version`, with no configuration counter or
  aliases. Authorized tenant configuration may override system partitions and
  Blooms using the same table rules as dynamic tables. Pruning is an optimization, not authority or
  a substitute for exact row filtering.
- **REQ-036** System and dynamic tables use the same table owner/lifecycle and the
  supplemental layout shape, defaults, and limits. Support bounded multi-field
  partitions and explicit scalar sort/Bloom declarations. Refuse invalid
  layouts or excess composite fanout before durable acceptance, with no
  partially accepted batch or inferred resource partitions.
- **REQ-037** Carry the same `table_version` and complete typed partition
  tuples through clients/Gate, Scribe, WAL slices/commits, seals, assembly,
  staging, batch fences, promotion, Oracle pins/peer reads and Forge. Resolve
  the complete definition from its retained native metadata, never current
  Bloom properties or a parallel recipe blob. Native schema/spec/order IDs
  remain physical descriptors. Preserve time bounds, tenant ownership,
  accepted-batch idempotency and null semantics.
- **REQ-038** Support explicit compatible schema/layout evolution with stable
  field IDs and one version allocated atomically with each effective
  definition edit. Retain native metadata under the publication/retention
  contract below. Reject stale new writes; accepted retries use the fence's
  original version through restart, publication and WAL retirement. Preserve
  reads across retained specifications. Re-registration stays idempotent.
  Historical rewrite is explicit maintenance; layout metadata changes alone
  do not claim old files are repartitioned or resorted.

- **REQ-039** Use `bifrost` as the public logical SQL catalog for every built-in,
  dynamic dataset, and operational namespace. Update table references, parsing,
  default catalogs, physical bindings, permission scopes, SDKs, examples, and
  docs consistently. No `vala` catalog alias remains. Internal Vala crate
  ownership is not renamed by this requirement.
- **REQ-040** Adapt the verification engine and all evidence paths to the new
  catalog, shared attribution, stable accounts, and resolved table layouts.
  Preserve exact binding owner/subject/Verifier distinctions, drift subject
  windows, grouped source isolation, and frozen accepted authority. Renaming
  SQL strings alone does not satisfy this requirement.
- **REQ-041** The future implementation plan must end with one separate,
  dedicated docsite task after runtime/contracts/SDK/example behavior settles
  and before final integrated change review. Cover authoring, auth, attribution,
  schemas, query/correlation, table layout configuration, verification, and
  fresh v0.3.0 setup/reset. This specification creates no implementation tasks.

- **REQ-042** Use one creation-default attribution Bloom set on all eight
  attributable system tables: `service_name`, `subject_name`,
  `workload_card_uid`, `subject_card_uid`, `run_id`, plus each table's existing
  signal-specific lookup keys as enumerated in the storage ledger. Verification
  results have ten defaults. Audit retains its own defaults. Tenant replacement,
  empty and omitted semantics remain explicit; there is no hidden mandatory floor.
  Missing filters never change exact results, authority or attribution. A join
  does not promise Bloom pruning without usable scan predicates/runtime filters.

- **REQ-043** Treat verification evidence-read performance as part of the
  table/schema refactor. Inventory and measure result, trace, drift/window,
  grouped, and declared dynamic-table reads through the existing Bifrost owner;
  optimize projection, selective predicates, trusted time/source bounds,
  batching, and supported pruning. Review every system Bloom default against
  its actual/intended queries. Preserve accepted authority, exact binding and
  evidence semantics, live-data visibility, deadlines, and refusal behavior;
  do not add arbitrary lookback bounds or silently narrow evidence to obtain
  faster reads. Reuse/read-view caching requires an explicit consistency scope.

- **REQ-044** The eight observation-attributed system tables sort by environment ASC NULLS
  FIRST, Service name ASC NULLS FIRST, Service version ASC NULLS FIRST, then its
  existing time axis DESC NULLS LAST. These four keys replace previous signal
  tie-breakers. Keep time-only partitions and the shared creation-default Bloom
  rule; `span_id` remains opt-in. Environment, kind and version have no default
  Bloom. Bloom order is not composite-index order. These are approved defaults,
  not a measured speedup or a new partition dimension.

- **REQ-045** Trace checks receive the entire authorized trace visible at the attempt's
  read, including
  spans from different workloads/Cards and untagged spans. Binding ownership
  and subject attribution do not filter the trace read. Authored selectors
  evaluate within that trace document. Do not introduce a workload-only mode,
  silently truncate evidence, or mistake a Card tag for a trace boundary. Reuse one fetched collection
  per trace per attempt, without implicit time-window omission. Late spans
  never silently revise completed results; no completeness claim is made.

- **REQ-046** Treat a queued trace check's not-yet-landed evidence as transient
  availability, preserving the accepted execution and using bounded existing
  retry/backoff under the existing per-attempt timeout (not a new total-lifetime
  deadline). Exhausted unavailable trace evidence is
  inconclusive with an explicit reason; preserve the Verifier's decision rule
  over its other requirements. Failed checks and invalid/auth/
  oversized evidence are not automatically retried as ingestion delay.
- **REQ-047** Apply the storage supplement's agreed safe-edit principles:
  supported transforms/types, compatible nullable additions and stable-ID
  renames, protected system/managed fields and layout dependencies, explicit
  administration authority, and atomic rejection of stale table-version edits through the specified PATCH
  contract. Use the existing table DDL permission, nullable custom additions
  and stable-ID renames; refuse type changes and deletions.
- **REQ-048** Retain hourly defaults for spans/logs/metrics/gateway and dynamic
  tables, and daily defaults for verification/drift/audit as enumerated in the
  system ledger. Withdraw the daily dynamic candidate. Authorized tenant
  overrides remain available under the shared rules; unrelated default changes
  still require explicit approval.

### Configured Service application and shared dimensions

- **REQ-049** Workloads are Services only. Reject Agent roots and Agent-family
  restrictions across issuance, request headers, federation, delegation and
  scheduling, while retaining Agent Cards, component Runs and Agent verification
  subjects. Do not add `workload_kind` or an alternative root-kind contract.
- **REQ-050** Expose `state.configure(service_name, service_version,
  deployment_environment)` on Rust, Python and TypeScript using language-native
  optional arguments/options. Omitted name/version come from the loaded Service
  bundle; explicit values must match it. Environment precedence is explicit
  nonempty value, then nonempty `WYRD_DEPLOYMENT_ENVIRONMENT`, then null. Reject
  explicitly empty values rather than silently selecting a default. Configuration
  is synchronous and local. The first successful explicit configure or implicit
  Run/client/writer/telemetry use freezes per-state context; lazy initialization
  applies the same defaults. Every later configure is a typed conflict, including after
  shutdown; separate states can coexist without global context leakage. Loading
  stays offline. The account authorizes a family; the bundle selects the release.
- **REQ-051** Nine system tables share workload/subject UIDs plus nullable
  Utf8 `service_name`, `service_version`, `deployment_environment`, `subject_kind`,
  `subject_name`, `subject_version`. The six named projections occur once, not as
  duplicate telemetry resource fields. The server validates UIDs and derives the
  five identity descriptors from the tenant registry; environment is descriptive
  accepted context, not authority. Carry context through all observations,
  telemetry, gateway calls, accepted executions, inputs, results, drift and
  trusted delivery. Preserve raw OTel attributes; attributed Service columns
  agree with the UID, while unowned telemetry retains observed Service labels.
  Universal seven-field identity/correlation and audit rules remain intact.
- **REQ-052** A root Run selects the Service subject and its binding; a component
  Run selects that component and its exact binding occurrence without changing
  Service/version/environment. Gateway calls made by verification retain that
  accepted Service/subject context, independently of Verifier or provider/model
  execution identity. Public managed descriptors cannot be forged before ACK.
- **REQ-053** Direct and queued verification capture one context
  {service_name, service_version, deployment_environment} at acceptance. Queued
  work stores it with the accepted execution; both paths write it unchanged to
  the result and inputs rows. Nothing else reads it. Gateway calls and native
  Bifrost gRPC writes carry `x-wyrd-deployment-environment`; OTLP carries the
  `deployment.environment.name` resource attribute; typed/MCP calls use
  `deployment_environment`. Missing environment is not auth failure.
  Do not change schedule fanout, group identities,
  whole-trace evidence selection, direct/queued deadlines or authority.
  Invalid configure fields use `WYRD_CLIENT_400_CONFIG_INVALID`; frozen configure
  uses new `WYRD_SDK_409_STATE_CONFIG_FROZEN`; non-Service roots use existing
  `WYRD_AUTH_401_WORKLOAD_UID_REFUSED`. These errors have typed remediation and
  generated public error/schema closure, without new token descriptor claims.

Telemetry supports two setup paths:

- **Wyrd-managed telemetry.** Configure the loaded state once, then call
  `state.start_telemetry()` (or its language-native equivalent). Wyrd supplies
  `service.name`, `service.version` and optional `deployment.environment.name`
  from the frozen state context to its managed OpenTelemetry setup, and exports
  with the state's validated workload context and existing authentication.
- **Application-managed telemetry.** The application owns its OpenTelemetry
  providers, resources and lifecycle. It supplies `service.name`,
  `service.version` and optional `deployment.environment.name` on its resources,
  and sends spans, logs and metrics through standard OTLP exporters to Wyrd's
  supported endpoints with supported authentication. This path does not require
  WyrdState, `state.start_telemetry()`, or an exporter accepting a state. Wyrd
  does not copy private state configuration into an application-owned provider.

Both paths use the same ingest validation and table projection. An application
may supply the existing workload/subject/Run context where attribution is
wanted; the server validates UIDs, permissions and graph membership. Resource
names and versions never establish Card ownership, and owned promoted identity
columns remain canonical for validated UIDs. When an accepted request supplies
an environment, retain that value as before; without that context, promote the
resource environment with the existing typed stable-key-first fallback, including
on owned telemetry. Preserve raw resource attributes and leave absent environment
null. Environment remains descriptive and grants no permission.

Automatic startup must not silently reuse a provider bound to another state or
replace an application-owned provider. Refuse incompatible startup with existing
`WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`; its remediation is to keep the existing
application-managed setup and use the second path. Wyrd never replaces or flushes
that provider. Native state/Run contexts may coexist without multiple ambient
defaults. No new exporter input, public state client accessor, exporter-specific
identity setting or provider pipeline is required.

### Stock provider SDK calls carry configured context

- **REQ-054** Expose Python `WyrdState.http_auth` and TypeScript
  `WyrdState.httpAuth` as state-bound forms of the existing WyrdClient adapters,
  with no new public `state.client` accessor or provider-specific Wyrd wrapper.
  Construct OpenAI/Anthropic clients once and use the current state/Run context
  on each outgoing request. Accessing the adapter freezes lazy configuration
  but performs no network authentication. Python supports sync/async HTTPX and
  HTTPX2; TypeScript uses the existing fetch extension. Shared Rust state/client
  context remains the owner behind the thin language projections.
- **REQ-055** The state-bound adapter sends one current bearer in
  `X-Wyrd-Access-Token`, replacing placeholder Authorization/x-api-key/
  x-goog-api-key, plus `x-wyrd-workload-uid`, optional
  `x-wyrd-deployment-environment`, subject `wyrd-card-uid`, optional
  `wyrd-run-id`, and active W3C trace context. Outside a Run the subject is the
  configured Service; inside a component Run use its exact subject and Run.
  Resolve context before asynchronous token refresh so it cannot drift across
  concurrent calls. Capture per dispatched request, preserve through transport
  retries, streaming and gateway attempts; fresh credentials never replace
  identity context. An explicit differing Service/subject/Run header
  conflicts with state-bound automatic context and fails locally with
  `WYRD_CLIENT_400_CONFIG_INVALID` before sending; identical values are accepted.
  Preserve explicit W3C trace context under the existing trace precedence rules.
  Never mutate a provider client's shared default_headers/defaultHeaders.
- **REQ-056** Keep both gateway credential modes: a Wyrd token/API key in the
  provider SDK credential slot with explicit context headers, and the automatic
  state-bound adapter with a placeholder provider key. Existing plain
  WyrdClient.http_auth/httpAuth remain supported without inventing a state or Run;
  preserve their explicit subject/Run inputs. Both modes reach the same gateway
  validation and capture. Validate requested subject membership before provider
  dispatch, freeze accepted Service/subject/environment/Run/trace, and derive
  canonical readable columns server-side. Add no readable identity headers.
  All existing routes creating gateway invocations, including provider Batch
  HTTP operations, preserve request context; no new batch-item correlation
  protocol, cross-request deduplication or asynchronous job owner is introduced.
  Consume Wyrd credentials/context at the gateway; never forward them upstream.
  Preserve provider credentials, caller authorization, streaming and capture rules.

The [auth supplement's approved provider-SDK example](supplements/auth-system.md#stock-provider-sdk-context-and-approved-examples)
is the public usage contract, including Python and TypeScript. TASK-009 owns
adapter/API/lifetime closure; TASK-003 owns gateway validation, canonical capture
and row readback; TASK-008 publishes tested examples. This amendment changes no
schema columns, partitions, sorts or Bloom defaults from revision 14.

### Single table version contract

This contract fixes the public and persisted decisions for REQ-035/037/038.
The storage supplement supplies the full alteration and lifecycle rules.

- A newly created table starts at 1. Every effective definition edit increments
  once, including schema/field IDs, partition, sort, Bloom and any writer/layout
  setting. Data, snapshot and housekeeping commits never increment it. No-op
  and invalid edits consume no committed version; there is no parallel
  configuration revision. Existing safe-edit limits and immutable time axis
  remain in force.
- The domain is `1..=9223372036854775807`: Rust `i64`, PostgreSQL `BIGINT` with
  positivity enforced, protobuf `int64` and eight-byte persisted version
  fields. JSON represents both `table_version` and `expected_table_version`
  exclusively as canonical unsigned base-10 **strings**, including `"1"`.
  TypeScript SDK values are `bigint`, converted directly to/from those strings;
  Rust/Python retain exact integers. No conversion passes through a JavaScript
  `number`. Reject zero, negative/out-of-range values, JSON numbers, signs,
  leading zeros, whitespace, fractions and exponent notation. Version
  exhaustion refuses the edit without wrapping or changing the definition.
- Store the decimal value in `wyrd.bifrost.table-version` with all definition
  edits in one Iceberg metadata commit. Publish the retained
  `(immutable table identity, table_version) -> native metadata location`
  reference in the same catalog SQL transaction as the metadata-pointer CAS.
  The Iceberg SQL catalog fork asserts expected version and base pointer at
  that boundary; competing Bloom-only edits cannot bypass it. No later SQL
  projection or second allocator owns this fact. Uncertain commit results
  reconcile exact evidence before success or cache refresh.
- Native Arrow writers obtain the version/schema binding from describe or
  registration, retain it in the shared client and send it on each batch.
  Canonical built-ins use a versioned server projection bound by Gate. New
  batches with a stale version receive the typed table-version conflict
  (HTTP 409 projection); accepted retries resolve the fence's original version
  and still prove unchanged source schema, row order/count and payload.
  Keep structural/type/nullability/reserved-field/value validation; a version
  claim is no substitute for checking data. Do not add a preliminary accepted-
  ID query on successful fresh ingest.
- Always retain the current definition. Retain older native metadata and its
  reference while any WAL, staged-data, fence, promotion/file-lineage or active
  query-pin root or accepted-retry retention remains. Forge expiry/orphan cleanup honors
  these roots even after native metadata-log rollover. There is no invented
  TTL; absence of a proven retention horizon means retain. Snapshot/data-file
  retention and visibility remain separate from definition identity.
  Settled promotion/rewrite lineage still roots the definition while retained
  files require it; settlement alone is not permission to discard the version.
- This is in-baseline evolution: no release migration, backfill, compatibility
  alias, automatic reset or dual on-disk authority. Legacy evidence is usable
  only if its exact definition maps unambiguously to retained version evidence;
  otherwise refuse it. Never guess a version from current schema or Blooms.
- Keep the existing PostgreSQL fence before ACK. Successful normal ingest adds
  no PostgreSQL call or transaction. Each tenant's drained group uses one
  multi-row fence INSERT, one batched conflict read if needed, and batched
  ambiguous-COMMIT resolution, preserving per-batch classification, rollback,
  group order, RLS and poisoning. Transaction count remains unchanged.
  Replacing the fence is out of scope.
- Implementation may run the Bifrost capacity benchmarks
  (`bench:bifrost:write-capacity` native, and `bench:bifrost:query-capacity`)
  and the tracing benchmark (`bench:bifrost:write-capacity -- --fixture
  telemetry`) only when no other worktree is running a build, test, server or
  benchmark; otherwise skip and record the limit. A run compares the reviewed
  baseline and the candidate on the same host and configuration. Any
  throughput metric (rows/s, MB/s, MB/s per core) more than 5% below the
  baseline, or any query latency more than 5% above it, is a regression and
  fails the change. Without a run, throughput stays unmeasured.

## Invariants and non-goals

- **INV-001** Verified credentials alone establish the principal and tenant.
  A root selector, resource attribute, observed Card UID, namespace, or
  environment never establishes either.
- **INV-002** Permissions remain role-derived and server-enforced. Context
  adds no authority; no role-name administrative bypass replaces wildcard
  permission evaluation.
- **INV-003** Account identity, exact execution ownership, and observation
  subject remain independently queryable. None is substituted for another.
- **INV-004** Exact graph scope and verification binding occurrence survive
  credential reuse, concurrency, delegation, batching, retry, and recovery.
- **INV-005** Generic telemetry and privileged rootless use remain valid;
  neither null owner nor null Card correlation becomes fabricated attribution.
- **INV-006** The Rust server owns durable decisions. SDKs propagate context
  and use existing clients; no new auth orchestration, graph service, writer,
  policy engine, or client-owned durable behavior is introduced.
- **INV-007** Permission checks block; audit remains nonblocking. Audit retains
  account, credential provenance when present, and validated root independently
  of the operation's target Card. Ordinary expiry/revocation semantics and
  tenant/platform separation remain unchanged.
- **INV-008** OTel data remains typed and signal-specific. Integer counters,
  nanosecond timestamps, SpanKind, log bodies, nested events/links, metric
  temporality, summaries, and exemplars are not discarded to copy a UI schema.

Non-goals: binary/runtime attestation, tenant model changes, a renamed space
concept, one physical records table, arbitrary-attribute columns, a new public
auth endpoint, per-key permission overrides, release-specific credentials,
the remaining guided-setup/ASGI implementation.

## Acceptance and credible evidence

Each row states the observable result, not a prescribed fixture or private
implementation. Public behavior needs real SDK-to-server/Postgres journeys on
each shipped surface; pure extraction additionally needs in-process cases.

| Criterion | Situation and expected observation | Obligations |
| --- | --- | --- |
| **AC-001 Normal auth** | A User token and WyrdState emit and verify with sufficient grants and no machine/root requirement. A wildcard-authorized User operates on registered Cards outside any one Service graph. A User without the operation grant is refused. Admin, platform, and System boundaries remain distinct. | REQ-001, 010–012, 032; INV-001–002, 005 |
| **AC-002 Reusable account** | Declare two releases with different components, create one account/key, and run both concurrently. Both authenticate as the same account; each token and row retains its own root. v1 cannot attribute to v2-only components. No registration creates a principal or role grant. | REQ-002–009, 013; INV-003–004 |
| **AC-003 Missing and false context** | A restricted account cannot gain arbitrary Card access by omitting root context. Authorized generic calls still work. A foreign/unknown root is indistinguishably refused. An administrative User cannot label an out-of-graph row as belonging to a selected root, but can perform the separately authorized operation without that false owner. | REQ-006–012, 022; INV-001–005 |
| **AC-004 Lifecycle and delegation** | Renew and rotate keys while two releases run. Preserve account, root, and permissions. Delegate a User call through the Service, then submit/restart/retry work: the subject, actor, accepted authority, and root remain correct without rereading a removed version principal. Credential revocation keeps existing expiry semantics. | REQ-004–005, 014; INV-004, 007 |
| **AC-005 Verification** | Two roots share a component but have different binding occurrences. Runs, inputs, and results remain attached to the chosen owner/binding. Activity from v2 alone does not activate v1 schedules. Another caller cannot silently become scheduled authority. | REQ-015–017; INV-003–004 |
| **AC-006 Flat identity** | Register/pull/reference Cards using kind/name/version without space across SDKs, CLI, HTTP and MCP. Two tenants remain isolated; two same-kind/name/version Cards in one tenant conflict as defined by registration. Removed space forms fail with actionable typed validation. | REQ-018–020, 032; INV-001 |
| **AC-007 Schemas and production** | Describe and write/read each complete supplemental schema. Repeated and wrongly typed attributes follow its exact rules. OTLP and native Arrow produce equal promoted values; forged derived/managed values are refused before acknowledgement. Exact counters, nested fields, and timestamp precision survive hot reads, publication, restart, and compaction. | REQ-021–024; INV-007–008 |
| **AC-008 Queries and layout** | Direct Service-name queries return both releases, version/UID filters separate them, kind/name/version subject filters remain independent, and environment filters apply across telemetry, gateway, verification inputs/results and drift without registry joins or unrelated decoding. Inspect actual partition manifests and pruning on representative same-service and cross-service queries; report file counts/sizes and planning overhead, not an invented latency target. | REQ-025–026; INV-003 |
| **AC-009 Fresh baseline** | Start from fresh/reset development state and create accounts, Cards, system and dynamic tables using only the new catalog/schema/auth contract. No historical backfill, legacy key/token conversion, dual reads, or space/catalog aliases are required. | REQ-027–031; INV-001–008 |
| **AC-010 Contract closure** | Owning architecture, public schemas, examples, SDK declarations and error contracts agree. Every surface round-trips table versions above 2^53 through the maximum without precision loss: JSON uses decimal strings and TypeScript uses bigint. JSON numbers and malformed/range-invalid versions refuse. Existing unrelated guided-setup work remains independently valid; no broader credential hides a failure. | REQ-032, 035–038; INV-006 |
| **AC-011 Complete auth contract** | Exercise the auth supplement's principal/grant matrix, account restriction administration, exact request/claim shapes, three Card-access states, delegation intersections, external assertion mapping, gateway/OTLP ingress, and queued/scheduled authority recovery. Malformed or inconsistent claims are refused, and accepted token sizes remain within the existing verifier limit. | REQ-001–017, 027–033; INV-001–007 |
| **AC-012 Unified tables and layouts** | Describe all nine system tables and dynamic tables; observe the shared envelope and exact layout/Bloom/writer policy. Exercise custom multi-field/null partitions, invalid declarations, fanout refusal before ACK, original audit/outbox attribution, and dynamic-table correlation with verification and telemetry. | REQ-024–027, 034–037; INV-001–008 |
| **AC-013 Evolution and recovery** | Rename fields with stable IDs/values; change partition grouping, then retry original bytes/ID under the fence's version with warm/cold caches, provisional WAL attempts, restart, publication and WAL retirement. Identical retries preserve original acceptance/time/origin; altered content refuses. Stale new batches refuse without residue. Retained mixed-spec reads/compaction keep exact rows and truthful old-file layout. Unmappable legacy evidence refuses without guessing/backfill. | REQ-027–032, 035, 037–038; INV-001–008 |
| **AC-014 Catalog and verification** | Use only `bifrost.*` names for built-ins/datasets and permission scopes. Direct, queued, grouped, drift, and trace verification retain their accepted account/workload/subject/binding semantics across the renamed contracts. | REQ-039–040; INV-001–008 |
| **AC-015 Final docsite task** | The implementation plan's last task is a separate docsite task covering the shipped contract and runnable examples, with the owning documentation checks before integrated review. | REQ-041 |
| **AC-016 Correlation and configurable Blooms** | Describe verification results and observe exactly the ten default columns. Replace, clear or reorder declared Blooms at a new table version without floor reinsertion or partition/sort change. Staged old-version data recovers/publishes with its original policy; new batches use the new one. Account/request columns remain stored; exact query results agree across complete/partial/empty/reordered policies, missing historical filters and compaction. | REQ-035–038, 042; INV-001–008 |
| **AC-017 Verification evidence lookup** | Exercise Service-name aggregates, version/environment and kind/name/version subject drilldowns using direct-name and exact-UID predicates, plus exact result, repeated-trace, subject/series-window, grouped input/result, and dynamic-table evidence reads across hot/published/live sources. Record planning/source/scan timings, bytes/files/groups examined and pruned, repeated query counts, and end-to-end latency. Demonstrate narrower projections/bounds and safe reuse where justified while obtaining the same authorized evidence; do not claim a millisecond lookup SLO from execution deadlines. | REQ-026, 035–038, 040, 042–043, 049–053; INV-001–008 |
| **AC-018 Lean defaults and trace correlation** | Describe all nine tables and observe the agreed complete per-table Bloom sets; spans/logs omit span_id by default but accept an authorized explicit configuration. Trace, span, Service/environment, and time filters return the same authorized rows with missing/disabled filters; no hidden time window or order-dependent Bloom semantics appear. | REQ-035–038, 042–044; INV-001–008 |
| **AC-019 Whole-trace evidence** | Verify a Service/component binding against a trace containing its own spans, other workloads' spans, and untagged spans. All authorized visible trace spans, including retained spans outside the old fixed time window, remain available to the authored selector; checks in one attempt share the fetched collection and later spans do not mutate a completed result; the result retains the binding owner/subject while every source span retains its own attribution. | REQ-014–015, 040, 043, 045; INV-001–008 |
| **AC-020 Evidence-availability retry** | Queue a verification before its trace lands: no immediate completed inconclusive result is emitted, and the same execution resumes after bounded backoff when evidence appears. Exhaustion produces an inconclusive trace check (and an inconclusive trace-only result), preserving other requirement judgments; failed checks and invalid/auth/oversized evidence do not enter not-yet-landed retries. Accepted identity, authority, inputs, and configured per-attempt timeout remain unchanged. | REQ-014, 040, 043, 045–046; INV-001–008 |
| **AC-021 Safe edits and agreed defaults** | Create system/dynamic tables at table_version 1 with agreed defaults. Effective edits increment once; no-op/failed edits and data/snapshot/housekeeping commits do not. Competing edits, including Bloom-only edits, publish exactly one winning definition/reference in the catalog CAS transaction; stale/overflow edits change nothing. Preserve safe types, protected fields and dependencies. | REQ-035–038, 047–048; INV-001–008 |
| **AC-022 Definition retention** | Retain the current version after data-only metadata-pointer movement. While WAL/stage/fence/promotion/query-pin or accepted-retry roots remain, expiry/orphan cleanup and metadata-log rollover cannot remove older definitions. After roots and retry retention are provably gone, eligible noncurrent metadata may be pruned; unavailable evidence never proves safety. | REQ-037–038 |
| **AC-023 Grouped fence semantics** | A group of N fresh batches uses one multi-row INSERT per tenant with unchanged tenant transactions. Mixed duplicate/conflicting rows retain per-batch results and group order; contradictions roll back fresh rows. Batched uncertain-COMMIT reconciliation preserves absent/retry and poisoning behavior, RLS and no ACK before durable WAL/fence. Verify operation counts without claiming measured throughput. | REQ-037–038; INV-001–002, 006–007 |
| **AC-024 Configured Service journey** | Load Service A v1/v2 with one Service-bound and one Agent-bound Verifier, use one family account, configure defaults/explicit environment, start telemetry and emit both subjects including gateway-backed checks. Every signal has the correct immutable Service/version/environment and subject descriptors; same-named different Card kinds remain separate. Concurrent states, mismatches, late configure, forged fields, Agent roots/families, restart/retry, raw OTel preservation, unowned rows and environment tag readback have explicit positive/negative proof. | REQ-013–015, 022–026, 049–053; INV-001–008 |
| **AC-025 Stock provider SDK context** | Create long-lived OpenAI and Anthropic clients using state.http_auth/httpAuth, then call outside a Run and within Service/Agent Runs. Python sync/async and TypeScript preserve exact Service/version/environment, subject and Run across concurrent states, token refresh, transport retries and streaming. Conflicting Service/subject/Run context fails before dispatch; wrong-kind/foreign/out-of-graph raw headers fail gateway admission without an upstream request. Both credential modes and Batch HTTP operations use the same server projection; queries expose canonical readable columns and original context across delivery/recovery. Provider receives only its operator credential and permitted protocol headers, never caller credentials or Wyrd context. | REQ-054–056; INV-001–008 |
| **AC-026 Two telemetry setup paths** | Wyrd-managed telemetry uses the frozen state Service/version/environment. Independently, an application-owned OpenTelemetry provider sends all three signals through standard OTLP with resource Service/version/environment, supported credentials and optional validated Card context, without WyrdState or a new exporter API. Both paths retain typed signal data and the same server attribution/permission checks; missing environment remains null. Automatic startup against an application provider refuses without replacing or flushing it. | REQ-050–053; INV-001–002, 005, 008 |

## Approval and material decisions

None remain for this revision. The maintainer additionally approved state-bound
stock-provider adapters and the accompanying code example, with explicit client/
gateway ownership and validation on both credential paths. The maintainer approved the
Service-only model, immutable `configure()` defaults, shared Service/subject/
environment context across all nine system tables, direct name/version
aggregates and drilldowns, time-only partitioning, the common four-key sort,
and common attribution Blooms plus existing signal lookup keys. TASK-002 and
TASK-004 are assumed completed; TASK-009 is the runtime/context follow-up and
TASK-003 owns canonical schemas/defaults. No storage-engine follow-up is needed.

Revision 13's single table version, exact integer/wire representations, atomic
catalog publication, accepted-retry preservation and rooted retention remain.
The revised shared context and defaults replace older conflicting auth,
telemetry and layout clauses. Whole-trace evidence and availability retries
retain their existing scope/deadlines; no schedule fanout or migration/backfill
is introduced. Physical performance remains unmeasured until implementation.



## Research and inspected owners

Repository facts are distinct from the proposal: current version principals,
scope derivation, optional correlation, the existing schemas, and hourly
partitions are implementation/authority facts. Stable accounts with selected
root context, flat Card identity, and the supplemental additions are proposed
changes.

- [Wyrd design](../../../architecture/wyrd-design.md): Doctrine, Runtime
  identity, Observation identity, Verifier, declaration, references, config.
- [Security posture](../../../architecture/wyrd-security-posture.md): account
  lifecycle, tokens, delegation, RBAC, tenancy, audit.
- [Bifrost design](../../../architecture/bifrost-design.md): row identity,
  Variant, canonical signal projection, physical layout, ingest and query.
- Existing owners: `wyrd-runtime::Principal::card_attribution`,
  `wyrd-auth::TenantTokenIssuer`, `wyrd-auth::card_scope`, principal credential
  routes, `wyrd-client` credential/state/Run owners, `wyrd-sql` registry and
  verification activity/queue owners, and the table-owned signal ledgers and
  projections cited in the supplement. Extend their complete workflows.
- [OTel service conventions](https://opentelemetry.io/docs/specs/semconv/resource/service/)
  and [deployment environment](https://opentelemetry.io/docs/specs/semconv/resource/deployment-environment/):
  optional resource identity dimensions, not required registry spaces.
- [MLflow model-version API](https://mlflow.org/docs/latest/api_reference/rest-api.html#get-modelversion)
  and [BentoML model management](https://docs.bentoml.org/en/latest/build-with-bentoml/model-loading-and-management.html):
  conventional name/version addressing without a separate required space.
- [Iceberg partitioning](https://iceberg.apache.org/docs/latest/partitioning/):
  logical columns and evolvable physical transforms are separate contracts.
- [Spec-driven development](../../spec-driven-development.md): approval and
  independent comprehension, not implementation authorization.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | New replacement proposal: stable accounts, validated exact-root scope/ownership, removal of space, and complete additive signal schemas without cosmetic renames. Explicitly preserves authorized rootless User/admin access. |
| 2 | draft | Adds the full auth supplement: account and credential lifecycle, exact token and claim contracts, explicit Card-access states, all principal/grant flows, delegation/recovery, schedules, ingress, errors, and migration. Telemetry schemas are unchanged. |
| 3 | draft | Names attribution explicitly: existing signal `card_uid` becomes `subject_card_uid`; proposed `owner_card_uid` becomes `workload_card_uid`. Requires coordinated column/schema/query migration without aliases or changed identity semantics. Records trace-evidence selection as an additional open decision. |
| 4 | draft | Records the unified system/dynamic storage supplement: shared envelope, all nine exact layouts, bounded custom multi-field partitions, writer policy, correlation, and explicit evolution. Replaces the old Service/environment partition proposal and records corrected trace-selection guidance plus remaining public-contract questions. |
| 5 | draft | Records authorized tenant partition/Bloom overrides under shared table rules, the public bifrost catalog, current-vs-proposed writer classification, verification impacts, a separate final docsite task, and a resettable v0.3.0 baseline without release migration. Partition defaults remain subject to explicit approval. |
| 6 | draft | Adds query-driven per-table Bloom policy and eight verification-result defaults; removes mandatory global Bloom indexing while preserving the shared schema, configurable table rules, and existing partition proposals. |
| 7 | draft | Adds evidence-read profiling and optimization to the table/schema refactor, covering dynamic queries and existing specialized readers without changing partition/Bloom defaults or evidence semantics. |
| 8 | draft | Records approved lean system Bloom sets and span_id opt-in, with clustering/order/query explanations. Existing partition/sort proposals and separate approval requirements remain unchanged. |
| 9 | draft | Settles whole-trace verification and removes workload-only scope selection; retains explicit availability/late-arrival/read-consistency and performance-budget questions. No partition/Bloom changes. |
| 10 | draft | Records measured before/after evidence-read validation without inventing a lookup SLO; documents the current trace-not-landed retry gap and leaves its readiness/retry policy open. Partition/Bloom defaults are unchanged. |
| 11 | draft | Records agreed hourly/daily defaults, safe editing and concurrency principles, and bounded queued missing-trace retries with final inconclusive exhaustion. Narrows the remaining open contracts without authorizing implementation or changing runtime configuration. |
| 12 | approved | Records the maintainer-agreed table editing API/types, conservative schema evolution, existing DDL authority and revision preconditions. Settles visible-trace reads, per-attempt reuse, direct execution and late arrivals; corrects per-attempt timeout semantics without changing layout defaults. |
| 13 | approved | Adopts one complete table_version with exact signed-64-bit/JSON-string/TypeScript-bigint representation; atomically publishes its Iceberg property and retained native metadata reference under expected-version/base-pointer CAS. Specifies stale-new-write refusal, original-version retries, rooted retention, unmappable-legacy refusal and multi-row tenant-group fencing without added ingest SQL. Defaults/auth/telemetry remain unchanged. Maintainer-amended: capacity and tracing benchmarks may run when no other worktree is running, and fail on a regression beyond 5%. |
| 14 | approved | Maintainer-approved Service-only configure/subject model; six shared descriptive columns on eight attributable tables; shared four-key sorting and five attribution Blooms plus signal lookup keys; Task009 follows completed Task002, with Task004 engine reused. |
| 15 | approved | Adds state.http_auth/httpAuth stock-provider SDK contract, automatic Service/subject/Run/environment/trace injection, two credential modes, validation/capture closure and approved Python/TypeScript examples; physical defaults unchanged. |
| 16 | approved | Maintainer-directed two telemetry setup paths: Wyrd-managed frozen state context, or application-managed standard OpenTelemetry resources and OTLP export. Removes the requirement for state-bound external exporters; preserves validated UID attribution, signal schemas, partitions, sorts, Blooms and provider ownership. |
