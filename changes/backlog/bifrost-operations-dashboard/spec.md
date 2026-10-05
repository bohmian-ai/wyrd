---
id: SPEC-bifrost-operations-dashboard
revision: 5
status: draft
---

# Bifrost operations dashboard

## Objective

A deployment operator can open the Wyrd UI and see whether Bifrost is keeping
up: writes arriving and becoming durable, data leaving memory and staging,
Forge keeping pace, and queries answered promptly, with where they wait or
fail. The same health facts are available as one machine-readable call so an
agent or script does not scrape and interpret Prometheus text.

The dashboard reads the measurement contract fixed by
`changes/completed/2026/bifrost-scribe-live-reads.md` (the six operator
questions and their production families). It does not create new telemetry,
a metrics store, or a dashboard builder. This backlog draft does not authorize
implementation.

## Users and value

- **Deployment operator** (self-hosted, enterprise cloud, SaaS platform team):
  diagnoses Bifrost backlog and latency from the product UI without first
  installing Grafana.
- **Operator agent**: reads one typed health response and decides whether to
  page, wait, or scale.

Tenant users are not the audience. Bifrost metrics are pod-wide aggregates; in
a multi-tenant deployment they describe every tenant's load.

## Required behavior

### Plane and access

- **REQ-001 — Platform plane only.** Every surface in this specification is a
  platform-plane surface under `/platform/*`, authorized only for platform
  principals per the two-administration-planes rule in
  `architecture/wyrd-design.md`. A tenant credential or session is refused with
  the existing platform-plane refusal, and no tenant route, tenant navigation
  entry, or tenant MCP tool exposes these facts.
- **REQ-002 — Audited reads.** Each authorization decision for these reads is
  recorded through the canonical audit path, like other platform-plane
  decisions.

### Durable health summary

- **REQ-003 — One call.** `GET /platform/bifrost/health` returns one typed
  response describing the whole deployment, independent of which replica
  serves it and of whether Prometheus is configured.
- **REQ-004 — Durable facts only.** The summary reports only facts with a
  cluster-wide durable owner:
  - live members and the roles each serves, from cluster membership;
  - Scribe staged backlog: unpublished staged members, bytes, and oldest
    ready time, from durable staging state;
  - Forge pending tasks and oldest pending age per task type, and
    outstanding planning demands, from Forge task and demand state;
  - the observation time at which the server read them.

  Process-local values (Oracle queue depth, active queries, memtable bytes,
  request rates) are not in the summary, because one replica cannot state them
  for the deployment.
- **REQ-016 — Facts, not verdicts.** The summary reports counts, bytes, and
  ages only. It returns no healthy/degraded judgment and accepts no threshold
  configuration; the UI presents the facts without inferring a health state.
- **REQ-005 — Honest failure.** If a durable owner cannot be read, the
  response names the unavailable section and its stable error code; it never
  substitutes zero for an unread backlog.
- **REQ-006 — Bounded cost.** One summary costs a fixed, small number of
  aggregate reads that does not grow with tenants, tables, files, or tasks.

### Time-series panels

- **REQ-007 — Optional Prometheus.** A deployment may configure one Prometheus
  query endpoint for the platform plane through server configuration. Its
  connection, including any credential, is deployment configuration resolved
  server-side, never a tenant `Source` Card and never sent to the browser.
- **REQ-008 — Closed panel catalog.** The server owns a closed catalog of
  dashboard panels, one group per TASK-005 operator question. Each panel is a
  fixed PromQL expression over the published Bifrost families, aggregated
  across all scraped pods (histograms summed by bucket before a quantile),
  with an optional per-pod breakdown. Callers choose a panel, time range, and
  step within server bounds; they never submit PromQL.
- **REQ-009 — Panel API.** `GET /platform/bifrost/panels` lists the catalog
  with each panel's question, unit, and meaning.
  `GET /platform/bifrost/panels/{panel}` returns its series for a bounded
  range. Upstream timeout, refusal, or absence returns a stable error, never
  an empty series presented as healthy.
- **REQ-010 — Replica-local fallback.** Without a configured Prometheus, the
  panel endpoints return the serving replica's current registry values for
  the same catalog, marked `scope: replica` with the replica identity and no
  history. The UI labels this state and states that it is not cluster-wide.
  With Prometheus configured, results are marked `scope: cluster`.

- **REQ-015 — Same behavior in every deployment.** Self-hosted, enterprise
  cloud, and Wyrd-operated SaaS deployments expose the same surfaces; whether
  Prometheus panels are cluster-scoped depends only on configuration. No
  deployment-mode or edition check enables or disables them.

### UI

- **REQ-011 — Platform console page.** The existing SvelteKit UI deployment
  serves a `/platform` route tree, outside `/t/[tenantKey]`, with its own
  minimal shell and no tenant navigation. Its Bifrost operations page renders
  the health summary first, then the panel groups in TASK-005 question order.
- **REQ-014 — Separate sessions.** `/platform` pages require a platform
  session, established through the deployment's platform-scope OIDC connection
  or a platform credential. The UI's server-side layer keeps platform and
  tenant sessions distinct: neither session reaches the other tree, a tenant
  session never sees a link to `/platform`, and the browser never receives a
  platform token. The server's `/platform/*` authorization remains the
  enforcing boundary.
- **REQ-012 — Server-owned meaning.** Panel titles, units, and meanings come
  from the catalog response. The browser does not compute health, compose
  queries, or call Prometheus.

### Agent surface

- **REQ-013 — Machine-readable.** The health summary and panel catalog have
  generated schemas and stable error codes. Agents use the platform-plane
  HTTP API with a platform credential. No MCP tool exposes these surfaces in
  this change.

## Invariants

- **INV-001** — No tenant-plane principal reads deployment-wide Bifrost
  telemetry.
- **INV-002** — No surface accepts caller-supplied PromQL or forwards a
  Prometheus credential to a client.
- **INV-003** — Cluster-scoped results come only from durable owners or from
  Prometheus aggregation across scraped pods; replica-scoped results are
  always labelled as such.
- **INV-004** — No new metric family, label, telemetry ledger, metrics store,
  or write to an external system.
- **INV-005** — Dashboard reads never take locks, leases, or admission slots
  that production Bifrost work needs.

## Scope

- Health summary endpoint, panel catalog and panel endpoints, the optional
  Prometheus configuration, the platform console page, schemas, docs, and
  journeys.

## Non-goals

- Alerting, recording rules, or HPA changes. Prometheus remains the alerting
  and autoscaling authority.
- Wyrd storing its own metric history in Bifrost. A deferred option; it fails
  exactly when Bifrost is degraded and needs its own retention and
  self-measurement design.
- Tenant-facing usage dashboards or per-tenant Bifrost metrics.
- A general dashboard builder, arbitrary PromQL, or Grafana replacement.
- Peer fan-out to other replicas' metrics for the fallback view.
- Gate, Scribe, Oracle, or Forge behavior changes.
- Server-owned health thresholds or healthy/degraded verdicts.
- A platform-plane MCP surface. Wyrd MCP stays tenant-scoped; a platform MCP
  surface needs its own specification once more platform tools justify it.

## Expensive-to-reverse decisions

1. Platform-plane ownership of deployment-wide Bifrost telemetry.
2. Public routes `/platform/bifrost/health` and `/platform/bifrost/panels`,
   their response shapes, `scope` semantics, and error codes.
3. A closed, server-owned panel catalog instead of query passthrough.
4. Prometheus connection as deployment configuration, not a Card.
5. A platform console as a UI area outside tenant navigation.

## Acceptance criteria

- **AC-001** — A journey with a platform credential reads the health summary
  from a multi-replica cluster and matches durable state after a controlled
  write, staging stall, restart, and Forge backlog; the result is identical
  from either replica.
- **AC-002** — A tenant credential is refused on every route, with the
  refusal audited; no tenant UI route or tenant MCP tool shows these facts.
- **AC-008** — A tenant session requesting a `/platform` UI page and a platform
  session requesting a `/t/[tenantKey]` page are each refused; neither page's
  data or navigation renders, and no platform token reaches the browser.
- **AC-003** — With Prometheus configured against a multi-replica cluster,
  each catalog panel returns cluster-scoped series whose values agree with
  the summed per-pod scrape; a quantile panel aggregates buckets across pods.
- **AC-004** — Without Prometheus, panels return `scope: replica` values equal
  to that replica's registry, and the UI shows the replica-only label.
- **AC-005** — Prometheus timeout and refusal, and an unreadable durable owner,
  each return their stable error; none renders as zero or healthy.
- **AC-006** — A request containing PromQL or an unknown panel is refused;
  range and step outside bounds are refused.
- **AC-007** — Generated schemas, the error catalog, the self-hosting guide,
  and the platform console docs are current, and contract checks pass.

Evidence classes: platform-plane user journeys against `WyrdTestCluster`
with and without a test Prometheus, HTTP contract tests on the served OpenAPI
document, and UI route tests for the platform console.

## Open material decisions

None. This draft is ready for approval review.

## Revision history

- Revision 1 — draft, 2026-10-01: initial backlog specification.
- Revision 2 — draft, 2026-10-01: MCP exposure decided — HTTP only; a
  platform-plane MCP surface is a non-goal.
- Revision 3 — draft, 2026-10-01: console placement decided — a `/platform`
  route tree in the existing SvelteKit UI with separate platform sessions.
- Revision 4 — draft, 2026-10-01: SaaS availability decided — identical in
  every deployment, controlled only by configuration.
- Revision 5 — draft, 2026-10-01: health thresholds decided — raw facts only;
  no open material decisions remain.

## Authority

- Tracking issue: https://github.com/bohmian-ai/wyrd/issues/96

- `architecture/wyrd-design.md` — two administration planes; `Source` is a
  tenant read reference.
- `architecture/bifrost-design.md` — Telemetry and Measurement meanings.
- `changes/completed/2026/bifrost-scribe-live-reads.md`
  — operator questions and dashboard measurement contract.
- `changes/active/wyrd-ui-foundation/spec.md` — tenant navigation and route
  rules this console must stay outside of.
- `docs/src/content/docs/self-hosting/kubernetes-production.svx` — scrape job.
