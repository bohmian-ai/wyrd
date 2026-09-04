---
id: SPEC-wyrd-ui-foundation
revision: 6
status: approved
---

# Wyrd UI foundation and Change Request workspace

## Human intent and user value

Build the first real Wyrd product UI as a SvelteKit browser-for-frontend (BFF)
served from the Wyrd deployment. It must give people a clear operational home
for Cards, observation workflows, and Change Requests while Wyrd's integrating
server branches are still unstable.

The first implementation uses realistic mocked Wyrd responses behind real UI
and BFF routes. It must establish the routes, view contracts, authentication
boundary, reusable components, Docker/Nginx serving shape, and viewable SVG
mockups without coupling the browser to unfinished Wyrd services.

Change Requests are the next coordination layer above pull requests. This UI
projects their intent, exact proposed revisions, Claims, standardized Evidence,
Verifier judgments, review, and decision state without defining or owning the
durable protocol. Pull requests, commits, and CI runs remain linked inputs and
projections rather than UI-owned truth.

### User Persona

1. **Data Scientist / AI Engineer (primary)**: Builds and evaluates models,
   prompts, agents, experiments, and AI services. They need fast movement from a
   registered definition to runs, outputs, metrics, traces, evaluation, drift,
   and the exact evidence behind a result without translating infrastructure
   vocabulary first.
2. **Software Engineer**: Implements and operates services and the Wyrd UI.
   They need current state, precise failure context, stable links, and enough
   technical detail to verify behavior against the specification.
3. **Product Manager**: Coordinates service changes and their intended impact.
   They need plain-language intent, ownership, verification state, and a clear
   path into supporting evidence without losing the shared technical record.

## Scope

- An authenticated operational homepage.
- Card discovery and one shared Card-detail shell with earned kind-specific
  content.
- A Service-local operational workspace that leads with current behavior and
  attention through an intuitive mini-dashboard, links into canonical Observe
  investigations, and keeps the deterministic composition of independently
  versioned Cards available as a subordinate view without embedding or copying
  their durable identities.
- Observation workspaces for logs, metrics, traces, dashboards, evaluations,
  and drift.
- A raw, read-only Bifrost SQL query workbench with query execution state and tabular
  results.
- Change Request list, creation, detail, verification, review, and timeline
  views.
- Provider- and verifier-neutral Change Request views represented by typed mock
  BFF view models; durable protocol contracts remain outside this change.
- Change Requests containing one or more linked repository/commit/PR subjects,
  including multiple subjects from one repository or across repositories.
- A local development authentication/session implementation with production-
  shaped boundaries.
- A small reusable component foundation for repeated Wyrd controls, status,
  data display, containers, and charts. First-party workspaces consume this
  foundation while retaining distinct information hierarchy and workflows.
- A machine-readable catalog of the safe, semantic component contracts that
  are suitable for future user- or agent-composed views. The catalog is an
  implementation boundary and compatibility aid, not a view-authoring product.
- Viewable SVG mockups for the agreed product routes before full view
  implementation.
- A SvelteKit adapter-node application packaged for Docker and served through
  Nginx using the repository's relevant deployment patterns.

## Non-goals

- Connecting to unfinished Wyrd domain services in this change.
- Browser access to Wyrd, provider, GitHub, or verifier credentials.
- A global notification Inbox or top-level Connections work area.
- GitHub merge or deployment authority.
- An Agent playground without a real headless execution contract.
- A verifier marketplace, verifier optimizer, portfolio router, or autonomous
  verifier-policy rewriting.
- A reusable Assurance Card before repeated cross-Change Claim bundles prove
  that separate concept is needed.
- Separate page shells and navigation implementations for every Card kind.
- Native Evidence adapters, format sniffing, and framework-specific ingestion
  for JUnit, SARIF, pytest, or other producer formats.
- Additional Wyrd-owned Evidence schemas beyond `TestRunEvidence` until a
  concrete producer and verifier workflow earns them.
- A general-purpose Service dashboard builder, arbitrary dashboard layout,
  browser-owned chart configuration, or raw SQL chart authoring. The Service
  mocks may show one optional custom chart slot, but durable configuration is
  deferred until its server-owned contract is specified.
- User- or agent-authored view persistence, a drag-and-drop builder, an A2UI or
  other remote-rendering protocol, arbitrary component loading, arbitrary HTML
  or CSS, executable callbacks, and browser-direct data fetching. Those require
  a separately approved security and durable-contract change.
- Durable Change Request, Evidence, Verifier, Verification Result, or Decision
  Receipt wire contracts, persistence, execution, ingestion, and policy. Those
  belong to `changes/active/verified-change-contract/spec.md`.

## Definitions

- **Change Request**: the long-lived, provider-neutral coordination record for
  an intended service change shared across Product, Data Science, Engineering,
  and other stakeholders. It explains the proposed change, intent, Claims,
  linked implementation work, verification, and coordination history.
- **Change Request lifecycle**: `draft`, `open`, or `closed`. A closed Change
  Request records `completed` or `cancelled` as its closure reason.
- **Change Request revision**: an immutable proposed state containing one or
  more Change subjects and its semantic verification requirements.
- **Change subject**: one stable member of a Change Request revision identifying
  a repository, exact base and candidate commits, and optional provider/PR
  reference. A revision may contain multiple subjects from one repository or
  from different repositories.
- **Claim**: a statement that must be established for a Change Request revision.
- **Evidence**: immutable standardized data supplied for a Change Request
  revision. Evidence is data, not verification or decision state.
- **Verifier**: a reusable, versioned mechanism capable of judging whether
  declared Evidence and source inputs support a Claim about an exact change.
  A Verifier may be deterministic, agent/LLM-backed, or implemented by an
  external review system while returning the same portable result contract.
- **Verifier Card**: Wyrd's declarative, governed representation of a Verifier.
  It is a seventeenth registrable Card kind.
- **Verification result**: one Verifier's judgment about one Claim for an exact
  Change Request revision, bound to the Evidence it considered.
- **Verification run status**: `queued`, `running`, `completed`, `cancelled`,
  `timed_out`, or `errored`. It describes execution, not judgment.
- **Verifier verdict**: `passed`, `failed`, or `inconclusive`, present only for
  a completed Verification run.
- **Claim resolution**: `pending`, `satisfied`, or `not_satisfied`, derived from
  the current required Verifier results for that Claim.
- **Discussion thread**: a stable Change Request conversation with a typed,
  revision-aware anchor and reversible resolution state.
- **Comment**: one stable authored message within a Discussion thread. Replies,
  mentions, and anchors refer to this stable identity rather than an editable
  body revision.
- **Comment revision**: one immutable created, edited, or deleted representation
  of a Comment's canonical GitHub Flavored Markdown body, editor, time,
  resolved mention identities, and predecessor identity or digest. Rendered
  HTML is derived and sanitized rather than authoritative.
- **Service composition**: the service-centered hierarchy formed from a
  Service Card's direct and transitively linked, independently versioned Cards.
  Membership in this hierarchy is a navigational and operational relationship;
  it does not embed a linked Card, transfer its durable ownership, or remove its
  independent route and lifecycle.
- **Service operational workspace**: the Card-local workspace for answering
  what is happening to one exact Service Card version over one visible time
  range, what needs attention, which component or signal explains it, and
  where to investigate next. It projects authorized server-owned observation
  state and links into canonical Observe routes; it does not own telemetry,
  calculate health in the browser, or create Service-scoped signal routes.
- **Experiment workspace**: the Card-local analytical workspace used to
  understand an Experiment definition and the runs produced against one exact
  Experiment Card version. It is not a static metadata record and does not
  make the browser or mock fixture the durable run authority.
- **Experiment run**: one model, agentic, evaluation, or related execution
  associated with an exact Experiment Card version. A run is operational
  state rather than a Card and may expose lifecycle, inputs, parameters,
  environment, measurements, outputs, provenance, and linked Cards.
- **Experiment output**: run-associated information presented according to
  its meaning: scalar or historical Metrics, structured Tables, previewable
  Visuals, arbitrary Files, or independently registered and versioned Artifact
  Cards. These categories MUST NOT be flattened into an undifferentiated file
  list.
- **Card workspace**: focused kind-specific content rendered inside the shared
  `/cards/[uid]` shell. A workspace may expose URL-backed local views when its
  workflow earns them, but it does not create a separate kind route tree.
- **Workbench component**: a reusable presentational Svelte component with one
  focused job and a typed contract. Reuse does not make the component a durable
  domain owner or require every workspace to share the same composition.
- **View component catalog**: the allowlisted, machine-readable subset of
  workbench component contracts suitable for future declarative composition.
  It excludes application chrome, authentication, tenant selection, routing,
  server access, and executable behavior.

These definitions fix the user-facing vocabulary and state separation required
by the mocks. They do not define the durable wire or persistence contract,
which is owned by `changes/active/verified-change-contract/spec.md`.

## Required behavior

### Product shell and routing

- **REQ-001**: The product MUST expose only five primary navigation entries:
  Home, Cards, Observe, Changes, and Query.
- **REQ-002**: `/` MUST be the authenticated tenant-resolution entry point.
  Zero authorized tenants MUST produce an access or provisioning state; one
  authorized tenant MUST redirect to `/t/[tenantKey]`; multiple authorized
  tenants MUST use a still-authorized recent selection or present a minimal
  tenant chooser. `/t/[tenantKey]` MUST be the operational homepage centered
  on actionable items, recent work, compact linked summaries, Card lookup, and
  Change Request creation.
- **REQ-003**: Card routes MUST include `/cards` and `/cards/[uid]`.
- **REQ-004**: Observe routes MUST include `/observe`, `/observe/logs`,
  `/observe/metrics`, `/observe/traces`,
  `/observe/traces/[traceId]`, `/observe/dashboards`,
  `/observe/dashboards/[id]`, `/observe/evaluations`,
  `/observe/evaluations/[recordId]`, and `/observe/drift`.
- **REQ-005**: Change Request routes MUST include `/changes`, `/changes/new`,
  `/changes/[id]`, `/changes/[id]/verification`, `/changes/[id]/review`, and
  `/changes/[id]/timeline`. Each linked Change subject MUST have a shareable
  `/changes/[id]/subjects/[subjectId]` drilldown route.
- **REQ-006**: Claims, Evidence, activity, verifier administration, and future
  mention inboxes MUST NOT become top-level navigation areas merely because
  they are independent data types.
- **REQ-007**: A future mention inbox MUST live within Changes because its
  actions and anchors are Change Request scoped.
- **REQ-008**: Future provider connection management MUST live under settings
  or integrations and MUST appear only when the underlying capability exists.
- **REQ-092**: Except for the `/` tenant-resolution entry point and
  authentication callbacks, every product route in this specification MUST be
  nested under `/t/[tenantKey]`. Short route names such as `/cards`,
  `/observe`, `/changes`, and `/query` denote tenant-relative suffixes; their
  canonical browser locations are `/t/[tenantKey]/cards`,
  `/t/[tenantKey]/observe`, `/t/[tenantKey]/changes`, and
  `/t/[tenantKey]/query`.
- **REQ-093**: `tenantKey` MUST be an immutable, non-secret public routing key
  backed by the tenant's durable identity. A tenant display name MAY change
  without changing the key or breaking canonical links. The key expresses
  requested routing context only: the server MUST resolve it at the
  authentication boundary, verify access, and establish tenant-bound session
  context before returning tenant data. URL parameters, headers, browser
  state, and payloads MUST NOT select the effective `DataTenantId`.
- **REQ-094**: The shell MUST always display the current tenant identity. It
  MUST be noninteractive when the principal can access only one tenant and a
  searchable switcher when more than one is authorized. Switching MUST use a
  CSRF-protected server action, revalidate membership, obtain tenant-bound
  session context, require tenant-specific reauthentication when policy
  demands it, and navigate to the destination tenant homepage. Switching one
  browser tab MUST NOT retarget another tab or carry a resource-specific path
  into the destination tenant.
- **REQ-095**: `space` MUST remain subordinate to the authenticated tenant and
  MUST NOT become a second global context switcher. Applicable pages MAY expose
  it as a URL-backed filter, defaulting to all spaces the principal is
  authorized to read where that workflow supports cross-space results.
- **REQ-070**: `/query` MUST be a distinct raw OLAP workbench rather than an
  Observe subpage. It MUST provide a catalog/schema explorer, SQL editor,
  run/cancel controls, visible execution state and timing, and a tabular results
  viewer using mocked BFF responses in this change. It MUST project Oracle's
  authorized, read-only, single-`SELECT` query contract and its timeout,
  function, row, and byte ceilings; it MUST NOT imply table mutation, data
  loading, destination writes, or warehouse administration.
- **REQ-083**: The initial Query workbench MUST use one unsaved editor with a
  searchable Bifrost catalog/schema/table/column explorer and one results pane
  exposing `Results`, `Query Details`, and `History`. Query Details MUST show
  query identity, execution status, elapsed time, rows returned, bytes scanned,
  and any structured error. The mock set MUST demonstrate empty, running,
  successful, cancelled, and failed execution without introducing separate
  pages. Saved worksheets, multiple editor tabs, charts, sharing,
  collaboration, and query profiles are deferred until their persistence and
  user workflows are established.

### BFF and authentication

- **REQ-009**: Browser code MUST communicate with SvelteKit UI/BFF routes and
  MUST NOT call Wyrd domain services, databases, Bifrost, GitHub, object
  storage, or verifier runtimes directly.
- **REQ-010**: The initial BFF MUST return typed mock domain responses from real
  server route/load/action boundaries shaped for eventual Wyrd APIs.
- **REQ-011**: Local development authentication MUST provide realistic tenant,
  subject, scope, expiry, and CSRF context through an HttpOnly session.
- **REQ-012**: Browser-visible page data MUST contain only safe session metadata
  and MUST never contain credentials, bearer tokens, refresh tokens, provider
  secrets, or verifier secrets.
- **REQ-013**: Replacing local authentication and mocked domain clients with
  Wyrd server implementations MUST NOT require changing browser route or
  component contracts unnecessarily.
- **REQ-100**: SvelteKit server loads and form actions MUST be the initial BFF
  boundary. They MUST call one server-only typed Wyrd client, backed by mocks
  in this change and later by canonical Wyrd `/v1` routes through the internal
  gateway. The UI MUST NOT create a duplicate `/api/ui` proxy tree. A separate
  browser-callable JSON or streaming endpoint MAY be added only when a concrete
  interaction cannot be served cleanly by a page load or action.
- **REQ-101**: BFF and UI errors MUST preserve Wyrd's existing RFC 9457
  `WyrdProblem` shape and stable error catalog. Loads, actions, and any earned
  browser-callable endpoint MUST expose only safe problem fields and MUST NOT
  invent a competing UI error vocabulary or leak upstream credentials and
  internal connection details.

### Card experience

- **REQ-014**: `/cards` MUST support lookup and filtering across all registrable
  Card kinds, including Verifier.
- **REQ-015**: `/cards/[uid]` MUST provide one shared Card shell for identity,
  version selection, metadata, relationships, and common navigation.
- **REQ-016**: Card-kind components MUST be added only when the kind exposes
  materially different data or workflows.
- **REQ-017**: The initial focused Card workspaces MUST cover, in product
  priority order, Service, Experiment, Model, Agent, Prompt, Drift, Eval, Data,
  Operator, and Trigger. Workflow structure and Verifier configuration MUST
  receive earned specialized presentations inside the shared Card shell but do
  not require separate major workspace implementations. Every other kind uses
  the shared typed Spec presentation until a materially different workflow is
  approved.
- **REQ-018**: Eval and Drift result exploration MUST remain canonical Observe
  workflows. Their declaring Card pages MAY present contextual calculated
  summaries and drilldowns from those workflows, but MUST keep declared Card
  configuration distinct from runtime observations and calculated results.
- **REQ-019**: Card kinds without an earned specialized workflow MUST use the
  shared typed Spec presentation rather than receive a duplicate page tree.
- **REQ-080**: The Card mock set MUST include the Card inventory, the generic
  shared detail presentation, the ten focused Card workspaces named by
  REQ-017, and earned Workflow and Verifier presentations. These are
  presentations of `/cards/[uid]`, not separate route trees.
- **REQ-110**: Prompt detail MUST treat the prompt definition as its dominant
  work region. It MUST distinguish system instructions and ordered message
  roles; render supported text, image, audio, file, and tool-call content;
  expose variables and media variables with required/default state; show
  provider, model, relevant model settings, and response schema; provide a raw
  definition escape hatch; and link to consuming Agents and Services without
  exposing secrets.
- **REQ-111**: Agent detail MUST make its registered Prompt the primary linked
  dependency and provide contextual Prompt inspection without copying that
  Prompt's durable identity. It MUST also expose tools, run limits,
  publication targets, containing Services, and the relationships among them.
- **REQ-112**: Service detail MUST be the single operational plane for
  understanding one exact Service Card version. Its default Overview MUST
  answer what is happening now, why it needs attention, which component or
  signal explains that state, and where to investigate next. It MUST read as an
  intuitive mini-dashboard rather than a status report: one compact assessment
  followed by visible operational and published-signal trends. The
  deterministic Service composition remains directly reachable but MUST NOT
  dominate the default operational view. Each linked Card MUST retain its own
  identity, version, status, and direct route; the same Card MAY participate in
  more than one Service hierarchy.
- **REQ-113**: Service composition and earned Card relationship views MUST use
  a legible deterministic layout that labels relationship meaning and
  direction, identifies Card kind/name/version/status, supports selection and
  contextual inspection, and provides direct navigation. They MUST distinguish
  distinct authored aliases or reference occurrences that resolve to the same
  Card. They MUST NOT imply that a declared relationship proves a runtime
  execution or observation occurred.
- **REQ-123**: The Service-local primary subnavigation MUST be exactly
  `Overview`, `Composition`, and `Definition`. Overview MUST be the default and
  dominant operational view. Composition MUST preserve the complete linked
  hierarchy and contextual Card inspection. Definition MUST present the
  Service declaration, entry point, components, publication bindings,
  principal and policy context when available, metadata, and raw-spec
  disclosure. The shared Card header MUST retain identity, Card status, and
  version selection; version selection MUST NOT become a duplicate tab.
- **REQ-124**: Service Overview MUST visibly separate Card lifecycle state,
  deployment state when authoritative, operational state, and observation-data
  freshness in one compact header. It MUST then make three core operational
  trends dominant: requests per second, error rate, and latency. Availability
  MAY remain a compact indicator unless meaningful history exists. Each chart
  MUST expose its unit, latest value, freshness, and threshold or budget when
  applicable; multi-series charts and thresholds MUST remain legible without
  color. The browser MUST NOT infer a healthy operational state from an active
  Card or from missing data.
- **REQ-126**: Service Overview MUST render concise Drift and Eval trend panels
  only when the selected Service version or its components publish to those
  Cards. Each panel MUST retain the publishing component or Service subject,
  exact Card version, current result, threshold or pass context, sample and
  freshness context when available, and a scope-preserving canonical Observe
  link. Breached or failed published signals MUST precede healthy signals. One
  optional custom-metric chart slot MAY be shown as an opt-in server projection
  with an `Add chart` empty state; it MUST NOT become a general dashboard
  builder or browser-owned durable configuration. Attention, component state,
  and recent activity MUST remain available as subordinate context without
  crowding the primary trends.
- **REQ-125**: Service operational state MUST be scoped to the selected exact
  Service Card version and one visible, URL-backed time range. Historical
  versions MUST show only version-scoped observations or an explicit no-data
  state; they MUST NOT display the current version's health without
  qualification. Logs, Metrics, Traces, Evaluations, and Drift affordances MUST
  link to their canonical Observe routes with compatible Service, version, and
  time scope preserved and a reversible path back to the Service. Attention
  items MUST identify their originating signal and link to that canonical
  investigation. The UI MUST NOT add an Alert Card, a Service-owned signal
  route, or a standalone `/observe/alerts` destination.
- **REQ-114**: Eval detail MUST expose the declared evaluation workflow and its
  ordered/dependent tasks. Selecting a workflow or task MUST provide sufficient
  contextual inspection for its definition while preserving the surrounding
  Eval Card. Runtime task results remain distinct and link to the canonical
  Observe evaluation workflow.
- **REQ-115**: Drift detail MUST separate declaration from calculated results.
  Declaration MUST show method/profile, signal-specific features or metric,
  baseline/Eval/Source reference, condition and thresholds, publishers, and
  Trigger/Operator relationships. Calculated result presentation MUST support
  feature/metric selection, time-series values, applicable baseline or control
  bounds, sample/data-quality context when available, latest calculation,
  alerts, and a filter-preserving link to canonical Observe drift exploration.
- **REQ-116**: Trigger detail MUST show schedule and timezone, optional Eval or
  Drift observation source, optional subject filter, linked Operator,
  reachable publishing Service/components, and a plain-language explanation of
  the wiring. Runtime firing history, when projected, MUST remain distinct from
  the declaration.
- **REQ-117**: Operator detail MUST show exactly one action and its
  variant-specific configuration, invoking Triggers, upstream Eval or Drift
  context, optional execution budget, redacted credential references, and
  contextual execution outcomes when available. Workflow actions MUST link to
  the referenced Workflow; notification and HTTP actions MUST not imply a Card
  target that the declaration does not contain.
- **REQ-118**: Experiment detail MUST be a complete Card-local analytical
  workspace with Overview, Runs, Compare, Outputs, and Versions
  subnavigation. The workspace MUST present an Experiment as a grouping and
  comparison context for runs rather than as a single completed record.
- **REQ-119**: Experiment Runs MUST support searchable and filterable inventory,
  selected-run inspection, and visible empty, queued, running with partial
  output, completed, failed, and cancelled states. Common run inspection MUST
  cover lifecycle, parameters, environment, provenance, linked Cards, and
  outputs. Model runs MUST earn model-training detail, and agentic runs MUST
  earn trace, tool, token/cost/latency, and evaluation detail without becoming
  a separate product.
- **REQ-120**: Experiment comparison MUST be a distinct workflow driven by an
  explicit compatible run selection and baseline. It MUST compare parameters,
  scalar metrics, aligned metric histories, environment, provenance, linked
  Cards, and available outputs. A metric delta MUST NOT imply improvement or
  regression unless authoritative directionality is available.
- **REQ-121**: Experiment Outputs MUST distinguish Metrics, Tables, Visuals,
  Files, and registered Artifact Cards; preserve the producing run; and expose
  appropriate inspection for each. Arbitrary files MUST NOT be presented as
  independently registered Artifact Cards, and visualizations MUST identify
  their measure, axes or domain, and producing run.
- **REQ-122**: Experiment selection, filters, subnavigation, selected run,
  comparison set, output selection, and version selection MUST be represented
  in restorable URL state. The mock phase MAY use query parameters on the
  shared `/cards/[uid]` route and MUST NOT create a kind-specific production
  route contract.

### UI comprehension

- **REQ-060**: The ordinary Change Request UI MUST answer: what is changing,
  why it is changing, who is affected and coordinating it, what must be true,
  what Evidence arrived, which Verifiers checked it and what they concluded,
  and whether the exact revision is Verified. It MUST use plain product and
  service language first while allowing progressively deeper technical detail;
  it MUST NOT split the shared record into separate Product, Data Science, and
  Engineering experiences. `/changes` MUST
  use a familiar searchable list rather than a kanban board. Each row MUST show
  title, owner, lifecycle state, linked repositories/PRs, required Claims
  satisfied versus total, Verifiers needing attention, and last activity.
  Initial filters MUST be `Open`, `Needs attention`, `Verified`, and `Closed`.
- **REQ-061**: Wyrd implementation nouns such as Card envelope details,
  Verifier binding, configuration digests, native payload formats, and policy
  internals MUST remain in contextual details or administration views rather
  than dominate the primary workflow.
- **REQ-062**: Change Request Overview MUST show intent, current immutable
  revision, proposed behavioral and service impact, owners and participating
  teams, every linked repository subject with its exact base/candidate identity
  and optional PR, and separate verification and authorization state.
  `/changes/new` MUST use one progressive draft form for title/intent,
  subjects, Claims, Verifier requirements, and run modes. Users MUST be able to
  save an incomplete Draft and continue from its detail route; creation MUST
  NOT require a wizard or artificial step sequence.
- **REQ-063**: Change Request Verification MUST show Claims, required and
  advisory Verifiers, received Evidence, results, failures, stale results, and
  results carried forward from unchanged declared inputs, plus the decision
  explanation without mixing their states. Its summary MUST remain legible to
  non-engineers while technical users can drill into runs, cases, provenance,
  source inputs, and provider activity.
  The UI MUST derive `Not ready`, `Not run`, `Verifying`, `Needs attention`,
  and `Verified` summaries from authoritative inputs, runs, verdicts, and Claim
  resolutions. `Stale`, `Carried forward`, and `Overridden` MUST remain
  provenance or authorization labels rather than additional verdicts.
- **REQ-064**: Change Request Review MUST own discussions, anchored findings,
  approvals, cross-team messages, and Change-scoped mentions. Conversations
  MUST remain anchored to the relevant Change Request, Claim, subject, Evidence,
  Verifier result, or revision-aware source location rather than becoming an
  unrelated chat. Threads and Comments MUST retain stable identities; every
  body edit MUST append an immutable Comment revision rather than replace the
  Comment or assign it a new identity. Mentions MUST use stable user/team
  identities resolved from familiar `@name` authoring. Resolve and reopen MUST
  be thread transitions, not comment deletion. The UI MUST expose familiar
  GitHub-style author, timestamp, edited-history, reply, mention, and
  resolve/reopen affordances. Writes MUST be retry-safe and reject edits based
  on a stale expected revision.
- **REQ-065**: Change Request Timeline MUST show immutable revision and audit
  history, including linked commit and PR movement, CI/CD and Evidence activity,
  Verifier execution, decisions, overrides, and human coordination. Users MUST
  be able to drill into source changes or navigate to the owning provider when
  available. The subject drilldown MUST show repository/provider identity,
  optional PR,
  exact base and candidate commits, commit history, changed files, a simple
  read-only unified diff, and an external provider link. It MUST NOT add source
  editing, merge authority, or a parallel provider review system.
- **REQ-082**: The Change Request mock set MUST cover the searchable list,
  progressive new/draft form, Overview, Verification, Review, Timeline, and
  subject/commit drilldown routes. Representative records within those pages
  MUST demonstrate the required lifecycle, execution, verdict, Claim,
  provenance, discussion, and authorization states; the mock set MUST NOT
  multiply full pages solely to show each status independently.
- **REQ-099**: Typed mock view models and fixtures MUST provide the user-facing
  Change Request, revision, subject, Claim, Evidence, Verifier, run, result,
  discussion, and authorization fields required by REQ-060 through REQ-065.
  Fixtures MUST keep lifecycle, execution status, verdict, Claim resolution,
  provenance, and override state visibly separate; include multiple subjects,
  Claims, and required/advisory Verifiers; and demonstrate manual and automatic
  run modes with clear billable-action warnings. These view models are a
  temporary UI projection, not the durable wire, persistence, ingestion, or
  execution contract, and MUST remain replaceable by the eventual approved
  `verified-change-contract` without moving domain logic into the browser.

### Observability experience

- **REQ-077**: `/observe` MUST be the operational-health and investigation
  entry point across observed Wyrd Cards, Runs, and signals. It MUST summarize
  recent traffic, latency, error, activity, and attention signals without
  making Service identity the hierarchy for observations, and it MUST provide
  direct filtered drilldowns into relevant traces, logs, metrics, Eval, and
  Drift views. It MUST NOT duplicate the general Wyrd homepage or become a
  second customizable dashboard.
- **REQ-079**: Observe sub-navigation MUST group `Overview` under
  `Overview`; `Logs`, `Metrics`, and `Traces` under `Explore`; and
  `Dashboards`, `Evaluations`, and `Drift` under `Analyze`. These group labels
  organize one Observe area and MUST NOT create additional top-level product
  areas.
- **REQ-071**: Observe MUST follow the familiar observability flow of choosing
  a signal, selecting a time range, narrowing results with common facets,
  inspecting trends and matching records, and drilling into one record without
  losing the active search context.
- **REQ-072**: Trace search MUST support service name, service namespace,
  service version, service instance, status, error presence, duration, time
  range, and attribute filters. Its primary result MUST combine compact trend
  context with a searchable trace table rather than present a decorative
  dashboard alone.
- **REQ-073**: Trace detail MUST show trace identity, service, operation,
  timing, status, error and span summaries; an inspectable span waterfall and
  service graph; and selected-span attributes, events, links, status, and
  timing. AI-specific span content MAY appear only when that content exists.
- **REQ-074**: Logs and Metrics MUST use the same time-range and filter
  vocabulary where their data supports it. Logs MUST provide searchable
  records with structured detail. Metrics MUST provide metric discovery,
  label filtering, visualization, and underlying values. Observe dashboards
  MUST provide a dashboard inventory and read-only dashboard view in this
  change; dashboard editing, alerting, and scheduled searches are deferred.
- **REQ-078**: Logs MUST provide a guided signal-investigation interface rather
  than a second unrestricted SQL workbench. Its primary controls MUST cover
  stream or service, time range, level or status, attributes, and text search,
  followed by trend context, matching records, and structured record detail.
  An `Open in Query` action MAY transfer an equivalent query and context into
  `/query`; advanced raw SQL editing MUST remain owned by `/query`.
- **REQ-084**: Observe signal pages MUST remain canonical unfiltered homes;
  Service, Card, Run, principal, experiment, request, trace, span, time, status,
  OpenTelemetry, and signal-specific dimensions are filters and MUST NOT become
  parent route hierarchies. Every applicable filter MUST be represented in the
  URL, rendered as visible removable filter state, and restored by direct
  navigation. Cross-signal links MUST preserve compatible filters and time
  range. `View all` MUST remove the inherited scope. Only independently
  identifiable records MAY receive detail paths; Card declarations remain at
  `/cards/[uid]` and link into canonical Observe pages with URL filters.
- **REQ-085**: The UI and BFF MUST query correlation context already injected
  and stored during authenticated observation emission. They MUST NOT create,
  infer, or rewrite Card, Run, principal, Service, experiment, request, or
  telemetry correlation identity in the browser. Any later typed-query work is
  limited to projecting existing authorized Bifrost dimensions consistently,
  not adding a second correlation path.
- **REQ-086**: `/observe/evaluations` MUST present a searchable, filterable
  history of individual Eval events. One event is identified by its stable
  `record_id`; its `run_id` correlates it with the broader subject invocation
  and MUST NOT replace the event identity. Selecting an event MUST navigate to
  the shareable `/observe/evaluations/[recordId]` workflow-execution view.
- **REQ-087**: The Eval workflow-execution view MUST show the resolved Eval
  Card, subject and correlation context, lifecycle and pass summary, execution
  stages, and per-task results. Selecting a task MUST set the URL-backed
  `task=<taskId>` state and display its type, status, stage, operator,
  expected and captured actual value when authorized and retained, score,
  timing, explanation, and related trace. The workflow and selected-task panes
  are two views of one Eval event; they MUST NOT create separate Workflow or
  Task resource hierarchies.
- **REQ-088**: `/observe/drift` MUST present calculated Drift results against
  the user-authored Drift definition. It MUST support URL-backed filtering by
  Drift Card, subject Card, principal, Service, Run, method, signal,
  feature or series, verdict, and time where those dimensions apply. When one
  Drift Card is selected, the page MUST show a compact read-only definition
  summary alongside overall verdict, feature or metric selection, calculated
  score history, applicable baseline or threshold overlays, per-feature score
  and verdict rows, relevant alerts, and correlation links. Drift methods MUST
  NOT become separate route trees.
- **REQ-089**: The UI and BFF MUST keep raw Drift input observations distinct
  from calculated Drift reports and MUST NOT label or derive one as the other.
  The eventual server-backed Eval query contract MUST expose event summaries,
  execution plans, and authorized per-task results; the eventual Drift query
  contract MUST expose calculated reports and verdicts. Until those contracts
  are integrated, typed mocks MUST represent the intended shapes and the
  browser MUST NOT reconstruct workflow execution or Drift calculations.
- **REQ-090**: The Observe mock set MUST include the Eval event inventory, one
  Eval workflow-execution view with a selected task, and one Drift results view
  with definition context, calculated metric history, feature verdicts, and
  alerts. Eval Card configuration remains a `/cards/[uid]` presentation; the
  mock set MUST NOT add task routes, standalone workflow routes, Drift result
  routes without stable result identity, or method-specific Drift routes.
- **REQ-091**: The `/observe` overview MUST use one shared URL-backed time and
  correlation filter bar, compact Logs, Metrics, Traces, Eval, and Drift
  summaries, one cross-signal attention feed, and recent observation activity.
  Every summary and attention item MUST link to the corresponding canonical
  signal page with compatible filters preserved. It MUST NOT introduce a
  Service inventory, customizable panels, or duplicate the full charts owned
  by the signal pages and read-only dashboards.

### Reusable and composable UI foundation

- **REQ-127**: Repeated Wyrd interaction and presentation needs MUST use a
  small shared component foundation when at least two real first-party
  consumers require the same behavior. The initial foundation MUST cover
  status and metadata badges, filter and time controls, empty/loading/error and
  authorization states, panels, tabular data, metric summaries, and chart
  framing. It MUST reuse installed Svelte and browser capabilities before
  introducing local infrastructure or a new dependency.
- **REQ-128**: Every standardized component contract MUST agree across
  `brand/components.json`, its Svelte implementation, its registry entry, and
  its style-guide or focused test evidence. Supported variants and semantic
  values MUST map to current theme tokens; obsolete tokens and undocumented
  variants MUST NOT survive as parallel APIs.
- **REQ-129**: Application infrastructure and chrome, including the shell,
  sidebar, top bar, tenant identity, authentication/session controls, routing,
  and server data access, MUST remain trusted application code and MUST NOT be
  exposed through the future-facing view component catalog. Composed content
  always renders inside that trusted boundary.
- **REQ-130**: Catalog-facing component inputs MUST be typed, JSON-safe, and
  semantic: raw values, labels, units, bounded enums, links, and declarative
  child content. They MUST NOT accept theme-token names, arbitrary colors,
  classes, CSS, raw HTML, scripts, executable callbacks, network URLs for data
  loading, or browser-owned queries. First-party Svelte components MAY use
  typed snippets internally without making snippets part of the serializable
  catalog contract.
- **REQ-131**: Charts MUST use one reusable chart frame for title, measure,
  unit, latest value, source, freshness, visible time range, canonical link,
  and loading, no-data, unauthorized, partial, and error states, composed with
  the smallest chart primitives needed by the approved mocks. Thresholds MUST
  be labeled, time axes MUST use `<time>` semantics where applicable, and
  multiple series MUST remain distinguishable without color. The foundation
  MUST NOT create one component per metric or one universal chart component.
- **REQ-132**: Route pages and Card workspaces MUST retain workflow-specific
  composition, hierarchy, copy, URL state, and actions. Shared components MUST
  NOT force a universal Card, table, dashboard, or page template, and a shared
  visual treatment MUST NOT imply shared domain meaning.

### Visual and deployment foundation

- **REQ-066**: The current `wyrd-ui/brand` directory MUST be the visual and
  theming source of truth, including its approved tokens, contrast rules,
  component contracts, 5px geometry, hard shadows, and light/dark behavior.
  GitHub familiarity MUST govern relevant interaction patterns, information
  hierarchy, control placement, and status language, but MUST NOT replace or
  dilute Wyrd's visual identity.
- **REQ-081**: Every product route and reusable component MUST support both
  light and dark modes with equivalent information, contrast, interaction,
  and accessibility. Both modes MUST use the same component structure and
  locked geometry; theme tokens alone change their palette. The route-mapped
  SVG mock set MUST make every required page reviewable in both modes without
  creating independently maintained light and dark designs.
- **REQ-067**: Useful OpsML BFF, route, and domain-view patterns MAY be adapted
  into Wyrd vocabulary, but OpsML routes, product names, contracts, duplicated
  per-kind layouts, and styling MUST NOT be copied as authority.
- **REQ-068**: SVG mockups MUST be directly viewable and MUST correspond to the
  expected implemented product routes rather than depict disconnected concept
  screens.
- **REQ-069**: The deployed UI MUST run as the existing SvelteKit adapter-node
  application in a Docker container served through Nginx using one repository-
  native deployment shape.
- **REQ-102**: Every external browser, SDK, CLI, MCP, agent, HTTP, and gRPC
  client MUST use one configured Wyrd base address. Nginx or an equivalent
  ingress MUST expose one external origin/authority and be the single public
  L7 gateway: UI paths route to SvelteKit; canonical `/auth/*` and `/v1/*`
  HTTP paths and public gRPC methods on the same gateway route to their owning
  Wyrd server role. External clients MUST NOT configure or discover Node,
  Oracle, Scribe, or Forge-worker addresses.
- **REQ-103**: In an all-in-one deployment, Nginx, SvelteKit, and
  `wyrd-server` target `all` MUST communicate over private local listeners. In
  a multi-role Kubernetes deployment, the UI gateway and SvelteKit MAY be
  colocated with target `server`, while the gateway routes Oracle queries and
  Scribe ingestion to ready internal role Services. Forge workers MUST have no
  public client route. Internal service names and ports MUST be deployment
  configuration rather than browser or SDK contract.
- **REQ-104**: Deployment targets MUST be exclusive: `all` runs the complete
  deployment; `server` runs core Wyrd serving and control-plane behavior
  without a local Oracle, Scribe, or Forge worker; `oracle` runs Oracle query
  behavior; `scribe` runs Scribe ingest behavior; and `forge-worker` runs Forge
  work without a public serving surface. Server-owned Forge coordination MAY
  remain in target `server`. The current broader `server` target matrix is
  implementation drift and MUST NOT cause duplicate data-plane roles in a
  split deployment.
- **REQ-105**: Nginx and ingress MUST forward authenticated requests without
  becoming identity or authorization authority, MUST preserve canonical Wyrd
  errors, MUST support HTTP and gRPC streaming without unbounded buffering,
  and MUST NOT retry non-idempotent writes. Each role MUST expose direct
  role-aware liveness and readiness for Kubernetes; the public gateway health
  path MUST additionally prove that the gateway can reach its required local
  serving processes. Cross-pod gateway-to-role traffic MUST use authenticated
  encryption and explicit trust roots; Kubernetes network location alone MUST
  NOT establish trust.
- **REQ-106**: The browser MUST hold only its HttpOnly SvelteKit session. The
  BFF MUST attach the current Wyrd access credential to server-side calls using
  Wyrd's canonical `X-Wyrd-Access-Token` contract. Nginx MUST forward that
  header unchanged and MUST NOT translate browser cookies into credentials or
  tenant identity. The stale OpsML-style `/api/*` proxy and generic
  `Authorization` injection MUST NOT be reproduced.
- **REQ-075**: The first planned implementation task MUST produce the complete
  route-mapped SVG mock set for Home, Cards, Observe, Changes, and Query before
  any Svelte view implementation begins. Shared shells and repeated states MUST
  be represented once and referenced by every route that uses them; Card kinds
  MUST NOT receive separate mock pages when the shared Card shell expresses
  their behavior.
- **REQ-096**: The SVG task MUST provide a complete desktop mock at an
  approximately 1440-pixel viewport for each distinct approved page: tenant
  entry, tenant Home, Cards inventory, generic Card detail, the ten approved
  focused Card workspaces plus earned Workflow and Verifier presentations, the
  ten approved Observe pages, the seven Change Request pages, and Query. Every
  page MUST have reviewable light and dark renderings produced from the same
  structure. Material states MAY be demonstrated within one page composition
  and MUST NOT multiply otherwise identical full-page mocks.
- **REQ-097**: The SVG task MUST additionally provide approximately 390-pixel
  light and dark mobile mocks for eight representative responsive layouts:
  Cards inventory, Card detail, Change Request creation, Change Request review
  or subject detail, Trace detail, Eval workflow/task detail, Drift results,
  and Query. It MUST include a route-to-responsive-pattern matrix covering
  every approved route. Reusing a representative pattern MUST NOT exempt any
  implemented route from responsive behavior or remove information,
  authorization context, filters, or actions at narrow widths.
- **REQ-098**: Responsive behavior MUST keep current tenant identity visible;
  replace the desktop sidebar with accessible mobile navigation; retain
  visible URL-backed filter state; use controlled overflow for irreducibly
  dense tables, diffs, and timelines; stack general detail rails below primary
  content; expose Workflow and Task panes as narrow-screen views of the same
  Eval event; stack Drift definition and calculated results; and present the
  Query catalog as a drawer above the editor/results workflow. Responsive
  transformations MUST preserve route and selection state rather than create
  mobile-only domain behavior.
- **REQ-076**: Observe mocks MUST adapt the established search-to-inspection
redacted
  using only Wyrd's approved visual system, terminology, and product
redacted
  copied into Wyrd.

## Invariants and prohibited outcomes

- **INV-001**: The browser never becomes a durable behavior or authorization
  owner.
- **INV-002**: Tenant identity and scope are explicit on every authenticated
  BFF request, including mocks.
- **INV-008**: Color is never the sole carrier of status or chart meaning.
- **INV-009**: The UI does not invent server truth, recompute authoritative
  verdicts, or optimistically display verification success.
- **INV-012**: Technical implementation detail never obscures the shared human
  explanation of what is changing, why, and how the claimed behavior is being
  verified.
- **INV-013**: Change Request discussions and their current projections remain
  tenant-scoped transactional Wyrd control-plane state in Postgres. Bifrost is
  not their write authority; it may contain retained audit or analytical
  projections without becoming the source used for discussion writes or
  current state.
- **INV-014**: Mock fixtures and BFF view models never become the durable
  Change Request, Evidence, or verification protocol authority.
- **INV-015**: A shared Card shell never reduces materially different Card
  workflows to the same generic metadata/table composition.
- **INV-016**: Service hierarchy never erases a linked Card's independent
  identity or version, and Card independence never prevents the UI from
  presenting linked Cards as one composed Service.
- **INV-017**: Experiment mock fixtures may illustrate the approved future
  workspace but never assert that the current server, `wyrd-spec`, Vala, or
  Bifrost already implements Experiment run lifecycle, association, query, or
  output contracts. Those gaps remain explicit deferred work and do not block
  static UI design.
- **INV-018**: A changing run never mutates the meaning of an immutable
  Experiment Card version or becomes a Card merely to power the UI. The UI
  keeps Card version, run lifecycle, and output identity visibly distinct.
- **INV-019**: Service presentation never conflates Card lifecycle,
  deployment, operational, or data-freshness state; never hides the component
  or observation subject behind a Service aggregate; and never reports absent,
  stale, unauthorized, or partially available data as healthy.
- **INV-020**: Service Overview remains immediately scannable: one assessment,
  three core operational trends, earned published-signal trends, and one clear
  investigation path. Supporting detail never becomes an information wall.
- **INV-021**: Future composability never permits an authored view to replace
  trusted application chrome, acquire credentials, choose effective tenant
  identity, execute code, fetch arbitrary data, or become server truth.
- **INV-022**: Reuse never erases the distinct mental model, dominant task, or
  information hierarchy of Change Requests, Observe, Query, or any focused
  Card workspace.

## Acceptance obligations

- **AC-001**: Route evidence demonstrates every required browser route,
  server-load/action boundary, typed mock response, and navigation relationship.
- **AC-002**: Session-boundary evidence demonstrates tenant/scope propagation,
  CSRF enforcement, credential containment, expiry, and unauthenticated
  behavior using the local development provider.
- **AC-003**: View evidence demonstrates the homepage, Card inventory/shared
  shell/earned specializations, Logs, Metrics, trace search and detail,
  Observe dashboard inventory and detail, Eval and Drift workspaces, the SQL
  workbench, and all Change Request views in both supported visual modes.
- **AC-004**: Change Request view evidence visibly separates Evidence,
  Verification Results, Claim resolution, lifecycle, provenance, and
  authorization or override state without presenting any one as another.
- **AC-005**: Change Request fixtures demonstrate multiple subjects and Claims,
  required and advisory Verifiers, manual and automatic run modes, missing
  Evidence, running work, failure, stale and carried-forward results, and an
  authorized override without treating the fixtures as durable contracts.
- **AC-006**: Deployment evidence demonstrates the production SvelteKit build
  running behind the repository's Nginx container boundary with expected route
  fallback and health behavior.
- **AC-007**: The first completed task's SVG mockups map explicitly to every
  route and material state obligation in this specification, reuse one Wyrd
  visual system, and remain viewable without the running application.
- **AC-008**: Card mock evidence demonstrates Prompt message/media inspection,
  Agent-to-Prompt drilldown, Eval workflow/task definition inspection, a
  Service operational Overview with exact version/time scope, a compact state
  header, requests/error/latency trends, publication-driven Drift and Eval
  trends, subject-preserving attention, canonical Observe investigation links,
  subordinate Composition and Definition destinations, Drift declaration and
  metric results, and dedicated Trigger and Operator presentations in light and
  dark modes. Service evidence MUST include healthy, needs-attention,
  stale/no-data, partial-authorization, safe backend-failure, optional custom
  chart, and credible narrow-width Overview and Composition behavior.
- **AC-009**: Experiment mock evidence demonstrates every Experiment
  subnavigation destination, mixed run lifecycle, completed model and agentic
  run inspection, compatible multi-run comparison, Metrics, Tables, Visuals,
  Files, registered Artifact lineage, version history, and narrow-width run and
  output inspection in light and dark modes. The mock ledger identifies every
  fixture-only projection and the deferred production contract gaps it relies
  upon.
- **AC-010**: Component-foundation evidence maps each shared component to at
  least two real first-party consumers; proves catalog, Svelte contract,
  registry, and style-guide or test agreement; and demonstrates keyboard use,
  accessible naming, non-color status/chart meaning, narrow-container behavior,
  and equivalent light/dark information. It separately identifies trusted app
  infrastructure that is intentionally absent from the view component catalog.
- **AC-011**: Integrated view evidence demonstrates Change Requests; Observe
  Traces, Metrics, and Logs; Service, Experiment, Model, Agent, Prompt, Drift,
  Eval, Data, Operator, and Trigger Card workspaces; and the Query OLAP
  workbench consuming the shared foundation without collapsing into one generic
  page composition. The same Card route host renders every Card workspace and
  preserves direct links and URL-backed local state.

## Open material decisions

None.

## Material authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/wyrd-security-posture.md`
- `architecture/operations/README.md`
- `architecture/operations/deployment-and-release.md`
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/domain/evaluation.md`
- `architecture/references/domain/drift-monitoring.md`
- `architecture/bifrost-design.md`
- `changes/active/verified-change-contract/spec.md`
- `crates/wyrd/wyrd-cli/tests/fixtures/card_lifecycle`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`
- `crates/wyrd/wyrd-server/wyrd-ui/brand/palette.json`
- `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui`

## Revision history

- **Revision 6 (approved, 2026-09-04):** Promoted the Data Scientist / AI
  Engineer to the primary UI persona; locked the reusable component foundation
  and a safe semantic catalog boundary for future user- or agent-composed
  views; explicitly deferred the renderer, authoring, persistence, executable
  extension, and arbitrary-layout systems; and fixed the implementation
  priority as Change Requests, Observe, Service, Experiment, Model, Agent,
  Prompt, Drift, Eval, Data, Operator, Trigger, then Query. The user explicitly
  directed this revision and the subsequent replanning, with parallel workspace
  implementation after the shared foundation.

- **Revision 5 (approved, 2026-09-04):** Simplified Service Overview into an
  intuitive mini-dashboard: one compact assessment, three dominant engineering
  trends for requests, errors, and latency, publication-driven Drift and Eval
  trends, subordinate attention/context, and one optional custom chart slot.
  Rejected a general dashboard builder and the revision 4 information-wall
  treatment. The user explicitly approved this direction and limited the mock
  remediation to `brand/renders/product/cards/service/`.

- **Revision 4 (approved, 2026-09-04):** Replaced the graph-dominant Service
  Card direction with a Service-local operational workspace whose default
  Overview answers current state, cause, affected component or signal, and the
  next investigation. Locked `Overview`, `Composition`, and `Definition` as
  the exact Service-local navigation; preserved the deterministic relationship
  graph under Composition; separated Card, deployment, operational, and data
  freshness state; scoped evidence and links to the selected Service version
  and visible time range; and kept Logs, Metrics, Traces, Evaluations, Drift,
  and attention destinations canonical in Observe. Explicitly approved by the
  user after adversarial Product Manager, Data Scientist, and Software Engineer
  usability review.

- **Revision 3 (approved, 2026-09-04):** Expanded Experiment from one summary
  Card mock into a complete mock-first analytical workspace covering Overview,
  Runs, Compare, Outputs, and Versions; model and agentic run inspection;
  lifecycle states; URL-restorable state; and distinct metric, table, visual,
  file, and registered Artifact output semantics. Kept server, `wyrd-spec`,
  Vala, Bifrost, ingestion, persistence, and execution-contract gaps explicit
  and deferred. The user explicitly directed creation of the corresponding
  remediation task after reviewing the OpsML, Wyrd, MLflow, and Weights &
  Biases research.

- **Revision 2 (approved, 2026-09-04):** Corrected Card specialization after
  review of the complete product mocks. Locked Service as the top-level
  hierarchy for directly and transitively linked, independently versioned
  Cards; required deterministic relationship visualization; expanded Prompt,
  Agent, Eval, and Drift inspection; and added dedicated Trigger and Operator
  presentations. Explicitly approved after review of OpsML Prompt/Drift
  precedents, current Wyrd Card contracts, and the Card lifecycle fixture.

- **Revision 1 (approved, 2026-09-03):** Recorded the initial UI scope, route
  system, authenticated BFF boundary, Wyrd visual authority, and mock-first
  delivery. Explicitly approved after the protocol split and gateway-topology
  decision.
- **Revision 1 protocol split (2026-09-03):** Moved durable Change Request,
  Evidence, Verifier, result, ingestion, persistence, and policy behavior into
  `changes/active/verified-change-contract/spec.md`. This UI draft retains only
  the user-facing vocabulary, view behavior, and temporary typed projections
  needed to design and implement the mock-driven interface.
- **Revision 1 gateway-topology addition (2026-09-03):** Selected one public
  Wyrd address behind Nginx or equivalent ingress, canonical `/auth` and `/v1`
  server routes, SvelteKit loads/actions as the BFF, and no duplicate UI API
  namespace. Fixed exclusive `all`, `server`, `oracle`, `scribe`, and
  `forge-worker` target meanings for all-in-one and Kubernetes deployments;
  external clients remain topology-blind.
- **Revision 1 creation-flow addition (2026-09-03):** Made Change Request
  creation one progressively editable draft form rather than a wizard. Drafts
  may be saved before subjects, Claims, and Verifier requirements are complete.
- **Revision 1 cross-functional-workspace addition (2026-09-03):** Reframed the
  Change Request UI as one shared service-change workspace for Product, Data
  Science, and Engineering. Plain-language intent, impact, Claims, and
  verification lead; commits, diffs, CI/CD, Evidence, provenance, and provider
  links remain progressively available, with anchored cross-team discussion.
- **Revision 1 Change-list addition (2026-09-03):** Selected a familiar,
  searchable pull-request-style list with plain lifecycle, subject, Claim,
  Verifier-attention, and activity summaries instead of a kanban board.
- **Revision 1 subject-drilldown addition (2026-09-03):** Added one shareable,
  read-only route per linked Change subject for commits, changed files, a
  minimal unified diff, and navigation to the owning source provider.
- **Revision 1 discussion addition (2026-09-03):** Selected stable Discussion
  thread and Comment identities with immutable Comment revisions, explicit
  user/team mentions, revision-aware anchors, reversible resolution, retry-safe
  writes, and optimistic edit checks. Postgres owns current coordination state;
  Bifrost is limited to audit/analytical projection.
- **Revision 1 comment-format addition (2026-09-03):** Selected GitHub Flavored
  Markdown as the canonical Comment body and GitHub-familiar thread/edit/
  mention interactions. Rendered HTML remains a sanitized derived view.
- **Revision 1 observability addition (2026-09-03):** Expanded Observe to the
  familiar Logs, Metrics, Traces, Dashboards, Eval, and Drift workspaces and
  added a distinct operational-health landing page. Search, common facets, trend
  context, record drilldown, trace waterfall/graph, and span inspection adapt
redacted
- **Revision 1 query-boundary addition (2026-09-03):** Separated guided Logs
  investigation from unrestricted Bifrost SQL. Logs owns common filters,
  trends, records, and structured detail; advanced SQL escalates to the single
  `/query` workbench without creating a second editor.
- **Revision 1 Observe-navigation addition (2026-09-03):** Grouped the Observe
  workspace into operational Overview, telemetry Explore, and higher-level
  Analyze destinations while retaining one top-level Observe product area.
- **Revision 1 linkable-filter addition (2026-09-03):** Made each Observe
  signal page an unfiltered canonical home with URL-authoritative, visible,
  removable filters across the existing observation correlation dimensions.
  Compatible scope follows cross-signal links, no filter dimension becomes a
  parent route, and the browser never recreates correlation already injected
  during authenticated emission.
- **Revision 1 Eval-and-Drift addition (2026-09-03):** Adapted OpsML's useful
  workflow-stage, task-inspection, calculated-metric, definition-context, and
  alert patterns without retaining its Card-scoped or method-specific route
  trees. Eval event identity is `record_id`, task selection is URL-backed
  within one workflow-execution page, and Drift results remain distinct from
  raw input observations. Recorded the later server-query projections required
  to replace the initial typed mocks without browser-owned reconstruction.
- **Revision 1 Observe-overview addition (2026-09-03):** Selected a compact,
  signal-first operational overview with shared URL-backed scope, cross-signal
  attention, and recent activity. Service remains one correlation filter; the
  overview is neither a Service inventory nor a customizable dashboard.
- **Revision 1 tenant-routing addition (2026-09-03):** Adopted the independent
  advisory recommendation for immutable `/t/[tenantKey]/...` canonical paths
  across SaaS and self-hosted deployments. The root resolves authorized tenant
  context, the shell always identifies it, switching is a server-controlled
  tenant-bound session transition, and `space` remains an in-tenant filter.
- **Revision 1 responsive-mock addition (2026-09-03):** Fixed the mock package
  at 32 distinct desktop pages with paired light/dark renderings, eight paired
  representative mobile layouts, and a route-to-pattern matrix covering every
  page. Narrow layouts preserve tenant, filters, data, actions, and URL state;
  repeated responsive transformations are specified once rather than redrawn
  for every route.
- **Revision 1 dashboard-scope addition (2026-09-03):** Limited the first
  Observe dashboard surface to inventory and read-only viewing. Dashboard
  creation and editing remain deferred until durable panel-query, persistence,
  permission, and versioning contracts exist.
- **Revision 1 mock-first addition (2026-09-03):** Required the first planned
  implementation task to deliver the complete route-mapped SVG mock set for
  Home, Cards, Observe, Changes, and the raw OLAP Query workbench before any
  Svelte view implementation.
- **Revision 1 Card-mock addition (2026-09-03):** Fixed the Card mock inventory
  to the shared inventory/detail system plus the earned Data, Model, Prompt,
  Experiment, Agent, Workflow, Service, Eval, Drift, and Verifier detail
  presentations.
- **Revision 1 theme-mode addition (2026-09-03):** Required full light/dark
  parity for every route and component using one structural design and the
  existing Wyrd theme tokens. Every route mock must be reviewable in both
  modes without maintaining separate designs.
- **Revision 1 Change-mock addition (2026-09-03):** Fixed the Change Request
  mock inventory to its seven approved routes and required representative
  mixed states within those pages instead of separate pages for every status.
- **Revision 1 query-safety addition (2026-09-03):** Bound the raw Query
  workbench to Oracle's existing authorized read-only `SELECT` contract and
  safety ceilings. Warehouse mutation and administration controls remain out
  of scope.
- **Revision 1 query-workbench addition (2026-09-03):** Selected one unsaved
  catalog/editor/results workbench with execution details and history. Saved
  worksheets, multi-tab IDE behavior, charts, sharing, collaboration, and
  query profiles remain deferred.
- **Revision 1 mock-first addition (2026-09-03):** Made route-mapped SVG mocks
  for every core product area the mandatory first implementation task. Added a
  distinct Query workbench and expanded Observe to the familiar Logs, Metrics,
  Traces, Dashboards, Eval, and Drift search-to-inspection workflow informed by
redacted
- **Revision 1 visual-familiarity addition (2026-09-03):** Locked GitHub-
  familiar workflow and interaction conventions under Wyrd's uncompromised
  brand, geometry, palette, and light/dark theming.
- **Revision 1 lifecycle addition (2026-09-03):** Limited Change Request
  lifecycle to `draft`, `open`, and `closed`, with `completed` or `cancelled`
  closure reasons. Verification and authorization remain separate dimensions.
- **Revision 1 verification-status addition (2026-09-03):** Separated run
  execution status, Verifier verdict, and Claim resolution, with familiar UI
  summaries derived from those authoritative records rather than stored as a
  competing state machine.
