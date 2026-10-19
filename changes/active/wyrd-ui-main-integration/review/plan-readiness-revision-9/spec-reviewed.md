---
id: SPEC-wyrd-ui-main-integration
revision: 9
status: draft
---

# Migrate the Wyrd UI onto current main

## Objective and authority

Carry useful historical UI work onto current Wyrd in increments that a human can
run and inspect before the next dependent increment. First make the existing
production UI installable and reachable with its existing login. Then migrate
the implemented domain screens, reconciling their data against current contracts.

This revision replaces an invalid draft that claimed revision 1 approval. That
claim is not carried forward. The caller approved revision 5 on 2026-10-19;
revision 9 incorporates the subsequent main update, full-library completion
requirement, replacement readiness reviewer, and the caller's loader/action/SQL
and read-parity decisions. It is draft pending independent readiness review
and explicit revision approval.

Original baseline: `d3ce50626a8e380475c0561d8d53144c212e8603`. On the caller's
instruction, destination branch `wyrd-ui-main-integration` was fast-forwarded
to fetched `origin/main` at `8651b96fd0c049ab5e63b0606c98326af1fc8848` on
2026-10-19. Current-main capability claims must use that committed baseline;
the pre-existing local trace-summary removal is preserved separately and is
not committed-main behavior. Historical implementation:
`wyrd-server-ui` at `7bfc3a8a282daf490a1ee649ce9fe74382d4fd35`.
[Evidence](references/evidence.md) pins the inspected sources and their limits.
Current architecture wins over historical code, mock data, canvases, and this
packet's archived draft.

## Required behavior

### Installation, launch, and existing login — TASK-001 boundary

- **REQ-001 — Native production package.** Extend the existing
  `scripts/build.sh` bundle path to ship the existing adapter-node production BFF
  and its runtime dependencies alongside the Rust server. Installed users need
  no source checkout, package installation, or frontend build to reach the UI.
  Supported native binaries are macOS Apple Silicon (aarch64), Linux x86_64,
  and Linux aarch64. macOS Intel is unsupported. Use system Node;
  do not bundle a Node executable. Document and check the production-compatible
  system-Node prerequisite before launching owned children.
- **REQ-002 — Verified installation.** Extend the existing signed bundle and
  atomic installer workflow to include the UI. An incomplete, corrupt, incompatible,
  or unverifiable package must not replace a usable installation. A successful
  install contains a runnable BFF, not merely an empty directory.
- **REQ-003 — Native lifecycle.** Extend `wyrd server dev` and its existing
  `DevServer` owner to launch the packaged Rust server and BFF, report a usable
  UI address only after readiness, and preserve current first-run database,
  migration, initialization, and saved credential behavior. Missing Node,
  invalid UI payload, occupied ports, partial startup, child failure, interruption,
  and normal shutdown produce a useful result and clean up owned processes.
  No failure may leave an unreported running sibling or damage existing state.
- **REQ-004 — Native browser origin.** Native Rust and Node processes use
  separate loopback ports; the browser enters at the BFF origin. The BFF forwards
  the existing browser `/auth/*` interactions to the configured Rust upstream,
  retaining the browser origin, methods, bodies, path/query, redirects, cookies,
  and error responses. It must not follow the provider redirect server-side.
  Preserve the server's exact `{public origin}/login/callback` validation and
  provider callback derivation. Forwarding targets the configured server, never
  a browser-supplied destination; public-origin headers are established at the
  trusted boundary, not copied blindly from an incoming forwarded-header chain.
- **REQ-005 — Official containers.** Reconcile historical official-container
  Rust + Node + nginx behavior with main's current startup and release owners.
  Containers supply Node and production UI dependencies. nginx presents one
  public application origin: existing Rust API/auth/platform/MCP, metadata,
  health and gRPC routes retain their owner; UI routes reach the BFF. Preserve
  public host/scheme, form origin checks, redirects, cookies, stream behavior,
  exit status, and cleanup of all owned processes. Supported OCI outputs are
  Linux only: Debian on x86_64 and aarch64, and Rocky on x86_64 only. Reconcile the existing
  release matrix with these outputs; Ubuntu images are outside the selected
  distribution matrix.
- **REQ-006 — Existing auth exercised.** Reuse `BrowserSessions`,
  `BrowserSession`, hooks, login/callback/logout routes, the configured confidential
  OAuth client, API-key recovery sign-in, and existing session protection.
  Verify real login through the native public UI path and the container public
  path using repository-owned identity fixtures. Preserve tenant/platform
  separation, PKCE, issuer/state checks, callback validation, refresh, revocation,
  HttpOnly/encrypted cookie behavior, and production HTTPS requirements.
  A mock login or a harness that routes `/auth/*` around the deployed path is
  insufficient evidence for the new topology.

### Remaining migration

The historical `wyrd-server-ui` Card workspaces (Agent, Prompt, Model,
Service) and page layouts are stale and are not a design source. The approved
canvas (`references/canvas/`) and REQ-021–REQ-031 define every structure and
layout. Historical code is reused only for reads, state handling, and tests
that still match current contracts, and it lands in the REQ-030 component
system, never as a ported page.

New components are built from scratch. Existing components are not
refactored, wrapped, or kept for reference. The old UI is deleted first, as
its own task (REQ-032), before any component or page work starts, so an
implementor never sees two versions of anything.

- **REQ-032 — Delete the old UI first.** One task, before every other UI task
  except TASK-001 (which may run in parallel; see INV-005). Paths are relative
  to `crates/wyrd/wyrd-server/wyrd-ui/`.
  - Delete these files and directories entirely, with their tests:
    - `src/lib/components/` (all of it: `Badge`, `Button`, `Chip`,
      `CodeBlock`, `Disclosure`, `KpiTile`, `ModeProvider`, `Panel`,
      `Select`, `Shell`, `Sidebar`, `StateBlock`, `Table`, `Topbar`,
      `app/`, `charts/`, `component-contracts.test.ts`);
    - `src/lib/features/` (all of it: `cards/`, `changes/`, `observe/`);
    - `src/lib/server/wyrd.ts`, `src/lib/server/mock/`,
      `src/lib/server/changes/`;
    - `src/lib/registry.ts`, `src/lib/views.ts` and their tests;
    - routes `src/routes/styleguide/`, `src/routes/t/[tenantKey]/cards/`,
      `src/routes/t/[tenantKey]/changes/`, `src/routes/t/[tenantKey]/observe/`
      (including `drift`, `genai`, `dashboards`, `logs`, `metrics`, `traces`).
  - Delete the mock-data switch: `mockDataEnabled`, `mockCookie`,
    `mockTenantKey` in `src/lib/server/development.ts`, the
    `WYRD_UI_MOCK_DATA` and `WYRD_UI_MOCK_TENANT` settings, `locals.mockData`
    and `locals.wyrd` in `src/hooks.server.ts` and `src/app.d.ts`. No page may
    show mock domain data after this task.
  - Keep, unchanged in behavior: `src/lib/server/auth/`,
    `src/lib/server/problem.ts`, `src/lib/server/upstream.ts`,
    `src/lib/server/routing/`, `localAuthEnabled` and the login-scenario
    helpers in `development.ts`, `src/hooks.server.ts` (minus the mock and
    `WyrdClient` lines), routes `+layout`, `+page` (tenant chooser),
    `+error`, `login/callback`, `t/[tenantKey]/+layout`,
    `t/[tenantKey]/login/`, `t/[tenantKey]/login/api-key/`,
    `t/[tenantKey]/settings/`, plus `brand/`, `src/app.css`,
    `src/lib/theme.svelte.ts`, and `src/lib/format.ts`.
  - The kept routes import deleted components today (`Panel`, `Badge`,
    `StateBlock`, `TenantChooser`, `ModeProvider`, `app/Shell`). Rewrite them
    as plain Svelte markup styled with the existing brand tokens, with no
    shared component imports. `t/[tenantKey]/+page` becomes a plain signed-in
    page (tenant, principal, sign out) with no domain data. These are
    placeholders replaced by REQ-021 pages later.
  - Done when `pnpm check`, `pnpm test`, and `pnpm build` pass, login works
    with the existing identity journey, and `rg` finds no reference to any
    deleted path.
- `wyrd-server-ui` (historical branch) is not in this tree. Nothing is copied
  from it. A data read (a query or API call) from it may be used as a
  reference when writing a loader (REQ-030) only if it still matches current
  contracts; its components, workspaces, layouts, and types are never copied.
- Final review fails the change if any deleted path, mock, or superseded
  component reappears.

- **REQ-007 — Shell.** The shipped UI shell is the canvas shell (REQ-021):
  left rail, top bar with breadcrumb, time picker, Live toggle, drawer, and
  the rail's "Find or ask" entry that opens the Ask panel. It is built new
  from the canvas `Main` board; nothing from the old shell is kept.
- **REQ-008 — Card inventory.** The Cards page, built from `CardInventory`
  (REQ-030) to the canvas: search, filter, and navigation over current
  authenticated registry reads, with real
  empty, denied, loading, failure, and pagination behavior. Use current native
  kinds, exact identity, metadata labels/annotations, and server-owned status.
- **REQ-009 — Exact Card detail.** Card detail built from `CardHeader`,
  `CardFacts`, `RelationshipList` (REQ-030): version selection,
  relationships, artifacts, and raw spec (YAML tab), using current
  Card/registry/storage contracts.
  Display the exact selected version; links cannot silently switch subject or
  version. Do not create another lineage or hydration owner.
- **REQ-010 — Agent workspace.** Built to the approved canvas `Agent` board
  and REQ-024: registered Prompt relationship, execution route, runtime tool
  names, verification bindings, and attributed telemetry.
- **REQ-011 — Prompt workspace.** Built to the canvas `Prompt` board and
  REQ-024, respecting the current portable/native Prompt forms and derived
  fields.
- **REQ-012 — Model workspace.** Built to the canvas `Model` board and
  REQ-024: declared interface, signature, artifacts, and relationships from
  current contracts.
- **REQ-013 — Service workspace.** Built to the canvas `Service` and
  `ServiceCharts` boards, REQ-024, and REQ-025: components, exact Card
  references, bindings, and server-derived relationships remain
  authoritative.
- **REQ-014 — Query workbench.** The Query page built from `QueryEditor`,
  `TableCatalog`, `QueryDetails`, `RecordTable`, and `Chart` (REQ-030) to the
  canvas `Query` and `QueryChart` boards: catalog, table description,
  parameterized read-only query, results table and chart. Use current Bifrost contracts and terminal-safe query
  semantics, preserve cancellation and bounded rendering, and represent empty,
  denied, failed, and degraded results truthfully.
- **REQ-015 — Traces.** The Traces page (REQ-028; canvas `Main`, `Spans`,
  `TraceDrawer`) built from `Chart`, `FacetRail`, `RecordTable`,
  `TraceWaterfall`, `RecordDetail`, as its own inspectable outcome against current tenant-scoped trace
  data, preserving time bounds, optional Card/Run identity, and exact trace links.
- **REQ-016 — Logs.** The Logs page (canvas `Logs`) built from `Chart`,
  `FacetRail`, `RecordTable`, `RecordDetail`, as its own inspectable outcome with current canonical log fields, bounded/time-scoped reads,
  filtering, and useful payload/empty/error states.
- **REQ-017 — Metrics.** The Metrics page (canvas `Metrics`) built from
  `Chart` and `RecordTable`, as its own inspectable outcome with current canonical metric shapes, time bounds, and
  honest treatment of missing data. Never invent a value from a mock default.
- **REQ-018 — Generic verification.** The Verify page and Verifier Card
  (REQ-026, REQ-027; canvas `Verify`, `VerifySession`, `VerifyDrift`,
  `Verifier`) built from the REQ-030 Verify components on main's generic
  Verifier, binding, execution, requirement, verdict, and evidence
  contracts. Retained
  results come from `vala.verification.results`; `card_uid` is the subject,
  `verifier_card_uid` is the Verifier, and `execution_id` is not application
  `run_id`. Drift is a check with detail; Agent judging is a check. Mock-only
  historical evaluation/drift functions are not a reusable live implementation.
- **REQ-019 — Summaries.** The Home page (canvas `Home`) built from
  `AttentionList`, `SignalBreakdownTable`, `CardInventorySummary`, and
  `StatStrip`, on current data and generic verification. Give each
  distinct summary page its own visual checkpoint; do not bundle all summaries
  merely to reduce task count. A successful query is not proof of complete live
  coverage. Distinguish no data, failed reads, inconclusive judgments, and passes.
- **REQ-020 — References and inspection.** Preserve the canvas assets and
  source inventory in this packet with provenance. The approved canvas
  (`references/canvas/`, PNG and `.dc.html` per board) is each UI task's visual
  target. Every implementation task
  must state its runnable visual checkpoint, required setup, representative
  populated/empty/error states, and desktop/mobile checks in light and dark.
  Record visual acceptance for every component, including components delivered
  together. A gallery checkpoint must cover keyboard/focus behavior, touch
  interaction, long content, and narrow-container overflow where applicable.
  Pause before beginning the next dependent slice until the human has inspected
  the current slice. Every inventory component must be fully implemented, tested,
  and visually accepted on desktop and mobile before the library ships or
  conventional pages begin. Fixture-only progress cannot satisfy that gate.

### UI design decisions — canvas revision (caller, 2026-10-19)

These are caller decisions from the Wyrd Observe redesign canvas review. They
bind every UI task derived from this packet. The canvas illustrates them; it is
not protocol authority.

- **REQ-021 — Left rail shell.** Primary navigation is a persistent left rail:
  Home, Cards, Observe (Traces, Logs, Metrics), Query, Verify, Changes, Repos.
- **REQ-022 — No invented data.** Every number, chart, and list is either a
  query against a named Bifrost table or a value read from a Card spec. Each
  chart offers "Open in Query" with its exact SQL. Without data, the panel shows
  an empty state naming the attribute or binding that would produce it. Wyrd
  emits no `agent.invoke`, `prompt.render`, `model.predict`, or Service-request
  span; the UI must not assume them. Automatic sources on main are: Skald
  `invoke_agent`/`chat`/`execute_tool` spans (server/CLI Agents), the opt-in
  SDK `WyrdAgentsTraceProcessor` (same names), one Gateway span per attempt, and
  `vala.gateway.calls` (latency `terminal_at - started_at`, usage, cost,
  outcome, keyed by `card_uid` when supplied). Spans gain `card_uid`/`run_id`
  inside an SDK Run scope; logs and metrics only when the record carries
  `wyrd.card_uid`. The UI never decides pass, fail, or drift: it renders
  recorded results (`vala.verification.results.verdict`, requirement outcomes,
  `vala.drift.result_features.verdict`); aggregates such as failure rate only
  count recorded results.
- **REQ-023 — Service telemetry correlation (platform prerequisite).** Every
  deployed Service must have one standard way to correlate its spans, logs,
  metrics, and gateway calls. `card_uid` stays the innermost producing Card
  (the record attribute `wyrd.card_uid` projected and scope-checked by
  `RecordCorrelation::extract`). A separate `service_card_uid` column is added
  to `vala.traces.spans`, `vala.logs.records`, `vala.metrics.points`,
  `vala.gateway.calls`, and `vala.verification.results` (written from the
  binding's Service when the result is recorded):
  - source: the OTel resource attribute `wyrd.service.card_uid`
    (`wyrd_spec::vala::trace::attributes::SERVICE_CARD_UID`, declared but not
    yet projected), set once per deployment, e.g.
    `OTEL_RESOURCE_ATTRIBUTES=wyrd.service.card_uid=<uid>`; works in any
    language, with or without the SDK, and through a multi-Service collector;
  - check: the UID must name a Service Card within the principal's Card scope,
    rejected per record exactly as an unauthorized `card_uid` is;
  - default: when absent and the credential is bound to exactly one Service
    Card, Bifrost derives it from that credential.
  Service pages filter on `service_card_uid` and break down by `card_uid`.
  Matching on `service_name` or on a component UID list is not acceptable: a
  shared component would merge two Services' telemetry. This is a Bifrost
  ingest and schema change owned outside the UI tasks; UI tasks depending on it
  wait for it.
- **REQ-024 — Kind-specific Card pages.** Each kind has its own layout and
  only real sources:
  - Agent: `invoke_agent` spans with `card_uid` = the Agent (count, latency,
    `status_code` errors, recent invocations linking to traces); gateway calls
    with `card_uid` = the Agent (tokens, cost); composition from the spec.
  - Prompt: spec only — messages, derived variables, response format, using
    Agents, version diff. No telemetry strip.
  - Model: interface, signature, artifacts; spans/metrics attributed to the
    Card grouped by name, shown only when present; drift from bound Verifiers.
  - Data: schema, `stats`, `splits`, `target_columns`, `sql`; per-feature drift
    from `vala.drift.result_features` where a drift check names it as
    `baseline_ref`.
  - Service: Overview (signals by `service_card_uid`, broken down by component),
    Charts, Versions, YAML.
  - Verifier: the Verifier graph (REQ-027); results live in Verify (REQ-026).
- **REQ-025 — Service charts (platform prerequisite).** `ServiceSpec` gains
  `charts`, next to `tables`, so charts are versioned with the Service,
  reviewed in Change Requests, and authored by agents through registration.
  The UI "Save chart" registers a new Service version. Built-in charts
  (verification, drift, gateway usage, signal counts) use the same definition.
  - Fields: `id`, `title`, `type` (`timeseries | bar | stat | table`), exactly
    one of `sql` (inline) or `sql_file` (path resolved at registration, as
    Verifier code `source` is), `x`, `y`, optional `series`, optional `unit`.
  - Binds: `$1`/`$2` are the range start/end as RFC 3339 strings, `$3` the
    bucket interval chosen by the UI; SQL casts them (`QueryParam` has no
    timestamp). A `timeseries` chart must bound its time column by `$1`/`$2`.
  - Shapes: `timeseries` — `x` one timestamp column, rows ordered by `x`; `y`
    one or more numeric columns; optional `series` one text column, more than
    20 series fold into "other". `bar` — one text category plus numeric
    columns. `stat` — exactly one row, one numeric column. `table` — any shape,
    row-capped.
  - Validation: registration checks the result schema (no execution) against
    the shape rules and rejects with a stable `WyrdError`; render re-checks and
    shows the error in the chart, never a blank chart.
- **REQ-026 — Verify: the results workbench, shaped like Traces.** One list
  of recorded results answers "what is failing"; a result opens in a drawer
  over the list so filters and position survive. No duration, hash, retry,
  provenance, or table-name detail anywhere.
  - List: WHERE bar, time picker and Live; two brushable charts (results
    stacked by recorded verdict; results by Verifier); left filter rail with
    counts (verdict, Verifier, failed requirement, Service, Card, session);
    table Time, Verifier, Card, Service, Run, Session, Failed requirements,
    Verdict. Group by none | run | Card | Service | session. A session group
    header identifies the owner-scoped group and its recorded reviews. Opening
    a review shows that execution's selected recorded result and pinned member
    executions, never every historical row sharing the caller's `group_id`.
  - Result drawer, with "Expand" to a shareable full page:
    - Run result: the Verifier graph (REQ-027) coloured by this result's
      recorded outcomes, first failed requirement selected; under it the
      selected requirement's reason, actual, citations (jump to input or
      trace span), and judge; header verdict, Card, Service, run, session,
      time; Inputs collapsed.
    - Session result: the same graph with that execution's pinned member
      executions as the input column, each opening its own result. Later group
      reviews remain distinct; an application Run is not a Verifier execution.
    - Drift result: one row per feature (recorded score sparkline with the
      recorded threshold dashed, score, threshold, verdict), every feature
      shown, drifted first; window header with previous/next.
  - The Verifier name opens the Verifier Card; a trace citation opens Traces.
- **REQ-027 — Verifier Card: the Verifier graph.** One component built only
  from `VerifierSpec`: inputs, then steps layered by the outputs they read
  and `depends_on` (edges labelled by the output read), step requirements
  under their step, then Verifier-level requirements. The Card shows it
  plain, with description, version, where it runs, "Results in Verify"
  (Verify filtered to this Verifier; the Card has no Results tab), and a
  YAML toggle. The result drawer reuses it coloured by recorded outcomes.
  - Sessions use current main's optional `group_id`, scoped to the tenant and
    authenticated owner Service. `group_end {after}` is a quiet-period trigger,
    not a once-per-session close or permanent finality rule. Later acknowledged
    activity can schedule another review; continuous activity can postpone it.
    The UI uses current `VerifierSpec` source declarations and binding settings.
  - Group view: never merge unrelated owners because their caller group keys
    match. Show recorded reviews within the owner-scoped group, preserving each
    exact Verifier execution. Read the selected review's `GroupSnapshot` through
    existing execution readback to identify its pinned source `execution_ids`;
    do not infer membership from all rows with the same `group_id`. Resolve the
    execution's selected `result_id` before treating a retained result as its
    authoritative judgment; abandoned attempts may also leave retained rows.
  - Group identity, `group_end`, sources, pinned snapshots, and readback already
    ship on current main and require no replacement platform implementation.
    Non-sensitive per-requirement outcome reads remain a separate prerequisite:
    current requirements are inside the sensitive `outcome` payload. The UI
    does not invent new group lifecycle or grant semantics. Pass/fail for every
    requirement, including a drift requirement over
    many features, is the server's recorded verdict in
    `vala.verification.results`; the UI never computes it. The drift view
    stops at recorded scores; no baseline distribution is drawn until one is
    recorded.
- **REQ-028 — Observe pages.** Observe and Verify stay separate: no verdict or
  verification content on Traces, Logs, or Metrics. Traces mode lists one row
  per trace (root span), with span/error counts fetched for visible rows only;
  Spans mode is a flat span list. Live mode pauses while the user scrolls,
  hovers, or has a trace open, and shows "↑ N new". A trace opens in a drawer
  over the list with a waterfall collapsed below two levels and span detail
  beneath (messages for LLM spans). Filters are open by default and writable
  into WHERE, and include Service (`service_card_uid`) and Card (`card_uid`).
  There is no trace-summary table.
- **REQ-029 — Change Requests (design ahead of contract).** A Change Request
  is defined as: repositories connect to a Wyrd GitHub webhook; a repository
  `wyrd.toml` points to a folder of Verifiers (with requirements) to run; a CR
  can be started at any time; a PR creates one if none exists; Verifiers start
  on CR open or when their evidence arrives; users can request additional
  verifications; every update creates a revision; one CR spans many PRs and
  repositories. There are no Claims, Policy gates, or carry-forward. The UI
  uses the existing GitHub-like mock (`features/changes/*`): header, tabs
  (Overview, Verification, Audit, Files changed), intent panel, Subjects
  table, Revision panel (each revision states its cause), conversation feed
  with comments, review submission, and Participants. Override is removed. A
  Verifiers panel lists each Verifier for the current revision with its origin
  (`wyrd.toml` or requested by a user), its start (CR open or awaited evidence
  slot), and status, plus "Request verification". The Verification tab lists
  Verifiers, not Claims. A Repos page manages webhook connections and the
  `wyrd.toml` path. No CR server contract exists on main; live CR pages wait
  for that separately specified server contract and the UI never owns CR state.
- **REQ-030 — The component library and the four UI use cases.** The
  component library is the most important deliverable of this change. Every
  component works in all four UI use cases. "The four UI use cases" means
  exactly these four, wherever this packet uses the phrase; they are not Card
  kinds, workspaces, or personas:

  | # | Use case | Who produces the view | How it is drawn | Who supplies the data |
  |---|---|---|---|---|
  | 1 | Shipped UI (conventional) | Wyrd writes the page | SvelteKit page; data loads run in the BFF | BFF, with the signed-in browser session's credential |
  | 2 | A user builds their own UI with an agent (e.g. Claude) | The agent writes Svelte code using `wyrd-ai-ui`, or writes a view document | The user's own app | The user's server, with the user's own Wyrd credential |
  | 3 | MCP App ("why did this fail? show me") | A Wyrd MCP tool returns a view document | The MCP client hosting the chat (Claude, VS Code, ChatGPT, …) draws `wyrd-ai-ui` inside its sandboxed iframe | The MCP App calls Wyrd's existing read tools as the same MCP caller |
  | 4 | Runtime UI in Wyrd (A2UI-style) | An agent answers a question in the Ask panel | The same view-document renderer, inside the shipped UI | BFF, with the signed-in browser session's credential |

  **Package.** One new npm package, `wyrd-ai-ui`, in
  `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/`, built with
  `svelte-package`, Svelte 5. The shipped app (`wyrd-ui`) consumes it through
  a pnpm workspace and imports nothing domain-specific from anywhere else. It
  is publishable (use case 2) and is the source of the MCP App bundle (use
  case 3). Its entry points:
  - `wyrd-ai-ui` — the components;
  - `wyrd-ai-ui/loaders` — one loader per data component (pure TypeScript, no
    Node or browser APIs);
  - `wyrd-ai-ui/view` — the view-document type, validator, and the `WyrdView`
    renderer component;
  - `wyrd-ai-ui/catalog.json` — the generated catalog;
  - `wyrd-ai-ui/node` — the Node executor (wraps `wyrd-ai`, a peer dependency);
  - `wyrd-ai-ui/mcp` — the MCP App executor (wraps
    `@modelcontextprotocol/ext-apps` `App`).
  Naming: Wyrd's published libraries are named `wyrd-ai`. The npm package
  `wyrd-ai` is the TypeScript SDK (`sdks/wyrd-sdk-ts/wyrd`); the component
  library is the separate npm package `wyrd-ai-ui`.

  Why loaders live in `wyrd-ai-ui`, not in `wyrd-ai`: the npm package
  `wyrd-ai` is Rust compiled into a native Node.js addon (a `.node` file,
  shipped as `wyrd-ai-darwin-arm64`, `wyrd-ai-linux-x64-gnu`,
  `wyrd-ai-linux-arm64-gnu`). A native addon loads only in Node.js. An MCP
  App (use case 3) runs in a browser iframe inside the chat client, where
  Node.js does not exist, so any code that imports `wyrd-ai` cannot run
  there. A loader is therefore split in two:
  - shared request planning, dependent-read sequencing, and result shaping are
    plain TypeScript with no imports from `wyrd-ai`, live in
    `wyrd-ai-ui/loaders`, and run in all four use cases;
  - the part that actually fetches is an executor, one per place the code
    runs: Node.js uses `wyrd-ai` (`wyrd-ai-ui/node`); the MCP App uses MCP
    `tools/call` (`wyrd-ai-ui/mcp`); the shipped UI's browser calls the BFF.
  `wyrd-ai` itself is unchanged and stays the only way Node.js code talks to
  Wyrd. `wyrd-ai-ui` lists `wyrd-ai` as a peer dependency, used only by
  `wyrd-ai-ui/node`.

  **Components.** A component is a Svelte 5 component that receives typed
  props and renders. It never fetches, never reads credentials, never decides
  pass/fail (REQ-022), and never hardcodes span names or column names that
  are not in the current table schemas. Prop types are generated Wyrd wire
  types (`sdks/wyrd-sdk-ts/wyrd/src/card-types.ts` from
  `scripts/gen_ts_types.py`; add the roots a component needs to
  `WIRE_ROOTS`) or rows of the normalized query result below. Domain types
  are never hand-written in the UI. Every data component renders these
  states, each from props alone: `populated`, `empty` (names the attribute,
  binding, or registration that would fill it), `denied`, `error` (shows the
  stable Wyrd error code and title), and `loading`; plus `live` where the
  inventory says so. Styling uses the existing brand tokens (`brand/`,
  `app.css`), light and dark.

  **Loaders.** Each data component has one shared, portable TypeScript loader.
  It plans reads, runs them through the supplied executor, and shapes their
  responses into props. Planning and transformation remain pure; transport
  and credentials stay in the executor. Dependent reads are allowed: a loader
  may use a response to determine its next read. There is no requirement to
  know every request before receiving the first response. Pages contain no
  queries, joins, or dependent-read orchestration; components only render.

  For a verification result addressed by execution ID, the loader reads the
  execution and its selected recorded result, then retrieves the exact Verifier
  Card named by that result for the definition. The existing Card read supplies
  the definition; no aggregate route or expanded execution response is required
  for this case. Other content is read only when its actual consumer needs it.

  Before adding repeated read sequences, inspect the complete existing owner
  workflow. A new or extended route is justified only when a concrete domain
  read demonstrably simplifies the workflow or avoids unnecessary round trips
  and duplicated orchestration. Reuse existing owners, tenant/permission checks,
  and authoritative identities; expose the read consistently to the required
  HTTP/SDK/MCP consumers. Do not create a route per visual component or a new
  orchestration framework. A material public-contract change must be specified
  with its consumer and verification closure before implementation.

  Changing a panel's input or range, removing it, or leaving its host supersedes
  its outstanding load. Stop issuing dependent reads and propagate cancellation
  to active executor work where the transport supports it. Results from a
  superseded load must never overwrite the newer panel, even when cancellation
  cannot stop the remote request. A denied or failed prerequisite read does not
  trigger dependent reads or become an empty success. Private loader function
  signatures and sequencing mechanics remain implementation-owned.
  `WireRequest` is a closed union over the existing domain reads below. Use
  their generated request/identity types rather than introducing approximate
  UI domain contracts. Except for the explicitly required MCP parameter
  projection below, these operations already exist on main on both surfaces:

  | WireRequest | MCP tool | `wyrd-ai` call |
  |---|---|---|
  | `cards.get_by_ref {card_ref: CardRef}` | `cards.get_by_ref` with the exact, space-resolved reference | `Cards.get(card_ref)` |
  | `cards.list ListCardsRequest` | `cards.list` with the flat generated request, including filters, limit and cursor | `Cards.list(request)` |
  | `bifrost.query {sql, params?, deadline_ms?, max_rows?, max_bytes?}` | existing `bifrost.query`, extended with optional typed parameters | `Bifrost.stream(sql, params, options)` with bounded result collection |
  | `bifrost.list_tables {}` | `bifrost.list_tables` | `Bifrost.listTables()` |
  | `bifrost.describe_table {namespace, name}` | `bifrost.describe_table` | `Bifrost.describeTable(namespace, name)` |
  | `verification.get_execution {execution_id}` | `verification.get_execution` | `verify.Execution.connect(execution_id, {client}).status()` |

  Card discovery and server-derived relationships supply exact Card references;
  reading a definition never silently selects a different version. No UID-only
  SDK method, UI-only Card lookup route, or evidence-upload operation is required
  by this union. Existing response-envelope differences are mapped within the
  executor using the existing generated domain types.

  **MCP parameter parity (foundation-owned).** Extend the existing MCP
  `bifrost.query` arguments, advertised input schema, and request projection to
  accept optional `params`, using the shared `QueryParam` contract: JSON null,
  boolean, number, or string. Omitted parameters remain an empty list. Pass the
  ordered values to the existing query service unchanged; do not interpolate
  SQL, create a query endpoint, or add a transport/selector/configuration layer.
  Keep existing authorization, auditing, read-only validation, deadlines,
  cancellation, and row/JSON-byte ceilings. Prove a bound query through both
  SDK and MCP and prove malformed binds and denied queries retain their existing
  refusal behavior. The foundation owns this real MCP/SDK consumer closure;
  TASK-001 must not acquire query or analytical dependencies to provide it.

  A new read the canvas needs (e.g. Change Requests, saved views) is added to
  this union only when its server contract ships.

  **Query results.** Use the existing MCP collected JSON format as the shared
  component-facing shape:
  ```text
  {
    columns: [{name, data_type, nullable}],
    rows: unknown[][],
    terminal: {outcome, query_class, row_count, warnings, source_completion}
  }
  ```
  MCP already returns this projection. The Node executor converts validated
  Arrow batches and their terminal into the same format, retaining row order,
  duplicate column names, nulls, and the existing MCP value-encoding conventions
  for supported types. Positional rows are intentional; do not turn them into
  objects keyed by column name. Do not substitute the full Arrow
  `QueryTerminalFrame`: `arrow_ipc_eos` is framing the executor consumes, not
  component data. Use current wire enums/types for the terminal's domain values.
  Keep bounded collection and the current row/JSON-byte ceilings on both paths;
  exceeding them is an error rather than a successful truncated result.

  Return rows only after the stream's valid terminal. Preserve `success` versus
  `degraded`, its warnings and source-completion evidence; degraded is not
  empty, failed, or silently upgraded to complete success. A failed terminal,
  transport failure, malformed stream, or denied read produces its structured
  error and no usable partial result. Map MCP structured tool errors and SDK
  errors to the same component error/denied behavior without losing the stable
  Wyrd code/title. This is executor adaptation, not another backend result route.
  Prove Node/MCP parity for populated/empty results, duplicate names/nulls,
  supported value encoding, degraded results, failure, denial, and cancellation.
  Loader result shaping consumes only this shared shape.

  **SQL inputs.** Built-in component reads use constant SQL templates with
  typed `$n` bind values. Explicitly authored Chart and Query inputs may supply
  read-only SQL at runtime; that SQL is not required to be a source constant.
  Never interpolate input values into SQL or silently rewrite authored SQL to
  inject a range. Range-aware charts follow REQ-025: `$1`/`$2` are the resolved
  range as RFC 3339 strings, `$3` the bucket interval, with casts in SQL. Resolve
  a relative range once per view load so its panels use the same bounds.
  General Query inputs use their explicitly supplied typed parameters; range
  controls affect them only through the input's declared range binds.
  Preserve the existing server query owner's read-only floor, authorization,
  limits, and terminal semantics. SQL/bind failures or incompatible chart result
  schemas render a panel error, never a blank chart or partial success. Return
  the exact executed SQL and bind values for "Open in Query" to reproduce it.

  **Executors.** An executor runs `WireRequest[]` and returns responses; it is
  the only code that touches a credential or transport. A denied, failed, or
  invalid partial response becomes the component's `denied` or `error` state,
  never an empty or mock success. A valid degraded query remains a degraded
  result with its warnings, following the query-results contract above.
  - Node executor (`wyrd-ai-ui/node`): built from a `wyrd-ai` `WyrdClient`. Use
    case 1 builds it in the BFF from the browser session's credential; use
    case 2 builds it in the user's server from their credential
    (`WYRD_API_KEY`, workload token, or saved login). If `wyrd-ai` cannot be
    constructed from a per-request browser-session bearer, adding that to
    `wyrd-ai` is part of the foundation task.
  - MCP executor (`wyrd-ai-ui/mcp`): calls the MCP tools above through the
    ext-apps `App.callServerTool`, so reads run as the MCP caller with that
    caller's scopes. It makes no network request of its own.
  - In the shipped UI's browser (Ask panel, gallery), the renderer uses a BFF
    endpoint `POST /t/[tenantKey]/ui/load` that takes `{component, input}`,
    runs the loader with the Node executor, and returns props. Credentials
    never reach the browser.

  **Component actions.** Interactive components emit typed, JSON-safe action
  intents to their host. They do not execute mutations or acquire credentials.
  Each catalog component declares its emitted action names, input schemas,
  and observable completion behavior alongside its input schema. A view
  document cannot supply arbitrary handlers, tool names, URLs, or executable
  code. Referenced resources are identifiers, never caller-selected authority.

  The host validates the intent and dispatches it through the owning server
  capability as the current viewer, preserving that API's authorization,
  audit, idempotency, and error behavior. Shipped and custom apps use their
  credential-bearing server boundaries and existing SDK/API owners. The MCP
  App requests only the declared action's supported MCP tool through the same
  authenticated caller connection, with its required write scope. Missing
  server/SDK/MCP action projections are explicit prerequisites, not license to
  introduce frontend-owned durable behavior or bypass another host's boundary.
  The shared read executor remains read-only; no generic write dispatcher or
  replacement transport framework is required by this contract.

  The host supplies pending, success, denied, and error state back to the
  component. While pending, prevent duplicate submission of that action; do not
  automatically retry a mutation unless its existing API makes that safe.
  Permission refusal and stable server errors remain visible. When a host or
  missing platform prerequisite cannot perform an action, show it unavailable
  with a reason. Such a fixture or unavailable action does not satisfy the
  component's production-complete shipping gate. Component action tests cover
  emitted input, host dispatch, denied/error outcomes, and successful completion
  on each applicable host without putting credentials into props or documents.

  **Catalog.** `catalog.json` is generated from the components and checked
  for drift by `mise run codegen:check`; it is never hand-edited. Each
  component's input is declared once, next to the component, as a Zod 4
  schema; the generator emits it as JSON Schema with `z.toJSONSchema`. Shape:
  `{catalogId: "wyrd.ui/v1", components: {<Name>: {description, input:
  <JSON Schema>, states: [...], emits: [...]}}}`. Shell-only components
  (rail, top bar, time picker, drawer) are not in the catalog.

  **View document.** The format every agent and host shares:
  ```json
  {
    "version": "wyrd.view/v1",
    "catalogId": "wyrd.ui/v1",
    "title": "Why refund answers failed on Tuesday",
    "range": {"from": "2026-10-14T00:00:00Z", "to": "2026-10-15T00:00:00Z"},
    "panels": [
      {"id": "p1", "component": "Chart", "width": "full",
       "input": {"type": "timeseries", "sql": "SELECT …", "x": "t", "y": ["failed"]}},
      {"id": "p2", "component": "VerificationResult", "width": "full",
       "input": {"resultId": "…"}}
    ]
  }
  ```
  - `range` is either absolute `{from, to}` or relative `{last: "6h"}`
    (REQ-031). Panels inherit it unless their input sets one.
  - `width` is `full` or `half`. Panels render in order. There is no other
    layout language.
  - A view document never contains query results, props, credentials, or
    tokens. It is re-run as whoever views it (REQ-031).
  - Validation: `version` and `catalogId` match; every `component` exists in
    the catalog; every `input` validates against that component's schema.
    An invalid panel renders an error naming the component and the failed
    field; the other panels still render.
  - `WyrdView` (`wyrd-ai-ui/view`) takes `{doc, executor}`, validates, runs each
    panel's loader, and renders each panel through all its states. A brush on
    a time-series panel updates the document's `range` for every panel.
  - A2UI export: a view document maps to A2UI v0.9 `createSurface`
    (`catalogId`) plus `updateComponents` (one component per panel). It is
    built only when a real third-party host needs it; nothing in the format
    may block that mapping.

  **Use case 1 — shipped UI.** Each `+page.server.ts` builds the Node executor
  from the session, runs the loaders for its components, and returns props
  (streamed where useful). Pages compose `wyrd-ai-ui` components directly; they
  contain layout only, no queries, joins, or data shaping. Shell components
  (REQ-007) live in the app, not the package.

  **Use case 2 — a user's own UI.** Wyrd ships:
  - `wyrd-ai-ui` on npm with the catalog, loaders, executors, and `WyrdView`;
  - a read-only MCP tool `ui.catalog` returning `catalog.json`, so an agent
    building the app knows every component and its input;
  - one runnable example app under `examples/` (SvelteKit, server-side Node
    executor with `WYRD_API_KEY`, one page using components directly and one
    rendering a view document).
  A browser-only app cannot run the Node executor; it needs a server, or it
  uses the MCP App route.

  **Use case 3 — MCP App.** On the existing `/mcp` endpoint
  (`crates/wyrd/wyrd-server/src/mcp/`):
  - enable the MCP resources capability and serve one resource,
    `ui://wyrd/view`, mime type `text/html;profile=mcp-app`: a single
    self-contained HTML file built from `wyrd-ai-ui` (renderer, every
    component, catalog, MCP executor). `scripts/build.sh` builds it; the
    server embeds it at compile time.
  - add a read tool `ui.show_view` whose input is a view document. The server
    validates it against the embedded `catalog.json` (the `jsonschema`
    workspace dependency `wyrd-server` already uses) and rejects an invalid
    document with a stable `WyrdError`. On success it returns the document as
    `structuredContent`, and the tool declares
    `_meta.ui.resourceUri = "ui://wyrd/view"`.
  - the app receives the document through
    `ui/notifications/tool-result`, loads every panel with the MCP executor
    (`tools/call` to `cards.*`, `bifrost.*`, `verification.*` as the same
    caller), and themes itself from `hostContext`. It needs no CSP
    `connectDomains`. Display modes: inline and fullscreen.
  - spec: MCP Apps 2026-01-26 (`modelcontextprotocol/ext-apps`).

  **Use case 4 — Ask panel.** The rail's "Find or ask" opens the Ask panel on
  every page. The user's question goes to the BFF, which calls a server Ask
  endpoint that returns a text answer plus a view document; `WyrdView`
  renders it through the BFF load endpoint; "Save" stores it as a saved view
  (REQ-031). No Ask/agent endpoint exists on main (the Gateway's
  `/v1/chat/completions` is model access, not an agent); the Ask endpoint is
  a platform prerequisite specified separately. Until it ships, the Ask panel
  is built and tested against fixture answers.

  **Component inventory.** These are the components the approved canvas
  needs. "Prereq" means the data source is a platform prerequisite not yet
  on main; the component is still built and tested against fixtures shaped
  by that prerequisite's contract, and only its live loader waits.

  | Component | Canvas boards | Input | Data source | Prereq |
  |---|---|---|---|---|
  | `Chart` (timeseries, bar, stat, table; title, source caption, Open in Query, in-chart error) | Main, Spans, Logs, Metrics, Verify*, ServiceCharts, QueryChart, Model, Data | chart definition (REQ-025 fields) + range + bucket | its own SQL on `vala.*` tables | Service charts: REQ-025 |
  | `StatStrip` (1–5 numbers with sparkline) | Agent, Service, Home, Main, Spans | stat definitions + range + scope (`card_uid` or `service_card_uid`) | `vala.traces.spans` (`invoke_agent`, `duration_nano`, `status_code`), `vala.gateway.calls` (`usage`, `cost`) | `service_card_uid`: REQ-023 |
  | `FacetRail` (value counts, click to add to WHERE) | Main, Spans, Logs, Verify* | `{signal, where, range, facets}` | GROUP BY counts on the list's table under the same WHERE | session facet: `group_id` |
  | `RecordTable` (typed cells, live mode, group by) | Main, Spans, Logs, Verify*, Agent, Query | `{signal \| sql, where, range, columns, groupBy, live, limit}` | spans (one row per root span), `vala.logs.records`, `vala.verification.results`, or SQL | session grouping: `group_id` |
  | `TraceWaterfall` | TraceDrawer | `{traceId, range}` | `vala.traces.spans` by `trace_id` | — |
  | `RecordDetail` (span or log: Messages, Attributes, Events, Exception, Raw) | TraceDrawer, Logs | `{traceId, spanId}` or log key | span `attributes`, `events`, `exception_*`, `gen_ai_*`; log `body`, `attributes` | — |
  | `CardHeader` (name, kind, version select, status, uid copy, tabs, primary action) | all Card boards | `{cardRef}` | Card envelope + registry version list | — |
  | `RelationshipList` | Agent, Prompt, Model, Data, Service | `{cardRef}` | server-derived `relationships` | — |
  | `CardFacts` (key/value) | all Card boards | `{cardRef, fields}` | Card `metadata` and `spec` | — |
  | `CompositionGraph` | Agent, Service | `{cardRef}` | `AgentSpec` / `ServiceSpec`; node badge = latest recorded verdict | — |
  | `PromptMessages`, `PromptVariables`, `UsedBy`, `VersionDiff` | Prompt | `{cardRef, otherVersion?}` | `PromptSpec`; relationships | — |
  | `ModelInterface`, `ArtifactTable` | Model | `{cardRef}` | `ModelSpec` interface/signature; artifacts | — |
  | `VerificationBindings` | Agent, Service, Home | `{cardRef, range}` | bindings + `vala.verification.results` | — |
  | `AttentionList` | Home, Service | `{scope, range}` | failed `vala.verification.results` by `owner_card_uid` | CR rows: REQ-029 |
  | `SignalBreakdownTable` | Home, Service | `{groupBy: service_card_uid \| card_uid, scope, range}` | root spans + `vala.gateway.calls` | REQ-023 |
  | `CardInventory` (list, filters, kinds) and `CardInventorySummary` | Cards, Home | `{filters, page}` / `{}` | `cards.list` | — |
  | `VerifierGraph` (plain or coloured by one result) | Verifier, Verify, VerifySession, VerifyDrift | `{cardRef}` or `{resultId}` | `VerifierSpec`; `outcome` of `vala.verification.results` | — |
  | `VerificationResult` (drawer body: header, `VerifierGraph`, `RequirementDetail`, Inputs) | Verify, VerifySession, VerifyDrift | `{resultId}` | `vala.verification.results`; drift adds `vala.drift.result_features`; existing execution readback supplies pinned group membership | main already supplies `group_id`/`group_end`; non-sensitive requirement outcomes remain a prerequisite |
  | `RequirementDetail` | Verify*, ChangeVerification | `{resultId, requirementId}` | requirement outcome in `outcome` | — |
  | `DriftFeatureTable` | VerifyDrift, Data | `{resultId}` or `{cardRef}` | `vala.drift.result_features` | — |
  | `QueryEditor`, `TableCatalog`, `QueryDetails` | Query, QueryChart | `{sql, range, limit}` / `{}` | `bifrost.*` | — |
  | `ChartEditor` | ServiceCharts | `{cardRef, chartId?}` | `ServiceSpec.charts` | REQ-025 |
  | `ChangeRequestList`, `ChangeHeader`, `SubjectsTable`, `RevisionList`, `ActivityFeed`, `ChangeVerifierList`, `RequestVerificationForm`, `ReviewSubmission`, `Participants` | Changes, ChangeDetail, ChangeVerification | `{crId, revision?}` | none on main | REQ-029 |
  | `RepositoryList`, `WyrdTomlView`, `VerifierFolderTable` | Repos | `{repo}` | none on main | REQ-029 |
  | `AskPanel` | rail entry (all pages) | `{question}` | Ask endpoint | Ask endpoint |

  (*Verify means Verify, VerifySession, VerifyDrift.) Primitives used inside
  these, also in `wyrd-ai-ui` but not in the catalog: `EmptyState`,
  `CardRefChip`, `VerdictPill`, `StatusPill`, `Sparkline`, `DurationBar`,
  `Drawer`. Shell components in the app, not the package: `LeftRail`,
  `TopBar`, `TimePicker`, `LiveToggle`, `WhereBar`, `GroupBySwitch`,
  `CardTabs`.

  Canvas corrections the components must apply (the canvas is illustrative):
  span names come from data, never constants (the canvas shows
  `agent.invoke`, `llm.chat`; Wyrd emits `invoke_agent`, `chat`,
  `execute_tool`); columns are the real schema (`severity_number` /
  `severity_text`, not `level`; `duration_nano`, not `duration`); Observe
  tabs are Traces, Logs, Metrics only; every chart link reads "Open in
  Query".

  **Tests.** Every component, before any page uses it:
  - T1 states: rendered with Vitest and Testing Library from fixture props in
    every state it lists, in light and dark, at desktop and mobile widths.
  - T2 loader: assert actual requests and dependent stages from recorded
    response fixtures, including exact built-in SQL/binds, unchanged authored
    SQL, and the resulting props. Prove denied/failed prerequisites issue no
    dependent reads and superseded responses cannot overwrite newer state.
  - T3 view document: a fixture document naming the component validates
    against `catalog.json` and renders through `WyrdView` with a fixture
    executor; an invalid input renders the panel error.
  - T4 MCP App: the built `ui://wyrd/view` HTML loads in the ext-apps
    `app-bridge` test host with a fixture tool result and fixture
    `tools/call` responses, and renders the panel.
  - T5 live (gated journey lane): the Node executor against `WyrdTestServer`
    and repository Postgres with seeded data renders `populated`, `empty`,
    and, with an under-privileged token, `denied` — never another tenant's
    data. Prereq components defer T5 until their prerequisite ships; they remain
    fixture-ready, not production-complete, and hold the full-library shipping
    gate until their real loading/actions and applicable T5 proof pass.
  - T6 page journey: added when the page is assembled.
  Fixtures for `populated` and `empty` are recorded from T5 runs by a script
  and checked in; `error` and prereq fixtures are hand-written and labeled.
  No test calls a live model.

  **Gallery (visualized).** A dev-only route `/_gallery` in the shipped app
  renders every catalog component in every state, light and dark, through
  `WyrdView` from the fixture view documents, each labeled with the canvas
  board it must match. The human inspects every component against
  `references/canvas/png/` on desktop and mobile, in light and dark, before
  pages are built. Record component-level acceptance with the task evidence;
  accepting a family does not waive inspection of its individual components.
  Automated checks supplement human visual inspection rather than replace it.

  **Build order (caller, 2026-10-19).**
  1. REQ-032 deletes the old UI. TASK-001 (packaging, install, launch, login)
     runs in parallel and is independent of the component library.
  2. Foundation: `wyrd-ai-ui` package and workspace, generated prop types,
     view-document schema and validator, `WyrdView`, catalog generator and
     `codegen:check` drift check, Node executor, MCP executor, BFF load
     endpoint, `/_gallery`, and the T1–T5 harness, proven end to end with
     `Chart` and `CardHeader`.
  3. The full component library, every component in the inventory with its
     loader, catalog entry, fixtures, T1–T5, and gallery entries, in groups
     with a human gallery checkpoint after each: primitives; Observe
     (`Chart`, `StatStrip`, `FacetRail`, `RecordTable`, `TraceWaterfall`,
     `RecordDetail`); Card (`CardHeader`, `RelationshipList`, `CardFacts`,
     `CompositionGraph`, Prompt, Model, `CardInventory`); Query; Verify
     (`VerifierGraph`, `VerificationResult`, `RequirementDetail`,
     `DriftFeatureTable`, `VerificationBindings`, `AttentionList`); Service
     (`SignalBreakdownTable`, `ChartEditor`); Changes and Repos; `AskPanel`.
  4. MCP App: resources capability, `ui://wyrd/view`, `ui.show_view`,
     `ui.catalog`.
  5. Only then the conventional pages (REQ-007 to REQ-019, REQ-021,
     REQ-024 to REQ-029), each assembled from finished components with its
     T6 journey. A page whose prerequisite has not shipped waits for it.

  The complete self-contained browser artifact needed for T4 is built during
  foundation/library verification against the fixture host. Step 4 registers
  and proves the production MCP resource/tools; it does not first create the
  artifact that earlier component proofs require. Before task handoff, use
  `$plan-readiness-reviewer` on the proposed task packet in place of the former
  pedantic-maintainer review, as requested by the caller.

  **Decisions (caller, 2026-10-19).**
  - Non-Svelte user apps: `wyrd-ai-ui/view` exports
    `mountView(element, {doc, executor})`, which uses Svelte's `mount()` to
    draw a view document into any DOM element, so React or Vue apps render
    view documents without writing Svelte. Using individual components
    directly is Svelte-only. No Web Components build.
  - Verify's Service filter and column read `service_card_uid` from
    `vala.verification.results`, written by the server when the result is
    recorded (the binding's Service). This adds `vala.verification.results`
    to REQ-023's column list. Service is never derived at read time from
    relationships.
  - Data per-column drift: the loader reads the Verifier specs whose drift
    check's `baseline_ref` names this Data Card, collecting each Verifier and
    requirement id, then queries `vala.drift.result_features` filtered by
    that Verifier and requirement id. No schema change.

- **REQ-031 — Saved views (platform prerequisite).** A saved view is a view
  document (REQ-030), never stored data. It is a tenant-scoped server resource,
  not a Card kind, readable and writable through HTTP and MCP so agents and every
  UI host share it. Each has an owner principal and visibility `private` or
  `tenant`; it is neither versioned nor reviewed. Opening it re-runs every panel
  live as the viewer, never as the author, so sharing never widens data access.
  An absolute time range keeps an answer ("why did it fail Tuesday"); a relative
  range makes a dashboard. When retention removes the underlying data, the view
  says so instead of storing data; durable evidence belongs to Verification. A
  chart that must be governed with its Service is promoted into
  `ServiceSpec.charts` (REQ-025), where it is versioned and reviewed in Change
  Requests.

## Invariants, constraints, and non-goals

- **INV-001 — Current contracts.** Current `wyrd-spec` and architecture own
  vocabulary and behavior. Fourteen registrable native Card kinds; no Workflow,
  Eval, Tool, Skill, SubAgent, Bifrost, or External registration. Verifier is the
  generic verification Card; drift belongs within its check model. No legacy routes,
  compatibility aliases, per-kind verification models, or replacement contracts.
- **INV-002 — Server and credential boundary.** Rust retains durable behavior,
  authorization, tenant isolation, storage, policy, and audit. BFF credentials
  stay server-side; never emit token/API-key material in page data, browser
  storage, logs, URLs, screenshots, or fixtures. A failed or denied live request
  never falls back to mock success or another tenant's data.
- **INV-003 — One build owner.** `scripts/build.sh` remains the sole authoritative
  build implementation. Existing mise and CI owners may invoke it; do not
  duplicate build logic in another script/framework. Native Darwin artifacts
  and Linux OCI images are separate outputs. Native macOS support does not
  require building Linux images on macOS. macOS Intel is explicitly excluded.
  The caller's 2026-10-19 correction supersedes the older all-four-target rule
  for this change. Implementation must reconcile the owning platform/release
  documentation with the three native targets and three Linux image variants;
  it must not add an Intel macOS installer target or release job.
- **INV-004 — Small integration.** Extend existing owners. No speculative
  transport, client facade, cache, environment controller, shared framework,
  replacement test selector, or exact-count guard framework. No dependency just
  to select tests. No new environment variable solely for tests, including
  `WYRD_IDENTITY_TARGET`, `WYRD_IDENTITY_FILTER`, or `WYRD_STARTUP_FILTER`.
  Existing legitimate settings may be reused only for demonstrated consumers.
- **INV-005 — TASK-001 exclusions.** TASK-001 ends at packaged/installed/launched
  UI, official container routing, lifecycle, and real existing login proof.
  It must not onboard Cards, Bifrost, Query, Observe, verification results,
  generated HTTP clients, or analytical native addons. It proves login and
  session with the authenticated surfaces REQ-032 keeps (`t/[tenantKey]/`
  signed-in page and `settings`). It may land before or after REQ-032; if
  before, REQ-032 must keep its login proof passing. Do not install a domain
  client to prove auth.
- **INV-006 — Useful migration only.** No whole-branch merge, redesign of Wyrd,
  new Workflow Card, general mock system, or new feature inferred solely from a
  canvas. The Data workspace and Change Request UI are caller-requested
  (REQ-024, REQ-029); Change Requests become live only with their server
  contract. Mock-only evaluation functions are reference material, not promised
  live capabilities. Generated
  projections remain source-driven. Ordinary private implementation choices
  remain the implementer's responsibility.
- **INV-007 — Real proof and visual size.** Reuse existing verification owners,
  fixtures, manifests, and setup. Add behavioral scenarios through TDD; use
  static/regression proof for unchanged/static obligations. No fake RED, weakened
  assertions, test-only production configuration, or invented test selector.
  Agent, Prompt, Model, traces, logs, metrics, and distinct summary pages cannot
  be combined into a large visual slice. Lower-level tests supplement real
  deployed journeys.

## Decisions and ownership

1. Existing SvelteKit adapter-node BFF with system Node for native use;
   container-provided runtime for OCI. UI build contents are part of verified
   installation and release identity. Preserve the installer signature boundary.
2. Separate native loopback Node/Rust listeners, with same-origin browser auth
   forwarding in the BFF. Official containers use the established nginx owner
   for public routing. Do not weaken exact-origin OAuth validation or introduce
   nginx as a new native prerequisite.
3. The existing installer, `DevServer`, build script, release workflow, official
   Dockerfiles/entrypoint/nginx, and auth/session owners retain their behavior.
   Extend them for the actual UI consumer; no second lifecycle controller.
4. Main's existing production identity journey is reused and extended where its
   synthetic `/auth/*` routing currently bypasses production routing. Native
   installation/lifecycle and Linux image routing have separate proof owners.
5. UI domain reads remain BFF-side against current public server contracts.
   The first actual consumer owns the smallest necessary client integration and
   its consumer closure; later pages reuse it. Historical SQL/projections must
   be revalidated against current table schemas and query terminal semantics.
6. Plan real dependency edges and one observable outcome per task. TASK-001 is
   the caller-required plumbing/login closure. Later domain work is not a
   dependency of it. Visual approval governs dependent execution, not a new
   persisted change-state controller.

## Acceptance and evidence classes

| ID | Observable acceptance | Credible evidence |
|---|---|---|
| AC-001 | A verified native install launches the production BFF and Rust server without a source checkout on each supported OS/architecture; missing/incompatible Node and malformed packages fail usefully and preserve a working install | Existing build/release and installer owners, package contents/runtime smoke, native release matrix evidence; no Node executable bundled |
| AC-002 | `wyrd server dev` reaches UI readiness and handles first/repeat runs, conflict, partial startup, interruption, and child death without abandoned owned processes or data loss | Existing local-dev journey/installed CLI owners, focused process regressions, public UI smoke |
| AC-003 | Existing tenant and platform login, callback, session/refresh and logout work at native browser origin; origin/state/issuer/tenant failures remain refused and no secret reaches the browser | Repository Keycloak/Dex and production-BFF identity owners extended through the actual public routing |
| AC-004 | Official images serve UI/BFF and existing Rust HTTP/gRPC surfaces through nginx at the intended public origin, with correct readiness, errors, and three-process cleanup | Existing Linux startup dev/prod owners against pinned image ID and source revision; production identity exercise through nginx |
| AC-005 | TASK-001 imports no deferred domain/client/native analytical integration and adds no prohibited build/test configuration | Actual diff/manifest review and relevant existing dependency checks |
| AC-006 | Card inventory and exact detail show current tenant data, exact versions, correct links and current kinds with honest empty/denied/error states | Focused BFF/UI tests and real registry journey plus separate inventory/detail visual checkpoints |
| AC-007 | Agent, Prompt, Model, and Service workspaces faithfully project their current specs and relationships | One focused consumer journey and human light/dark inspection per workspace |
| AC-008 | Query, traces, logs, and metrics each read current canonical data, respect authorization/time bounds, and do not present failed/partial reads as success | Existing Bifrost/identity fixtures and narrow UI consumer journeys, terminal/cancellation regressions, separate page inspections |
| AC-009 | Verification screens show generic requirements/verdicts/evidence and correct subject/Verifier/execution identities; summaries agree with those retained facts | Real generic verification-to-UI journey and focused projection tests; separate verification and summary checkpoints |
| AC-010 | All original draft/reference bytes remain available with provenance; every task is self-contained, source-backed, narrowly verified, and visually inspectable; independent task readiness findings are resolved or surfaced | Preservation hashes, evidence inventory, obligation/task map and independent plan-readiness-reviewer findings/dispositions |
| AC-011 | Every inventory component is fully implemented, tested, and individually accepted on desktop/mobile in light/dark before library shipment and conventional page work; unmet platform prerequisites block completion rather than disappear behind fixtures | Per-component interaction/accessibility/state evidence, applicable live authorization journeys, and recorded human gallery acceptance |

Task planning must choose exact focused commands from inspected current owners;
the evidence document identifies known commands and coverage gaps. No build,
test, or runtime success is claimed by this draft. Final integrated review owns
combined proof and cross-task seams; task gates remain bounded to their owners.

## Open material decisions and approval

The independent [readiness review](review/plan-readiness/review.md) assessed
revision 5 against refreshed main and returned `Revise`. Its exact reviewed
revision and source identity are preserved beside the report. Revision 6 addresses
the accepted desktop/mobile completion gate and replaces stale grouped
verification assumptions with current main's recorded contracts. It also records
the requested reviewer change. Remaining material corrections before executable
task derivation:

- PRR-001: upload planning is removed; the caller accepts optional shared typed
  parameters on the existing MCP query tool as foundation-owned work. Exact
  Card references and existing generated read requests replace the approximate
  mappings. Node normalizes its results to the existing MCP collected JSON
  shape, including degraded/error behavior, as specified in REQ-030.
- PRR-002: caller accepts dependent reads through the shared loader, subject to
  existing-owner reuse and demonstrated simplification before adding a route;
  cancellation/supersession behavior is now specified in REQ-030.
- PRR-003: caller accepts typed component action intents, authorized host
  execution, returned action state, and explicit unavailable-action behavior;
  unresolved platform action contracts remain prerequisites.
- PRR-006: caller accepts the distinction between built-in templates and authored
  read-only SQL, with explicit typed parameters, chart range/bucket binds, and
  existing server validation; REQ-030 now records these rules.

PRR-001 through PRR-006 have proposed corrections above; the original review is
not rewritten into a pass. Rerun `$plan-readiness-reviewer` on revision 9 before
claiming specification readiness. Missing separately specified platform contracts
remain real component dependencies and hold the full-library shipping gate;
they must not be invented by the UI implementer. Proposed task review also uses
`$plan-readiness-reviewer`.
Native origin topology and ordinary fixture/private implementation mechanics
remain unchanged. Resolve the material corrections and obtain explicit approval
of revision 9 before `$wyrd-plan` creates the replacement tasks. Approval of
planning does not authorize implementation.

## Revision history and authorities

- Revision 1: invalid prior draft; archived byte-for-byte, including its invalid
  approval claim. Not implementation authority.
- Revision 2 (2026-10-19): rebuilt from committed main, historical implementation,
  actual OpsML code, preserved canvases, and caller's scope; unapproved draft,
  superseded by revision 3. The earlier revision-2 approval question is obsolete.
- Revision 3 (2026-10-19): caller correction excludes Intel macOS, fixes native
  targets to Apple Silicon macOS and x86_64/aarch64 Linux, fixes Linux images to
  Debian/Rocky on both architectures, and explicitly excludes Eval Cards.
  Unapproved draft superseded by revision 4.
- Revision 4 (2026-10-19): caller clarifies Rocky is x86_64-only; Rocky aarch64
  is neither required nor a migration gap. Debian remains x86_64/aarch64.
  Superseded by revision 5.
- Revision 5 (2026-10-19): adds caller UI design decisions REQ-021–REQ-031
  from the canvas review: left rail, no invented data, `service_card_uid`
  correlation, kind-specific Card pages, `ServiceSpec.charts`, one UI for four
  hosts (REQ-030), saved views (REQ-031), Verifier views
  from `VerifierSpec`, Observe/Verify separation, and Change Requests per the
  caller definition and existing mock. Amends INV-006. Verifier UI restated
  after independent research: Verify list plus result drawer, one Verifier
  graph, no Card Results tab. Approved by the caller 2026-10-19. Amended the
  same day with the caller's REQ-030 build order: the full reusable component
  system is built, tested, and visualized before any conventional page;
  REQ-030 names the four UI use cases; historical workspaces are stale and
  REQ-010–REQ-013 follow the canvas. REQ-030 expanded into the full
  component-library specification (package, loaders, executors, catalog,
  view document, each use case, inventory, tests, gallery, build order);
  REQ-032 deletes the old UI first; REQ-007 and INV-005 amended to match.
- Revision 6 (2026-10-19): draft after caller-authorized fast-forward to main
  `8651b96fd`; records shipped `group_id`/`group_end`/snapshot semantics,
  per-component desktop/mobile completion and shipping gate, and replaces the
  pedantic-maintainer gate with plan-readiness-reviewer. Independent revision-5
  spec review is retained; unresolved read/loader/action/SQL contracts remain
  explicit before task derivation.
- Revision 7 (2026-10-19): removes `verification.evidence` upload planning from
  the UI read union per caller instruction; PRR-001 now retains only the MCP
  parameterized-query issue. Draft pending the remaining contract corrections.
- Revision 8 (2026-10-19): records caller acceptance of dependent loader reads
  unless an evidenced route simplification is justified, typed component action
  intents handled by authorized hosts, and distinct built-in/authored SQL rules.
  No aggregate verification route is prescribed. Draft pending remaining read
  parity/normalization closure and revision approval.
- Revision 9 (2026-10-19): caller accepts extending the existing MCP query tool
  with shared typed parameters as foundation work, adapting Node Arrow output
  to the existing MCP collected JSON format, and using exact CardRef/current
  read mappings instead of adding a UID-only SDK surface. Specifies matching
  degraded/error/limit behavior and requires independent spec readiness review.

Authorities: `AGENTS.md`; `architecture/agent-rules.md`;
`architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`;
`architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`;
`architecture/operations/deployment-and-release.md`;
`architecture/references/languages/spec-driven-development.md`;
`architecture/references/languages/testing-workflows.md`; repository `wyrd-ui`
guidance for styling. These are repository-relative authority paths, not copies
of skill instructions.
