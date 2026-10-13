---
id: SPEC-wyrd-ui-live-data
revision: 5
status: approved
---

# Wyrd UI live data integration and validation

## Objective and user value

Connect the existing Wyrd UI to real Wyrd server capabilities and prove the
complete human workflow: sign in, discover registered components, navigate
their relationships, investigate service executions, and inspect verification
evidence. Users must be able to trust that displayed records, charts, counts,
filters, and judgments describe their authorized tenant's actual data.

This is live-data integration, not a frontend rebuild. Preserve the existing
rendering, controls, shared components, URL state, and contextual navigation.
Repair or complete them only where real-data journeys expose an incorrect or
missing required behavior. Validate one page at a time.

## Scope and current baseline

In scope:

- Tenant login through the existing OIDC/OAuth BFF and CLI authentication for
  the same person.
- Home, Card inventory, and Model, Data, Agent, Prompt, and Service details.
- Service operational views and their contextual links into Observe.
- Observe overview, traces and span details, logs, metrics, evaluations and
  task details, and drift reports and feature history.
- A reusable disposable local environment and real browser journeys backed by
  public SDK registration, actual service executions, and retained evidence.
- One canonical server-to-UI TypeScript contract generation and drift-check
  path, rooted in the actual mounted server operations.
- SvelteKit server loading, rendering, navigation preloading and invalidation
  as hard requirements for the live BFF.
- Targeted usability corrections required to complete these journeys.
- A full-scale simulation of Wyrd and its UI: twenty Services with their
  composed Cards, observations, evaluations, drift and records, all visible
  and correct in the UI (REQ-026, AC-019).

The current mock UI already implements meaningful filters, search, pagination
on several surfaces, detail views, and scoped navigation. For example,
Model and Service links carry service context into Observe; Observe tabs
preserve it; mock logs and traces consume it. Card relationship links carry
UIDs, version links select versions, and verification links carry selectors.
The production authentication path exists independently of the mock content
facade, whose live domain operations remain unconnected.

Known mock limitations are integration findings, not permission to discard
the frontend: metric series do not change with every passed scope, some
verification counts are illustrative, and drift ignores some selectors.
Card inventory pagination also needs completion. Each affected behavior must
be made truthful against real data.

This specification defines a new change. The old
`changes/active/wyrd-ui-foundation/tasks` packet is historical context, not a
task authority for this work. It is not edited or retired here. Existing
rendering requirements remain useful where consistent with current protocol
authority; obsolete vocabulary or mock contracts do not redefine Wyrd.

### Non-goals

- Replacing the visual design or introducing a new global service selector.
- Implementing Changes, custom dashboards, the raw Query workbench, additional
  Card workspaces, or UI authoring/deployment flows beyond the scope above.
- Implementing a new identity system, shell credential export, or automatic
  modification of the user's shell environment.
- New analytical storage, verification engines, run registries, observation
  read endpoints, or UI-owned durable domain behavior.
- Paid model calls, customer infrastructure, production identities, or live
  external notification delivery as requirements of local journeys.
- Defining new public/persisted contracts, migrations, or ownership exceptions
  through this specification. A proven need for one requires a revision.

Generating TypeScript projections of existing wire contracts and repairing
schema metadata to describe existing serialization accurately are in scope;
they do not authorize changes to the underlying wire behavior.

## Definitions and navigation semantics

- **Live mode:** production authentication and server-backed content through
  the BFF, with local mock identity and mock content disabled. No fallback to
  fixtures occurs when an upstream operation fails.
- **Contextual scope:** the context already carried by a link or URL, such as
  service, time range, Card UID/version, Verifier selector, trace, or selected
  record. These presentation parameters are not uniformly serialized
  `CardRef` values and are never authorization authority.
- **Service composition:** exact independently versioned Cards and declared
  relationships resolved by Wyrd. Data and Verifiers may be related through
  declared references/bindings; they do not become direct Service components
  when the protocol disallows that kind.
- **Page gate:** evidence that one page and its outgoing investigative links
  satisfy the applicable requirements with live data in a real browser.

Contextual scope is preserved along links that represent continuation of the
same investigation. Relationship navigation selects the linked Card's own
identity. A scoped investigation is not a persistent application-wide service
selection: unrelated primary navigation need not acquire a new global filter.
The user can see, change, and clear the applicable scope.

Service resource names filter telemetry resource names; exact Card selectors
filter authorized subject identities; Verifier selectors filter verification
identity. They must not silently substitute for each other. Name-based URLs
must not claim exact-version scope unless their resolution actually supplies
it. Existing service/version context must not silently float to a newer Card.
If a record lacks optional Card correlation, the UI must not invent it.

## Required observable behavior

### Authentication and BFF

- **REQ-001:** A newly provisioned Keycloak user with no pre-created Wyrd human
  principal can sign in through the tenant UI. Wyrd creates/resolves the user
  by verified issuer and subject within that tenant, applies configured role
  mapping, and returns an authenticated tenant page. Repeat sign-in resolves
  the same principal without duplicate registration.
- **REQ-002:** That person can complete CLI device authorization against the
  same tenant and perform an authorized CLI read using saved credentials.
  UI and CLI resolve the same tenant principal. The journey isolates saved
  credentials and removes credential overrides that could hide failed login;
  it does not require tokens in the parent shell environment.
- **REQ-003:** Protected UI reads use the current tenant session through the
  BFF and existing public server contracts. Authentication, renewal, logout,
  denied access, and upstream failure have truthful user-visible outcomes.
  A revoked login cannot renew; already-issued access tokens retain the
  protocol's existing expiry semantics. Invalid authentication state never
  grants a session or releases protected content.
- **REQ-027:** The official image signs in with no UI- or origin-specific
  deployment settings. Run with only its database and storage settings and
  reached at any address — `http://localhost:8080` in development, or an
  `https://` host behind a TLS-terminating proxy — a person signs in through
  the UI with an API key, and through SSO once the tenant's connection is
  active. Neither the BFF nor the server requires a client secret, a public
  origin, or a UI `ORIGIN`: the deployment's public origin is the origin each
  request arrived at (the host and scheme the image's proxy forwards, else its
  `Host`). Nothing pins it.

### Home and Cards

- **REQ-004:** Home shows authorized live Card and operational summaries and
  working links into the included pages. Displayed counts and activity come
  from server evidence, not mock constants or inferred totals from one page.
  Out-of-scope sections must be omitted or explicitly unavailable in live
  mode, never populated with invented operational data.
- **REQ-005:** Inventory discovers multiple registered Model and Data Cards,
  Agents, Prompts, and Services with correct kind, identity, metadata, and
  lifecycle state. Search and displayed filters select actual matching data;
  ordering and pagination are deterministic on a settled dataset. Pagination
  must not require fetching the entire registry into the browser. Count labels
  distinguish page size from an authoritative total.
- **REQ-006:** Model, Data, Agent, Prompt, and Service detail pages render the
  exact selected Card, including its metadata, kind-specific declaration,
  server-derived relationships, and relevant status. Direct links and version
  selection work without prior inventory navigation. Missing versions,
  missing Cards, and denied access never fall back to a fixture or another
  version. Relationships navigate to the correct linked Card.
- **REQ-007:** Service composition and operational views reflect the selected
  Service's real declaration and authorized evidence. Component links and
  signal links preserve their intended identity and investigation context.
  Reused components retain independent Card identities; service association
  is not inferred from labels or display-name substring matches.

### Context, queries, and telemetry

- **REQ-008:** Preserve existing contextual navigation and URL-backed state
  while connecting the live data operations. Search, filter changes, relevant
  cross-page links, refresh, direct URLs, and browser back/forward restore the
  intended state. Every exposed selector is consumed by its destination or
  explicitly identified as inapplicable; it cannot merely change the URL.
  Changing a filter resets incompatible pagination or selected detail state.
- **REQ-009:** Observe overview and telemetry pages read retained observations
  through canonical Bifrost SQL and the established client boundary. Queries
  are parameterized, read-only, time-bounded where applicable, explicitly
  projected, and bounded in result size and execution lifetime. Filtering and
  aggregation apply to the selected population before pagination, rather
  than filtering an arbitrary fetched page in browser code.
- **REQ-010:** Multiple actual Service executions produce discoverable traces
  and spans. Search, service/time/status filters, incremental loading, trace
  detail, parent/child spans, events, links, and correlated navigation display
  the emitted evidence and preserve supported context. Successful and failed
  executions remain distinguishable.
- **REQ-011:** Logs show actual exported records, severity and resource
  metadata, search/filter results, pagination, selected record detail, and
  trace correlation when present. Page changes on settled data neither repeat
  nor omit matching records; long messages remain readable.
- **REQ-012:** Metrics show actual series and supported aggregations with
  correct units, timestamps, windows, and service/resource separation.
  Selector changes affect the displayed series. Counters, gauges, and
  distributions retain their actual metric meaning; absent points are not
  invented zeros and unlike units are not merged.

### Verification

- **REQ-013:** Evaluation and task views display retained results from actual
  Eval and Task Verifier executions, including passing and failing checks.
  Inventory counts, charts, task detail, status/verdict, and supported
  search/filter/pagination agree with the selected subject, Verifier, and
  window. Results retain exact identities and evidence links. Execution state
  is not conflated with verdict; missing or skipped checks are not passes.
- **REQ-014:** Drift views display actual fitted baseline state, comparison
  results, feature statistics/history, applicable thresholds, and sample
  information. Subject, Verifier, feature, and window selectors affect the
  result. Healthy, shifted, missing-baseline, insufficient-data, and execution
  failure cases remain distinct. Scheduling belongs to Trigger bindings;
  no schedule or reaction logic moves into the UI.
- **REQ-015:** Real-time direct verification and continuous/scheduled binding
  executions remain distinguishable. Direct analysis does not invent a
  durable queued run or failure dispatch. Retained results may become visible
  after execution completion; the UI does not fabricate history while they
  are unavailable.

### Environment, reliability, and usability

- **REQ-016:** One reusable disposable local setup supports independent page
  journeys and a complete combined journey. It provisions real Keycloak,
  repository-managed Postgres, Wyrd and the production-built BFF, and uses
  public client/server paths to register Cards and generate evidence. A
  deterministic local provider is allowed; mocked Wyrd responses, SQL-seeded
  domain results, or in-process engine substitutes are not live journey proof.
- **REQ-017:** Data generation finishes its normal export/flush/shutdown
  lifecycle. Readiness is established by bounded observation of expected
  server state, with useful diagnostics on timeout, rather than fixed sleeps.
  Isolated publication settling may support exact assertions in the test
  environment; it must not create a production freshness guarantee or expose
  a test-only control to the UI.
- **REQ-018:** Included pages handle loading, genuine empty data, invalid
  filters, access denial, upstream failure, and analytical degradation
  truthfully. A failed query is not an empty successful result; partial rows
  before a failure terminal are not rendered as a complete result. Known
  degraded results are visibly qualified. No stale response replaces a newer
  user selection.
- **REQ-019:** Real browser journeys operate the rendered UI, including SSO
  forms, navigation, search, filters, pagination, drawers/details, and charts
  where interactive. Controls have accessible names, keyboard operation and
  visible focus; relevant dialogs handle focus correctly. Selected context,
  units, status and recovery actions are understandable. Human review checks
  discoverability and interpretation rather than inferring usability from
  HTTP HTML assertions alone.

- **REQ-026:** The shared disposable setup can run at full simulation scale:
  twenty Services, each with its composed Cards, repeated actual executions
  producing traces, logs and metrics, real-time and continuous Eval results
  (passing and failing), and Drift bindings with fitted baselines and
  healthy and shifted observations. All of it is registered, emitted and
  evaluated through the same public pathways as the page gates; nothing is
  inserted directly. At that scale every included page shows every Service's
  authorized evidence: pagination covers the whole matching population
  without overlap, filters and counts agree with authoritative reads, and no
  page times out, truncates silently or falls back to fixtures. Per-page work
  stays bounded independently of the tenant's Service count, except where the
  displayed rows are themselves per-Service and limited by page size. Page
  server-render and navigation timings are recorded as evidence; they are not
  a pass/fail threshold in this revision. Page gates keep their small
  deterministic datasets; the simulation is one additional journey.

### Generated server-to-UI contracts

- **REQ-020:** The one canonical generation path is Rust wire types and mounted
  route registrations → the composed server's served `/openapi.json` →
  generated TypeScript HTTP contract types → BFF consumers and their typed UI
  projections. Cover each HTTP operation used by the in-scope pages, including
  method/path, path/query parameters, request bodies, response status/media
  types, JSON bodies and documented errors. Preserve required/optional fields,
  nullability, enum values and discriminated unions. Incomplete or conflicting
  schema descriptions must be repaired at their owning source to match actual
  serialization, not patched in generated output or a handwritten UI schema.
- **REQ-021:** BFF adapters consume these generated wire types; UI-owned
  presentation models may remain distinct and must have type-checked mappings
  from the wire contracts. Mocks of wire responses also satisfy the generated
  contracts. Do not maintain a parallel handwritten inventory of HTTP DTOs,
  routes or fields. Legitimately open JSON remains open; missing schema
  coverage for a closed contract must not be hidden by `any`, blanket
  `unknown`, or unchecked casts. SvelteKit page data continues to use its
  existing inferred/generated route types rather than a duplicate server API.
- **REQ-022:** Provide one deterministic repository-managed regeneration path
  and one corresponding check path under the existing `codegen:regen` and
  `codegen:check` ownership. Generate from a fresh locally served document for
  the same composed server surface, not a separately maintained OpenAPI file
  or a developer-selected remote deployment. The OpenAPI document remains a
  runtime contract; any intermediate export is ephemeral. Generated
  TypeScript is a reviewed artifact and cannot be hand-edited. Checks detect
  stale, missing, added and changed generated artifacts; regeneration followed
  by BFF/UI type-checking catches incompatible uses before a page gate passes.
  CI for affected server contracts and UI consumers includes this proof.

The TypeScript generator is a reversible tool choice, not a second contract
owner or a generated replacement transport. Existing generated Card types and
SDK declarations must be reused where they already project the same canonical
contract; overlapping declarations must agree rather than define independent
meanings. All UI contract generation is reached through the one workflow.

OpenAPI generation covers HTTP contracts, not arbitrary SQL result schemas or
the decoding of protobuf/Arrow frames. Bifrost keeps its shared client and
canonical stream contracts. Analytical projections must check the returned
schema against their expected columns/types before interpreting rows and use
existing canonical/generated types where available. A binary response must
not be declared as a JSON DTO to make generation appear complete.

Generated TypeScript is compile-time evidence, not runtime validation. Served
contract tests and live journeys must prove actual serialization and query
schema mapping. A stale-contract check detects source/output drift; it does
not declare every intentional API revision backward compatible. Changed
contracts require review, and behavior-only breaks remain journey obligations.

### SvelteKit server loading and navigation

- **REQ-023:** Initial authenticated page data loads through SvelteKit
  `+page.server.ts` and, for genuinely shared data, `+layout.server.ts`.
  Server-rendered pages include the primary authorized content or its truthful
  error/empty state before hydration; components must not wait for `onMount`
  or an effect to fetch that content. Reuse server-only BFF capabilities
  directly from loads without an unnecessary HTTP round trip to a new internal
  endpoint. Use SvelteKit-generated route types for page/layout data.
- **REQ-024:** URL-backed search, filters and pagination drive server loads
  during initial requests and client navigation. Use SvelteKit navigation and
  supported link-data preloading where it improves an included journey;
  speculative heavy analytical work must remain bounded. Reuse loaded data
  through hydration instead of fetching it again in components. Track load
  dependencies and use targeted invalidation when data or session state
  changes; avoid accidental serial waterfalls between independent reads.
- **REQ-025:** Mutations in scope use SvelteKit server form actions with
  progressive enhancement where applicable. Browser-driven incremental reads
  or refreshes that cannot use normal navigation remain behind authenticated
  SvelteKit server handlers. Server loads and preloads are read-only. Browser
  components do not call protected Wyrd backend routes directly. Credentials,
  upstream clients and authorization-sensitive work stay server-only.

These are hard acceptance requirements, not optional optimizations. Use the
repository-pinned SvelteKit version and established APIs; this change does not
require a framework upgrade or newer experimental fetching APIs. Server
rendering and navigation preloading do not replace Wyrd server authorization.

## Invariants and required boundaries

- **INV-001 — Authority:** Current Wyrd protocol, Bifrost, security and agent
  rules win over foundation mocks and old tasks. Card kinds, envelopes,
  `CardRef`, relationships, observations, and Verifiers keep their native
  contracts. Wyrd remains headless; no durable workflow becomes UI-only.
- **INV-002 — Ownership:** Rust server owners retain durable behavior. The
  BFF owns session handling, upstream orchestration and presentation mapping;
  it has no direct database/object-store access or replacement registry,
  lineage, verification, or authorization engine. Language SDK behavior and
  Bifrost transport reuse `wyrd-client`; canonical SQL remains the analytical
  read contract. This change does not authorize per-signal read APIs.
- **INV-003 — Tenancy:** Verified credentials supply effective tenant and
  permissions. URL context never widens access. Sessions, caches and page
  data cannot leak across tenants or principals. Tests include another tenant
  with overlapping names and direct URL manipulation. BFF credentials must
  not elevate the signed-in person's data authority.
- **INV-004 — Secrets and permissions:** OAuth credentials remain in the
  established server/session boundary, absent from page data, browser storage,
  URLs, logs and checked-in test artifacts. The BFF is the public OAuth client
  `wyrd-ui` and holds no secret of its own: each tenant session cookie
  (HttpOnly, SameSite=Lax, Secure whenever the request is `https`) carries
  only the credential the server validates on every renewal — the Wyrd
  refresh token, or the API key the person entered — and the server, not the
  cookie, is the authority that ends a session. Local HTTPS/trust setup preserves
  production cookie behavior. Server RBAC, query object scope, token audience,
  CSRF controls and any additional payload permissions remain enforced.
  Actual Bifrost column permissions govern; mock permission labels do not
  create a second access model.
- **INV-005 — Audit:** Permission decisions complete before protected work
  and stage audit through the shared non-blocking Scribe outbox. UI requests
  neither bypass authorization nor wait for audit persistence. No second
  audit history or writer is introduced.
- **INV-006 — Evidence:** Wyrd registration, ingestion and verification create
  the evidence shown. Synthetic local workload inputs may be deterministic;
  retained results are never inserted or forged to satisfy a screenshot.
  Optional correlation stays optional and results retain exact attribution.
- **INV-007 — Visibility:** Append acknowledgement and verification completion
  do not imply immediate complete query visibility. Preserve success,
  degraded and failure terminals, existing publication-overlap semantics,
  retention limitations, and outbox loss semantics. No exactly-once display
  or new freshness promise is introduced.
- **INV-008 — Bounded work:** Page loads and BFF queries do not perform
  unbounded scans/collection, fan-out or automatic retries. Supported server
  resource limits remain authoritative; cancellation and failures release
  resources. A new response-time SLO or benchmark suite is not implied;
  REQ-026 records full-scale timings as evidence, not as a gating threshold.
- **INV-009 — Compatibility:** Preserve the existing contextual URL contract
  where it expresses valid Wyrd behavior. Necessary private data projections
  can change; unsupported mock assumptions must be corrected without public
  compatibility aliases, invented counts, or floating exact references.
- **INV-010 — Contract source:** Mounted server operations and their canonical
  wire types are the only HTTP contract authority for UI generation. The
  generation path must not use an independent route list, fixture-derived
  schema or handwritten DTO copy. Generated artifacts, BFF mappings and UI
  consumers stay checked together; casts or disabled checks cannot conceal a
  breaking change.
- **INV-011 — SvelteKit BFF:** Initial page data and protected incremental
  access retain the server boundary. Every request, including preloads and
  client navigations, has current authenticated tenant context; a reused
  layout is not sufficient authorization proof. Request-specific user/page
  state is not stored in unkeyed shared server variables. Speculative loading
  never performs mutations or bypasses access checks.

## User journeys and page acceptance gates

The gates are behavioral acceptance, not implementation tasks. Each page is
independently runnable with shared setup and its required scenario. Complete
and record one page's gate before advancing to the next; combined closeout
then proves the seams. Revisiting an accepted page is required when a later
integration changes its behavior.

| Gate / acceptance ID | Journey and required outcome | Requirements |
|---|---|---|
| **AC-001 — Login and CLI** | Provision tenant trust and a fresh Keycloak user; use UI SSO; confirm one Wyrd principal; sign in again; complete CLI device approval as the same person; perform an authenticated read. Exercise logout, failed login and nonrenewable session behavior. | REQ-001–003, REQ-016, REQ-019 |
| **AC-002 — Initial Home** | Sign in to an empty tenant, then register real Cards and return home. Summaries and links reflect the change; unavailable out-of-scope sections contain no fixtures. Refresh and deep navigation retain valid tenant context. | REQ-004, REQ-018–019 |
| **AC-003 — Card inventory** | Register multiple Models and Data Cards, then registered Prompts, Agents and at least two Services through an SDK. Use browser search, kind/metadata filters and pagination with more records than one page; check inclusion, exclusion, ordering, totals and empty state against authoritative reads. | REQ-005, REQ-008, REQ-016, REQ-019 |
| **AC-004 — Model detail** | Open from inventory and directly; inspect declaration/metadata, follow related Cards, select another registered version, and exercise missing/denied identity. | REQ-006, REQ-008, REQ-019 |
| **AC-005 — Data detail** | Independently repeat detail/relationship/version acceptance for a real Data Card and its declared metadata/schema. | REQ-006, REQ-008, REQ-019 |
| **AC-006 — Agent detail** | Independently verify real Agent metadata, declaration, Prompt relationships and applicable contextual links. | REQ-006, REQ-008, REQ-019 |
| **AC-007 — Prompt detail** | Independently verify real Prompt metadata/declaration and relationships without inventing runtime outcomes or exposing credentials. | REQ-006, REQ-008, REQ-019 |
| **AC-008 — Service workspace** | Inspect either Service and its exact composition/version; follow component and scoped Observe links; switch service/time context and clear it. Shared components retain their identity and unrelated Service evidence is excluded from the selected investigation. | REQ-007–008, REQ-019 |
| **AC-009 — Observe overview** | Execute both Services repeatedly with distinct evidence; finish emission lifecycle and wait for readiness. Summary links and attention agree with retained evidence and lead to the intended scoped signal page. | REQ-008–009, REQ-016–019 |
| **AC-010 — Traces, then trace detail** | Gate the list first with search, service/time/status filters and load-more; gate trace/span detail separately with parentage, events, links and correlated navigation. Known runs are present and unrelated runs excluded. | REQ-008–010, REQ-017–019 |
| **AC-011 — Logs** | Generate correlated and uncorrelated logs for both Services; operate facets/search/pages/page-size and detail in the browser. Settled pages cover the matching population without overlap; supported trace links preserve context. | REQ-008–009, REQ-011, REQ-017–019 |
| **AC-012 — Metrics** | Generate known counters, gauges and distributions for both Services, with records inside/outside chosen windows. Select service, time and measure; compare displayed series, aggregation, units and gaps to retained points. | REQ-008–009, REQ-012, REQ-017–019 |
| **AC-013 — Evaluations, then task detail** | Register real-time Task/Eval and continuous Eval verification bindings; execute passing and failing cases across both Services. Gate inventory/filter/chart/pagination first, then result/task drilldowns and evidence links independently. Preserve inconclusive/error/skipped semantics. | REQ-008–009, REQ-013, REQ-015–019 |
| **AC-014 — Drift** | Register supported Drift bindings and fit baselines; emit healthy then shifted observations and let the real runtime evaluate. Inspect feature reports/history and scope changes; exercise missing baseline, insufficient data and failed execution without showing a healthy result. | REQ-008–009, REQ-014–019 |
| **AC-015 — Complete Home** | Return after telemetry and verification gates; operational summaries/counts and links agree with the underlying pages under equivalent scope. No operational section still uses mock data. | REQ-004, REQ-008–009, REQ-013–019 |
| **AC-016 — Combined journey and isolation** | In one user story, login → register → inspect Cards → execute → investigate telemetry → inspect verification → return home. Repeat targeted access checks with a restricted user and another tenant containing overlapping names; attempted scope/URL manipulation leaks no protected rows or details. | REQ-001–019, INV-001–009 |
| **AC-017 — Generated contract gate** | Before accepting live page integrations, regenerate from the locally served composed contract and type-check the consumers. Repeating regeneration yields identical artifacts. Demonstrate that stale/missing output fails the check, and that a representative consumed field/parameter or operation change leaves an unchanged adapter failing type-check after regeneration. Actual JSON/error serialization agrees with the served schema; analytical schema mismatches fail truthfully. Every page's acceptance record identifies its generated wire contract and typed projection. | REQ-020–022, INV-010 |
| **AC-018 — SvelteKit loading gate** | For each included page, verify primary content in the initial server-rendered response before hydration, then operate client navigation, supported preloading, URL filters and refresh/invalidation. Network evidence shows protected upstream access remains in the BFF and hydration does not redundantly fetch the same initial dataset. Navigation after session expiry and tenant changes revalidates authority; preloads perform no writes or cross-session reuse. | REQ-023–025, INV-011 |
| **AC-020 — Zero-configuration image login** | Build the official image and run it with only its database and storage settings: no `WYRD_PUBLIC_ORIGIN`, client secret or UI `ORIGIN`. Reach it over plain `http` at a non-`localhost` host and through an `https`-forwarding proxy; sign in with an API key and load an authenticated page each way. On the server, an authorization request is redirected back only to `{request origin}/login/callback`, a redirect URI on another origin is refused, and `wyrd-ui` redeems, refreshes and revokes with `client_id` alone. The SSO browser journey passes with no secret or origin setting. | REQ-001, REQ-003, REQ-016, REQ-027, INV-003–004 |
| **AC-019 — Full-scale simulation** | After the page gates and the combined journey, run the shared setup at twenty Services through public pathways and settle it with bounded readiness. In a real browser, every included page (Home, Cards inventory and details, Service workspaces, Observe overview, traces and trace detail, logs, metrics, evaluations and task detail, drift) shows each Service's evidence; paging covers each authoritative population without overlap; filters, counts and Home summaries agree with authoritative reads under equivalent scope; another tenant and a restricted user still see nothing protected. Record per-page server-render and navigation timings and the dataset sizes they were measured on. | REQ-004–019, REQ-023–024, REQ-026, INV-006–008 |

## Acceptance evidence and verification

Each page record maps requirement/invariant IDs to browser actions, expected
data identities/counts, authoritative SDK/HTTP/Bifrost reads, and pass/fail
outcomes. Assertions must prove exclusions as well as presence. Time-dependent
exact count/page assertions use a bounded settled dataset and matching query
scope; ongoing ingestion is not an implicit snapshot consistency guarantee.

Required evidence classes:

1. Real browser journeys against the production-built BFF and real server,
   exercising client hydration and controls, not only prepared HTTP URLs.
2. Public SDK → server → retained-read evidence for registration, executions,
   OTEL export and verification; preserve language/runtime ownership of tests.
3. Generated-contract drift, BFF/UI compilation, served serialization,
   analytical schema, error and security checks, plus existing
   mock/component tests where they protect behavior being reused.
4. Page-specific human usability review with screenshots and concrete issues
   or a recorded pass. Captures/traces must be scrubbed of authentication
   material and access-controlled payloads; protected artifacts stay local.
5. Scoped repository format, lint, build, type, journey and boundary checks
   for actual changed owners, including contract generation for UI-consumed
   HTTP surfaces. Wire behavior changes, if separately approved,
   require generated-contract and served-OpenAPI proof as applicable.

Use repository-managed `mise` lanes. The existing identity journey supports
`WYRD_IDENTITY_TARGET=ui` and `WYRD_IDENTITY_TARGET=cli` with
`mise run test:identity:journey`; these HTTP/CLI checks support but do not
replace browser acceptance. Planning must inspect current commands and add
the narrowest browser/page proof to existing verification ownership. Exact
test selectors belong to the task packet, not this behavioral specification.
No broad gate is required merely because this draft exists.

## Expensive-to-reverse decisions

1. The UI remains a BFF projection of existing Wyrd contracts; it gains no
   durable domain authority or direct storage access.
2. Preserve contextual investigation scope instead of adding a global service
   selection concept. Server identities and existing signal semantics decide
   what that scope selects; display parameters never grant access.
3. Use disposable local infrastructure and deterministic service inputs, with
   real public registration/ingestion/verification and production auth/BFF.
4. CLI authentication follows the existing saved-credential contract. Shell
   export is outside this change.
5. Page-by-page live browser gates and the combined user journey are mandatory
   completion evidence. Mock coverage alone cannot close a gate.
6. No new public or persistent protocol is approved here. If existing owners
   cannot fulfill a required workflow, identify the precise missing capability
   and revise the specification before the affected implementation proceeds.
7. The composed runtime OpenAPI contract is the source for generated HTTP
   types consumed by the BFF/UI. Extend existing code-generation ownership;
   do not introduce a parallel API specification, route catalog or transport.
8. SvelteKit server loads and SSR supply initial page data; framework
   navigation/preloading and targeted invalidation maintain it. Server actions
   and handlers cover applicable interactions. Client-only initial fetching
   is not an alternative implementation of these requirements.

9. Login derives the deployment's public origin from each request: the host
   from `X-Forwarded-Host`, else `Host`, and the scheme from
   `X-Forwarded-Proto` (`http` when absent). The image's nginx forwards an
   incoming pair, else its own, and the BFF reads them through adapter-node's
   `HOST_HEADER`/`PROTOCOL_HEADER` and forwards them on its `/v1` calls. Wyrd's own redirect to `wyrd-ui` must stay on the origin the
   authorization request arrived at, and the identity provider's registered
   redirect URIs remain the trust anchor for its callback, so a forged `Host`
   only redirects its own sender. There is no origin setting. `wyrd-ui` becomes a public client; its refresh token still
   does not rotate, because concurrent page requests renew one session, and
   it keeps its bounded absolute lifetime. This supersedes the confidential
   `wyrd-ui` client, its `WYRD_UI_CLIENT_SECRET`/`WYRD_UI_CLIENT_SECRET_SHA256`
   settings and RFC 8414 discovery by the BFF. No compatibility path keeps the
   secret; a Basic-authenticated `wyrd-ui` request is refused.

## Open material decisions

None proposed for revision 3. Approval covers the behavioral outcomes and
boundaries above, not a speculative new API. Planning must validate complete
existing workflows, especially analytical client/session composition, exact
Card lookup/version resolution, inventory summaries and verification reads.
These are source-validation obligations; a discovery requiring a material
contract, security or ownership change reopens this draft through a revision.

## Authority and implementation grounding

Normative authority:

- [Repository standards](../../../AGENTS.md) and
  [agent rules](../../../architecture/agent-rules.md).
- [Wyrd protocol](../../../architecture/wyrd-design.md),
  [doctrine](../../../architecture/wyrd-doctrine.mdx),
  [Bifrost](../../../architecture/bifrost-design.md), and
  [security posture](../../../architecture/wyrd-security-posture.md).
- [Reference router](../../../architecture/references/README.md),
  [spec-driven development](../../../architecture/references/languages/spec-driven-development.md),
  [testing](../../../architecture/references/languages/testing-workflows.md),
  [architecture patterns](../../../architecture/references/architecture/patterns.md),
  [telemetry](../../../architecture/references/domain/telemetry-observations.md),
  [evaluation](../../../architecture/references/domain/evaluation.md), and
  [drift](../../../architecture/references/domain/drift-monitoring.md).
- [Wyrd SvelteKit architecture](../../../.agents/skills/wyrd-ui/references/wyrd-sveltekit-architecture.md).

Framework grounding, applied to the repository-pinned version:
[SvelteKit loading data](https://svelte.dev/docs/kit/load),
[link preloading](https://svelte.dev/docs/kit/link-options), and
[form actions](https://svelte.dev/docs/kit/form-actions).

Current implementation grounding, not independent contract authority:

- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/wyrd.ts`: mock/live seam.
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts`:
  production session and upstream authentication.
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/mock/cards/` and
  `src/lib/server/mock/observe/project.ts`: current presentation behavior.
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/features/observe/ObserveNav.svelte`:
  scoped signal navigation.
- `crates/wyrd/wyrd-server/tests/integration/identity_ui_e2e.rs` and
  `crates/wyrd/wyrd-cli/tests/cli_login_journey.rs`: existing real identity paths.
- `examples/support-desk/python/support_desk.py` and
  `sdks/wyrd-sdk-python/tests/integration/test_support_desk.py`: actual service
  invocation and retained-result readiness.
- `sdks/wyrd-sdk-python/tests/integration/otel/test_otel_signals.py` and
  `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py`: signal
  export and deterministic verification.
- `mise.toml`: canonical environment and verification ownership.
- `crates/wyrd/wyrd-server/src/http/router.rs`: one route/OpenAPI composition
  serving `/openapi.json`.
- `crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs`: assembled
  served-contract and runtime refusal proof.
- `scripts/gen_card_types.py`: existing canonical-schema TypeScript/Python
  Card generation; this does not yet generate the complete HTTP operation
  contracts consumed by the UI.

Tool feasibility, not a mandated dependency:
[OpenAPI TypeScript](https://openapi-ts.dev/introduction) supports generating
TypeScript declarations from OpenAPI 3.0/3.1, including discriminators. Choose
the smallest sufficient generator during planning; no dependency is installed
by this specification edit.

## Revision history

| Revision | Date | Status | Change |
|---|---|---|---|
| 1 | 2026-10-12 | draft | New live-data integration spec incorporating the agreed reuse of frontend behavior and contextual navigation, disposable local journeys, and page-by-page acceptance. Awaiting explicit human approval. |
| 2 | 2026-10-12 | draft | Add the requested canonical server-to-UI TypeScript generation path, source/output drift and consumer compilation gates, and runtime/analytical schema limits. Awaiting explicit human approval. |
| 3 | 2026-10-12 | approved | Record the user's OpenAPI decision and add hard SvelteKit server-load, SSR, navigation/preloading and invalidation requirements with page-level acceptance evidence. Explicitly approved by the user on 2026-10-12 for task decomposition. |
| 4 | 2026-10-12 | approved | Add the user's original full-scale load requirement, previously missing: REQ-026 and AC-019 (twenty Services with observations, evaluations, drift and records, all visible and correct in the UI). Timings are recorded evidence, not a gating SLO, so INV-008 is clarified rather than replaced. Additive: revision-3 requirements and accepted tasks 001–008 stand unchanged. Approved by the user on 2026-10-12 ("agree"), choosing correctness at scale plus recorded timings. |
| 5 | 2026-10-12 | approved | Zero-configuration image login: REQ-027, AC-020 and decision 9, with INV-004 restated for a secretless BFF. The confidential `wyrd-ui` client, its secret settings, BFF discovery and the required public origin are removed: the image could not sign in as shipped. Revision 4 requirements and tasks stand; TASK-020 implements this. Approved by the user on 2026-10-12 ("write it up in a spec revision and make the changes"). |
