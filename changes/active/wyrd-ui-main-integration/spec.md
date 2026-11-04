---
id: SPEC-wyrd-ui-main-integration
revision: 25
status: approved
---

# Migrate the Wyrd UI onto current main

## Revision 24 — Metrics that stay fast at production volume

**Approved by the caller on 2026-11-04**, with SHA-256 as the hash.

### Problem

The Metrics page is slow on a few thousand points, and it gets slower as data
grows. The cause is how it reads, not the build:

- **It reads every point's full attribute sets.** A point's `attributes`,
  `resource_attributes` and `scope_attributes` are Variants, stored shredded:
  each key is its own column. To tell series apart, the chart groups by the
  whole Variant, so Bifrost rebuilds all three sets for every point. That takes
  0.7–0.9 s per 42,000 points. Reading one key with
  `variant_get(attributes, 'http.route', 'Utf8')` reads only that key's column,
  in about 0.2 s for the same points. Histogram charts take 2.5–3.4 s.
- **It splits every series by default.** A series is all the points of one
  metric that share the same resource, scope and attributes. For example, one
  service instance's `http.server.request.duration` for
  `GET /users 200` is one series, and the same metric for `POST /users 500` is
  another. One service can have thousands of series per metric. The page plots
  up to 20 of them apart and reads labels for up to 1,000, so the read grows
  with the number of series.
- **No column names a series.** Counter and histogram points are often
  cumulative: each point holds the running total since the process started.
  To chart a rate, the page must subtract each point from the previous point
  of the *same series*, then add the series together. Bifrost has no series
  column, so the page can only find a point's series by comparing all three
  attribute sets, which is the expensive read above.
- **The rail lists every metric.** The left rail lists up to 500 metric names
  across all services in the range, with no search. A team with a few hundred
  services has tens of thousands of metric names.

Conventional metric tools (Prometheus, Grafana, Datadog) default to one line per
metric, aggregated across series. A user splits it by chosen labels and asks
for raw series only on purpose. They also identify each series with a stored
id, computed once at ingest.

### Before and after

Before: opening `http.server.request.duration` for one service reads every
point's three attribute sets, builds up to 20 lines and a 1,000-row label
summary, and takes about 3 s.

After: the page reads the metric's value columns plus `series_id` and draws
the request rate, mean and p50/p95/p99 across all series, one line each.
Choosing "Group by `attributes.http.route`" adds
`variant_get(attributes, 'http.route', 'Utf8')` to the read and draws one line
per route, at most 20. Rows read grow with the
points in range; no attribute set is rebuilt.

### Terms

- **WHERE** is the Observe filter bar's list of column filters (for example
  `service_name = checkout` or a Card's `subject_name`) plus its optional
  predicate. Every read on the page applies it.
- **Measure** is the value that ranks groups and series: the increase for
  monotonic sums and histogram counts, the sum for delta sums, and the mean of
  the windows' values for gauges and non-monotonic sums. "Largest" means the
  largest measure over the whole range.

### Required changes

- **REQ-052 — Aggregate by default.** A selected metric is aggregated across
  its series by default, and the page states the aggregation it applied:
  - **Monotonic sums:** each series' increase per window (REQ-050's reset
    rule), added across series and shown as a rate.
  - **Non-monotonic sums and gauges:** each series' last value per window, then
    the sum across series for sums and the mean across series for gauges.
  - **Delta sums:** the points are added up.
  - **Histograms:** REQ-050's plots, computed across series.

  REQ-050's merge rule still applies. Series with different bounds, unit or
  temporality are separate groups, so a default view may hold more than one
  group.
- **REQ-053 — Group by chosen keys.** "Group by" takes up to three keys at
  once, each named with its source:
  - every promoted column of the table, such as `service_name`,
    `service_instance_id`, `deployment_environment` and `scope_name`;
  - `attributes.<key>`, `resource.<key>` or `scope.<key>`.

  A Variant key is read on that key alone with `variant_get`, as its text form
  (an integer as its decimal digits, a boolean as `true` or `false`). A point
  without the key falls in an `(absent)` group. Key suggestions come from the
  attribute sets of at most 200 of the metric's newest points in range; a
  reader can also type a key. The 20 groups with the largest measure are
  plotted, and the title says how many groups there were in all.
- **REQ-054 — Every series, on request.** "Every series apart" is an opt-in
  view. It plots the 20 series with the largest measure. It names how many
  series there were in all, by counting distinct `series_id`. Full attribute
  sets are read only for the points of those 20 series, selected by
  `series_id`, and never for more than those 20.
- **REQ-055 — A searched, scoped rail.** The rail lists metrics only under the
  WHERE. Each entry is one name, type and unit, as today. A search box filters
  names on the server, case-insensitively by substring, as the reader types.
  The rail shows at most 50 entries, in name order. If more match, it says
  "More than 50 match — type to narrow". It never lists all metrics.
- **REQ-056 — A stored series id.** `bifrost.metrics.points` gains
  `series_id`, a nullable `FixedSizeBinary(16)` column with a Bloom filter.
  Bifrost computes it once per point at admission, from the point's series
  identity:
  - metric name, metric type, unit, aggregation temporality and monotonicity;
  - histogram explicit bounds;
  - resource attributes;
  - scope name, scope version and scope attributes;
  - point attributes.

  Description, `metadata`, schema URLs, `resource_entity_refs`, exemplars,
  flags, values and times are not part of it.

  The id is the first 16 bytes of SHA-256 over this encoding:
  - a version byte `1`, then each identity field in the order listed above;
  - each field as a one-byte type tag, followed by a big-endian `u32` byte
    length and the bytes;
  - an absent value as the null tag with length 0;
  - attribute sets as objects whose keys are sorted bytewise at every depth,
    with arrays kept in order;
  - integers as big-endian `i64`;
  - floats as IEEE-754 bits, with `-0.0` written as `0.0` and every NaN written
    as one canonical NaN.

  The same identity therefore gives the same id on every replica, platform,
  tenant and release. A new encoding takes a new version byte. Admission always
  computes the column and overwrites any `series_id` a raw Arrow writer
  supplies; OTLP has no such field, so every stored point carries one. The id
  is not a security boundary.
- **REQ-057 — Series id in the charts.** Every per-series step groups by
  `series_id`: last values, increases, resets and the top-20 choice. REQ-050's
  reset rule applies only to monotonic sums and histogram counts.

### Constraints and invariants

- No Metrics read rebuilds a whole attribute set except REQ-054's read for at
  most 20 series and REQ-053's 200-point key sample.
- Every Metrics read keeps an explicit ceiling: 20 groups × 201 windows of
  rows, and 16 MiB. Hitting a ceiling shows the in-chart error naming it, never
  a partial chart.
- Exactness rules of REQ-028, REQ-039 and REQ-050 stand: no percentiles except
  explicit-bucket estimates, absent is not zero, and integer precision is
  preserved.
- `series_id` is part of the built-in table schema in
  `architecture/bifrost-design.md`, which is updated with the implementation.
  Like earlier built-in column additions, it is creation-only. A warehouse
  whose `bifrost.metrics.points` predates the column is rebuilt, because
  startup refuses a built-in whose stored definition differs. There is no
  migration, backfill or alias. Corrected in revision 25: revision 24 promised
  an Iceberg schema evolution that built-in reconciliation does not perform.
- Bloom pruning for `series_id IN (…)` lists of 4 or more values depends on the
  Bifrost IN-list fix that is being investigated separately. Until it lands,
  REQ-054's 20-id read scans the range's point files for that metric.

### Non-goals and known limits

- The rail still scans the range's `metric_name`, `metric_type` and `unit`
  columns to list names. A metric-name catalog maintained at ingest is the
  upgrade path if that scan becomes slow. It is not in this revision.
- No new table, service, wire encoding or `ChartInput` change. Exponential
  histograms and summaries remain listed and not plotted.

### Acceptance

| ID | Situation | Expected observation |
|---|---|---|
| AC-031 | Open a sum, a gauge and a histogram with default settings | Each is aggregated across its series, and the aggregation is stated. No read touches a whole attribute set: the SQL has no `to_json` and no bare Variant column. |
| AC-032 | Group a histogram by `http.route`; then choose "every series apart" | At most 20 lines each; the titles state the totals; labels are read only for the 20 plotted series. |
| AC-033 | Type in the rail's search under a service WHERE | Only that service's matching names, at most 50, with the "more" notice when more match. |
| AC-034 | Ingest the same series twice, across server restarts and replicas, with attribute keys in a different order | Identical `series_id`; a changed attribute value or bound gives a different id; a `series_id` sent by a raw Arrow writer is overwritten. |
| AC-036 | Live dev data over 24 hours, in a real browser, in light and dark themes | Every Metrics read (rail, default chart, group by, every series) returns within 500 ms on the server, with times and rows read recorded in the task evidence, and the human accepts it visually. |

## Revision 23 — Sorting and paging Observe lists

**Approved by the caller on 2026-11-04.** A signal list read only its newest
50 rows: Traces and Spans said "older traces not read" with no way to read
them, and Logs paged only inside those rows. No column could be sorted.
Conventional trace and log viewers sort on the server by a clicked column and
page through the whole range.

- **Header sort.** On Traces, Spans and Logs, clicking a sortable column
  heading reads the range again ordered by that column, then event time
  newest first, then the row's identity. A second click reverses the
  direction; an empty value sorts last either way. The default stays newest
  first. Sortable columns are the signal's default columns and its filter
  columns, except trace and span ids; Traces-mode span and error counts are
  read per visible row and are not sortable.
- **Keyset pages.** Older and Newer read the next or previous page after the
  first or last row shown, in the same order, so rows sharing a value or a
  timestamp are never skipped or repeated. Offsets are not used: each read
  sees the newest published data, so offset pages would shift. Logs' in-page
  pager is replaced by these controls. The header keeps the range totals.
- **URL state.** The sort and the page cursor live in the URL. Changing the
  signal, range, WHERE or predicate returns to the first page.
- **Filter-only WHERE.** The WHERE bar stays filter-only; an ORDER BY of the
  reader's own goes in Query through "Open in Query".
- **Authored SQL** sorts in the page, since the server refuses an authored
  read past its row cap rather than truncating it, so shown rows are complete.
- **No wire change.** Sorting and paging are SQL over the existing
  `bifrost.query` path. Metric point lists are unchanged.

## Revision 22 — DEBUG logs show by default

**Approved by the caller on 2026-11-04.** REQ-048 said a default UI severity
filter hides DEBUG. The Logs page has no such filter. Like conventional log
viewers, it lists every severity by default, and the records chart shows
TRACE and DEBUG (severity 1–8) as their own `debug` level. REQ-048 now says
so. No default severity filter is added.

## Revision 21 — Histograms on the conventional Metrics page only

**Approved by the caller on 2026-11-03.** Revision 20 assumed an MCP App
metrics view. None exists: the `ui://wyrd/view` bundle renders only authored
view documents, and the Wyrd MCP server does not yet serve that resource or
any tool that returns a view document (step 4 of the delivery order). An MCP
App histogram would need a new `ChartInput` variant for a surface no host can
reach yet.

- REQ-051 and AC-030 now cover the conventional Metrics page only.
- MCP App histograms are deferred to the MCP App delivery (step 4), which
  decides how a view document plots metrics. The shared histogram loader is
  exported from `packages/ui` for that work; no `ChartInput` change is made now.
- Everything else in revision 20 stands.

## Revision 20 — A realistic development workload and plotted histograms

**Approved by the caller on 2026-11-03,** as written: estimated percentiles are offered (open decision 1 resolved to the recommendation) and publishing `dev-workload` is authorized. This records scope approval, not implementation or visual acceptance. The caller asked for two things. First, the
`dev:full` workload should exercise every span status (`unset`, `ok` and
`error`) along with realistic production metrics and logs. Second, Observe
should plot histograms as part of Task 004, using reusable
`packages/ui` components that every UI host can use. Revision 19's approved
decisions are unchanged unless this section amends them.

### Problem

The development workload is too clean to test Observe:

- **Status.** Every span is `unset`. A 404 for an unknown order is not an
  error under OpenTelemetry server semantics, so `ok` and `error` never appear.
- **Provider.** MockServer answers every model call with an instant 200. There
  are no provider failures, timeouts or latency variance, so error counts,
  p95 and the slow-row highlight never show anything.
- **Metrics.** Observe plots only gauges and sums (`PLOTTED` in the shared
  metric loader). Histograms are only listed. Real latency metrics, such as
  the standard `http.server.request.duration` and `gen_ai.client.*`
  histograms and the app's `support_desk.ask.duration`, are histograms.
- **Logs.** The app writes INFO lines and one WARN case. There is no ERROR or
  exception, and nothing below INFO is written.

**Before:** the Errors chart is flat at zero, and every status pill reads
`unset`. A histogram metric's chart says it is not plotted.

**After:** the same 500-request batch yields known shares of `ok`, `unset` and
`error` traces. Failed traces carry exception events and trace-correlated
ERROR logs. The duration charts show a real latency spread with a slow tail.
Selecting `http.server.request.duration` plots request rate, mean and
estimated percentiles over time, plus the bucket distribution. The same Chart
component and shared metric loader draw this in the conventional UI; MCP App
histograms are deferred (revision 21).

### Required additions

- **REQ-045 — Failures come from the provider, not the app.** MockServer
  expectations owned by Wyrd's development fixtures answer the application's
  chat request according to the order id in the ask. Reserved order ids select
  the outcome:
  - HTTP 500;
  - HTTP 429 with `Retry-After: 1`;
  - a 3 s delay, longer than the application's 2 s model-call timeout;
  - a success. Successes are spread over a fixed set of delays (80, 150, 250
    and 400 ms) by order id, and about 5% get a 1.2 s slow tail.

  There is no randomness and no new marker. An order id is ordinary request
  content, visible in Observe like any other ask. Expectations stay
  repeatable under concurrency and across repeated `dev:batch` runs, as
  REQ-037 requires. Judge and tool expectations are unchanged. Failing asks
  produce no answer, so they never reach a judge.

  The application's model client retries 500, 429 and timeouts twice, which
  is the OpenAI client's default, and honors `Retry-After`. A failed request
  therefore makes three provider calls. The gateway, provider identity,
  credentials, capture and settlement keep their ordinary behavior. Unmatched
  requests still fail visibly.
- **REQ-046 — Span status follows OpenTelemetry semantics.** The root span of
  each request is the ASGI server span:
  - **`ok`:** set by the application on that span when it returns an answer.
    The OTel spec reserves `ok` for an application's explicit success, which
    instrumentation never sets.
  - **`error`:** set by the instrumentation for a 5xx response. A model call
    that fails after its retries, or times out, returns 502 or 504
    respectively, and the application records the exception as an event on
    its own ask span, which is also `error`.
  - **`unset`:** everything else, such as `GET /openapi.json` and
    unknown-order 404s, because the HTTP semantic conventions keep a server
    span's 4xx `unset`.

  Status never depends on verification results. Refund asks whose checks fail
  stay `ok`, and Observe and Verify stay separate (REQ-028). The UI shows
  recorded status and never infers it.
- **REQ-047 — Realistic metrics.** The app opts in to the stable HTTP
  semantic conventions, so the instrumentation emits
  `http.server.request.duration` in seconds. The app also emits:
  - its `support_desk.ask.duration` histogram;
  - counters: asks by outcome, and provider failures by kind (one per failed
    request, after retries);
  - the existing in-flight gauge.

  `gen_ai.client.*` histograms appear only if the installed instrumentation
  emits them, and no acceptance depends on them. Metrics use the SDK's
  ordinary providers and default (cumulative) temporality. Nothing is
  fabricated or written directly to Bifrost.
- **REQ-048 — Logs across severities.** The app writes:
  - INFO for each answered request;
  - WARN for each unknown order, and for each model call slower than 1 s;
  - ERROR with exception type, message and stack trace for each failed
    request;
  - DEBUG detail, which Observe lists by default with every other severity
    and charts as its own `debug` level.

  Every record emitted inside a request carries its trace and span ids, so
  Open trace works from any of them. No secret or credential is logged.
- **REQ-049 — The batch reconciles outcomes.** The default mix keeps REQ-038's
  500 requests at concurrency 8. It reserves 35 of the 400 shipping asks for
  injected failures:

  | Count | Request | Root status | HTTP |
  |---|---|---|---|
  | 365 | shipping ask, successful model calls | `ok` | 200 |
  | 15 | shipping ask, provider 500 | `error` | 502 |
  | 10 | shipping ask, provider 429 | `error` | 502 |
  | 10 | shipping ask, provider timeout | `error` | 504 |
  | 50 | refund ask (queued checks unchanged) | `ok` | 200 |
  | 25 | unknown order | `unset` | 404 |
  | 25 | `GET /openapi.json` | `unset` | 200 |

  Smaller bounded runs scale every row proportionally and keep at least one of
  each. Each batch tags its requests with its own batch id. For that batch id,
  the report gives expected and observed counts of:
  - root statuses;
  - ERROR logs;
  - slow-call WARN logs.

  Counter reconciliation uses the difference between readings taken before
  and after the batch, so repeated `dev:batch` runs against the same app
  reconcile independently. Readback uses the existing public bounded waits.
  Any mismatch, or any error outside the injected rows, fails the batch. This
  replaces REQ-038's default mix and narrows its unexpected-error rule.
- **REQ-050 — Observe plots explicit-bucket histograms.** Selecting a
  histogram metric shows:
  - its count rate and its mean (sum ÷ count) over time;
  - p50, p95 and p99 estimated from the buckets;
  - a bar chart of bucket counts over the applied range, with one categorical
    bar per bucket, labeled by its bounds (for example, `0.25–0.5 s`).

  Windows are the chart's existing time buckets for the applied range. For
  each series and window, the loader works as follows:
  - **Cumulative series:** use the increase between consecutive points. A
    start-time change or a decreasing count is a reset, and the post-reset
    point counts from zero.
  - **Delta series:** sum the points.
  - **Merging:** within a group, buckets merge only across series with
    identical bounds, unit and temporality. Other series stay separate.

  Percentiles use the conventional linear interpolation within the bucket
  that holds the rank, as Prometheus `histogram_quantile` does. The first
  bucket's lower bound is 0. A rank that falls in the unbounded last bucket
  shows `≥` the last finite bound. A window with no counts is a gap.
  Grouping and series selection keep the gauge and sum identity rules.
  Exponential histograms and summaries stay listed, with a statement that
  they are not plotted.

  This replaces REQ-028's and REQ-039's "no percentiles" rule for
  explicit-bucket histograms only. It still applies to summaries and to
  every other metric kind. Span duration percentiles remain exact.
- **REQ-051 — Reusable across hosts.** Histogram plots use the existing
  `Chart` component:
  - `timeseries` for rate, mean and percentiles;
  - `bar`, which is categorical, for the distribution, so bucket widths that
    are not uniform need no new chart form.

  They are driven by the shared metric loader in `packages/ui`, through the
  existing authenticated BFF/executor path. The page owns only host and layout
  state. The conventional Metrics page renders it from that loader output. MCP App
  histograms are deferred to the MCP App delivery (revision 21). No new
  component, `ChartInput` schema change, server service, table or wire
  encoding is planned. If one proves necessary, it returns to this spec for
  approval.

### Delivery and ownership

- **`bohmian-ai/wyrd-example-py`:** the application's status, logs and
  metrics, the batch mix and the reconciliation. They are delivered on its
  `dev-workload` branch. Wyrd pins one exact commit of that branch in place of
  REQ-038's `03428e1a…` pin and its patch kept in this repository. Wyrd keeps
  no copy of the example's code. `dev:full` clones the pinned commit, so the
  branch must be published to the example repository.
- **Wyrd:** only the MockServer expectations and the Observe histogram
  support.
- **Owner:** TASK-004-REV-03 owns all of it, including the conventional metric view, as revision 19 already assigns it the remaining
  Observe and dev workflow obligations.

### Invariants

- **INV-008 — Injected failures are ordinary provider failures.** No
  test-only header, mode, mock provider type or app branch decides an outcome.
  Only the order id that MockServer matches does.
- **INV-009 — No unlabeled estimate.** Every interpolated percentile, and
  every `≥ bound` result, says "estimated" wherever it is shown: chart titles,
  tooltips, summaries and agent-readable output. Exact values are never
  labeled as estimates.

### Non-goals

- Exponential-histogram and summary plotting.
- Random fault injection and chaos tooling.
- Log-scale axes.
- Changes to retained histogram encoding.
- New Bifrost tables.

### Acceptance for revision 20

| Criterion | Required observation |
|---|---|
| AC-028 | A default `dev:full` batch reports expected and observed counts that match for every REQ-049 row, log class and counter; Observe shows `ok`, `unset` and `error` traces; each failed trace has an exception event and a linked ERROR log; a second `dev:batch` reconciles on its own batch id. |
| AC-029 | Shared loader unit fixtures cover cumulative and delta temporality, resets, mismatched bounds, empty windows, interpolation and unbounded-bucket results. A live `http.server.request.duration` selection plots rate, mean, labeled estimated percentiles and the distribution. |
| AC-030 | The histogram plots render from the shared loader output on the conventional Metrics page, in a real browser, in light and dark themes, with human visual acceptance. |

### Resolved decisions

1. **Estimated percentiles.** Resolved: offered. Recommended: offer them, labeled (REQ-050,
   INV-009). The alternative is to plot only rate, mean and the distribution.
   That keeps REQ-028's rule unchanged, removes the percentile clause of
   REQ-050, INV-009 and AC-029, and leaves latency histograms without a p95.
2. **Publishing `dev-workload`.** Resolved: authorized. A fresh `dev:full` needs that branch pushed
   to `bohmian-ai/wyrd-example-py`. Approving this revision authorizes that
   push.

## Revision 19 — Local humans, personal credentials and aligned Observe delivery

**Approved by the caller on 2026-11-02.** The caller requested a third Task 004 revision on
2026-11-02: Wyrd-owned username/password signup and login, personal API-key
Settings for local and SSO users, and a complete `dev:full` journey with seeded
human credentials and the exact approved Observe composition. These requested
outcomes and the admission, credential, persisted and visual contracts below,
including Task 03's typed public contracts, are explicitly approved. This
records scope approval, not implementation or rendered visual acceptance.
Revision 18's delivered owners and prior acceptance records remain context.
Task 03 owns the new account/dev delivery
and the remaining Task02/R1/R2 functional/visual obligations. Their implementation
is retained and their acceptance evidence remains context; do not run conflicting
parallel assignments or require Task02 PASS before fixing its remaining gaps.


Today `dev:full` provisions an administrator API key and no human password.
The ordinary SSO route has no provider configured. Personal Settings does not
exist, and current credential SQL/issuer paths cannot support real User-owned
keys. The [current audit](references/task-03-observe-audit.md) also identifies
remaining Metrics, Logs, Traces and waterfall layout/color differences.

The intended outcome is one command, a usable browser URL and local username/
password before the 500-request batch finishes, normal Rust-owned login, an
account Settings page and real evidence in the agreed UI. Passwords and keys
extend existing auth/SQL/client owners; no external IdP is required for local
users and no parallel Node credential/session service is introduced.

### Approved security and product decisions

- **Admission — provision a workspace, then enroll its people.**
  Self-hosted operator setup provisions the usual single tenant through the
  existing TenantProvisioning owner and authorizes initial human-owner
  enrollment. SaaS onboarding uses the same protected platform API to provision
  a distinct tenant for each team and authorize its initial owner. Both then
  use one 24-hour, one-use enrollment/invitation workflow: code, username and
  password enroll a User with administrator-selected roles. Later teammates
  join by invitation. Dev:full automates the self-hosted bootstrap and seeds its
  local human; an anonymous first visitor never becomes administrator.
  Tenant creation remains a platform capability: password signup cannot
  fabricate PlatformCaller, receive tenants:write or choose an arbitrary
  existing tenant/role. Public anonymous SaaS workspace creation, verified-email
  delivery and billing/onboarding orchestration are separate platform-product
  work, not silently added by this local-auth task. It may use this same
  enrollment API after protected provisioning. No separate self-host/SaaS auth
  implementation or misuse of Development/Production profile is introduced.
- **Local identity.** Use a tenant-local username3–64 ASCII characters:
  first character alphanumeric, remaining alphanumeric/dot/underscore/hyphen;
  trim surrounding whitespace and ASCII-lowercase once at input. Store that
  canonical form with tenant-local uniqueness for local credentials. The
  immutable User ID remains identity; username rename is outside this slice.
  Optional email/SSO claims remain metadata and never identify/link accounts.
- **Passwords.** Adopt15–128 Unicode characters after consistent NFC
  normalization, preserving case/spaces and rejecting control characters; limit raw input to1024 UTF-8
  bytes before expensive work. Use a local common-password blocklist,
  password-manager/paste support and no composition rule or periodic rotation.
  Store an Argon2id PHC verifier with a fresh random salt, initially memory
  19456 KiB/iterations 2/parallelism 1; reuse workspace Argon2/password-hash and
  library verification, bounded hashing work and rehash-on-success. Never store
  recoverable passwords. Unicode normalization's installed transitive crate
  needs a declared narrow dependency if used; no auth framework is needed.
- **Abuse and recovery.** Generic unknown-user/wrong-password/disabled failure,
  including a dummy-verifier path; bounded attempts across replicas use tenant
  auth/SQL state and PostgreSQL time. Proposed baseline:10 failures per
  tenant/normalized-username in15 minutes,100 per trusted source in15 minutes,
  retry only after the fixed window expires; cap concurrent KDF jobs at4 per
  process. Use the existing auth maintenance owner for expiry cleanup. Initial
  recovery is operator-assisted: an authorized users:write administrator with the existing
  tenant-admin grant authority issues a 15-minute single-use reset code; the person chooses their own new password.
  No unimplemented email recovery is promised. Password change verifies the
  current password; reset requires the code; both bump credential revision and
  end local password sessions. Account suspension/deletion refuses all sessions
  and new key exchanges. Keys remain independently revocable credentials.
- **One session authority.** Extend existing BrowserSessions and its SQL rows
  with exactly-one local-password authority binding to User/credential revision,
  alongside existing SSO connection revision and recovery-key authority.
  Preserve fixed12-hour sessions, opaque host-only HttpOnly cookies, production
  Secure/SameSite=Lax, exact Origin/tenant/bearer-precedence rules and no-store.
  SSO identity remains issuer+subject; no same-email/username merge and no
  setting a local password on an SSO-only User through an ordinary session.
- **Personal keys.** Any active User may manage only their own personal keys;
  server-derived tenant/User identity grants that explicit self capability,
  without service_accounts:write or tenant OIDC administration. Define
  personal_credentials:read/write as authenticated-User self-only capabilities
  in the existing permission owner; include them in real User authority before
  credential ceiling intersection, never in Service authority. Read lists own
  metadata; write creates/revokes own credentials, with the separate recent
  human-login requirement for creation. Enforce both capability and ownership
  through the configured authorization owner. Extend existing principal
  credential owners; no page-level permission bypass. Named keys
  have a nonempty explicit permission ceiling and expiry; the UI explicitly
  selects30 days initially, permits shorter durations and caps at365 days.
  Ceiling must be within the caller's effective authority and current User
  authority; future exchanges intersect stored ceiling with current User
  grants. No privilege is inherited beyond that ceiling after role growth.
- **Persisted key ownership and exchange.** Extend the existing credential
  table with exactly-one tenant-bound relational owner: existing User or
  existing Service/Admin account. Preserve existing credential IDs, verifiers,
  prefixes, expiry and attribution in a forward migration; no surrogate Service
  or new identity store. Lookup/verification must return the verified owner
  kind, and TenantTokenIssuer must resolve that owner rather than infer kind
  solely from OAuth grant family. Retain current short-lived JWT snapshot and
  revoke-at-next-exchange semantics, and existing automation behavior.
- **One-time key secret.** Reuse256-bit generation/SHA-256 verifier storage and
  SecretBearer. Personal creation is a no-store authenticated response shown
  once for deliberate copy, never recoverable later. Metadata includes name,
  safe prefix, scopes, created/expiry/last-used and revocation. Require a human
  login/session no older than5 minutes for creation; local and SSO users can
  repeat their normal login. Freshness means Rust session creation after a
  successful password verification or normal bound SSO completion; an existing
  IdP session may satisfy that SSO completion. Merely reusing a Wyrd cookie/API
  key does not refresh it. No forced provider password entry, new auth_time
  profile or MFA assurance is claimed. Existing administrator issuance remains distinct; ordinary personal creation is an
  interactive browser-auth HTTP operation, not an API-key-to-master-key chain.
  SDKs retain normal use/exchange and expose own metadata/revoke; MCP creation
  remains absent to prevent secrets entering model transcripts.
- **Recovery boundaries.** Enrollment/reset codes are random256-bit one-use
  secrets with verifier-only storage, purpose/tenant/issuer binding and database
  expiry. Deliver code plus a fixed tenant signup/reset URL separately; codes,
  passwords and API keys never enter URLs, logs or screenshots. Operator-created
  invitations cannot grant roles beyond the existing administrator grant owner.
- **Dev workflow.** The dev scripts run `wyrd-server setup --tenant dev` as
  `wyrd init` does, then create the admin User `development` with password
  `development` through `wyrd principal user create`, and wait until both exist
  before dispatching the batch. Production setup creates no User.
  Disable inherited fixture auth in dev:full; local login must not require
  Keycloak. Keep optional SSO fixtures and the existing500-request/concurrency8,
  production wheel/source pin, failed-batch inspectability and app-first drain.

### Required additions

- **REQ-041 — First-class local human signup/login.** Approved admission creates
  a real tenant User with the chosen local credential, supports normal login,
  logout, password change and operator-assisted reset, and enforces current
  membership/credential revision across replicas. Use the same browser session
  owner as SSO; add typed browser HTTP contracts, SQL migrations and truthful
  server-rendered SvelteKit forms.
- **REQ-042 — Personal account Settings and credentials.** Local and SSO User
  sessions share an ordinary account Settings page showing identity/auth method,
  local password-change where applicable and own-key create/list/revoke.
  Tenant SSO administration stays a separate authorized section. A User's key
  must actually exchange/use the public SDK as that User; ownership, permissions,
  expiry and revocation must be enforced by the existing shared backend chain.
- **REQ-043 — Inspectable normal development workflow.** One dev:full command
  reaches real UI/local-login readiness and displays seeded human access before
  workload completion; the signed-in user sees retained real Traces/Spans, Logs,
  Metrics and waterfall, can use Settings and logout, and sees new data from an
  appended dev:batch. No recovery-key login or test fixture identity substitutes
  for the primary journey. Existing recovery remains separately available.
- **REQ-044 — Exact approved Observe visual composition.** Main, Metrics, Logs,
  Spans, TraceDrawer and TimePicker define the layout/region/spacing/type/color
  target, using current shared components. Close every source-backed difference
  in task-03-observe-audit.md; do not treat current screenshots, generic Chart
  defaults or a difference list as a newly approved design. Preserve current
  fields/real data, existing mark assets/local fonts and truthful unavailable
  percentile choices. Metrics retains its two-card default board; an explicit
  recorded-points inspector may reuse existing Drawer/RecordTable and the metric
  list loader for required retained structures, rather than adding a default
  third table or a new detail service. The unsafe exemplar integer limitation
  remains explicitly deferred by the caller; no DataFusion/Bifrost encoding
  work is revived.

### Canonical color reconciliation

Keep one palette/generator and update its owning brand guidance to adopt the
reference roles. Proposed role mapping fixes values instead of permitting an
implementor to invent a near match:

| Role | Dark value from canvas | Approved light counterpart |
|---|---|---|
| Navigation surface | #0e1017 | existing secondary surface #eef0f4 |
| Selected/filter border | #3a4170 | existing declaration stroke #4d5ef0 |
| Log info fill | #3a4170 | #4d5ef0 |
| Log warning/error fill | #f0a13c / #e2484d | same status fills, existing text/glyph safeguards |
| p95 / p50 | #c5f23b / #7a8cff | #c5f23b / #4d5ef0 |
| Additional chart/waterfall series | #a9b4ff, #8fae2a, #4d5ef0 | #4d5ef0, #8fae2a, #7a8cff with noncolor cues |

Reference fills must not be changed by unapproved opacity blending. Retain
canonical text/surface/rule/font tokens where the canvas already matches. New
semantic tokens are authored only in brand/palette.json, with appropriate
contrast pairs and regeneration; never duplicate hex in page/component CSS.
Waterfall distinctions use actual recorded identity/role where present and
consistent legend-backed series identity otherwise; no invented span-name
semantics. Dark canvas mapping is exact; because no light canvas board exists,
the stated light mapping and its actual browser result require human acceptance.

### Authority amendments required with implementation

Amend architecture/wyrd-design.md Human sign-in/Client model and
architecture/wyrd-security-posture.md API keys/First-party browser sessions/
Permission check to include local credentials, exactly-one local session
binding, owner-bound personal rights/permission ceilings and the deliberate
one-time Settings secret response. Those existing sections remain authoritative
until their approved revision-19 amendments land with the behavior.
No bearer revalidation model, machine OAuth password grant, external IdP,
permission shortcut, durable Node session, SSO linking, email system or duplicate
credential service is authorized.

The one-time key-creation response is the sole narrow exception to INV-002's
no-browser-page-secret rule: deliberate no-store POST response/transient display
only, never initial SSR load/PageData, browser storage/history, URL, logs,
telemetry or captures. Enrollment/reset forms similarly transmit supplied
secrets only in scrubbed POST bodies. No new production credential exposure
is justified by development console seeding.

### Approved public contract additions

The normative field/type definitions, required/optional/null rules, repeated
password route table, profile projection and explicit credential paging/client
compatibility contract are packet-local in [Task 03 — Public contracts](tasks/TASK-004-revision/TASK-03-local-users-personal-keys-and-observe.md#public-contracts).
That section elaborates the approved revision-19 contract below; implementations must
not select different shapes independently in HTTP, MCP or an SDK.

Browser password operations keep the existing JSON/problem+json auth protocol,
strict request schemas, exact Origin check and cookie/no-store response owner;
they are not OAuth password grants. Password fields are JSON strings with
redacted secret handling, never debug/loggable raw values.

| Route | Input / authority | Success |
|---|---|---|
| POST /v1/principals/users (CLI `wyrd principal user create`, password on stdin) | username, password, Role names; User management plus tenant administration | 201 principal id and canonical username |
| POST /auth/browser/password/signup | tenant, invitation_code, username, password; invitation authority, no role/User input | 201 BrowserSessionView plus the existing opaque session cookie; BFF redirects to the fixed tenant Observe entry |
| POST /auth/browser/password/login | tenant, username, password | 200 BrowserSessionView plus fresh cookie |
| POST /auth/browser/password/change | tenant, current_password, new_password; selected local User session | 204, revoke local revision sessions and clear selected cookie; ordinary login required |
| POST /auth/browser/password/reset | tenant, reset_code, new_password; code authority | 204, invalidate reset/local sessions; no automatic login |
| POST /v1/principals/users/invitations (CLI `wyrd principal user invite`) | existing Role names; users:write and existing grant authority | 201 invitation metadata, one-time code, fixed signup URL; default 24-hour expiry |
| DELETE /v1/principals/users/invitations/{id} | tenant-bound user administration | 204 revocation |
| POST /v1/principals/users/{id}/password-reset | local User target; users:write plus existing tenant-admin grant authority | 201 reset metadata, one-time code, fixed reset URL; 15-minute expiry |
| GET /v1/principals/me | actual authenticated User, not a caller-selected subject | safe own profile and effective self capabilities; no password verifier/key secret |
| GET/POST /v1/principals/{principal_id}/credentials and DELETE credential subroute | extend existing routes with explicit real-User self case and required personal-credential capability; other targets retain administrative checks | existing metadata/IssuedCredential/revoke shapes extended for named scoped User keys; POST own creation additionally needs recent human login |

Reuse existing targeted credential routes, clients and MCP metadata/revoke tools;
do not add synonymous /me/credentials aliases. UI derives its principal target
from the Rust session, and the server compares it with verified caller ownership;
a target supplied by a client is never authority. Existing Service/Admin
administrator issuance keeps its no-body behavior. Newly usable User-target
issuance must require name/ceiling/expiry rather than silently mint an
unrestricted User key. Other-User issuance requires tenant-admin grant authority;
self-service rights alone can never issue for someone else.

Own-key metadata reads are bounded: default 50, maximum 100 records with an
opaque continuation cursor; no key secret in any list page. The existing key
creation response retains SecretBearer; the User additions are normalized name,
permission ceiling and expiry metadata, not a parallel key format.

Only the bootstrap/dev credential-delivery exception may retain the generated
initial development password in a private 0600 ephemeral record for the live
session and test owner; remove it during owned cleanup and never copy it into
review/CI artifacts or client credential caches. Production User/password
stores retain PHC verifiers only. Production setup does not seed a default
human password. Apply rate admission to every password verification path,
including password change/recent local login, not only the login page.

### Acceptance for revision 19

| Criterion | Required observation |
|---|---|
| AC-024 | Real server/BFF local signup and login obey approved admission and password policy; unknown/disabled/incorrect/throttled/expired/reused/cross-tenant flows refuse without leaked credentials or partial membership; reset/change revokes local sessions across replicas and SSO remains unchanged. |
| AC-025 | Ordinary local and SSO Users create a scoped expiring own key once, list metadata, use it through first-class clients as the actual User, and revoke it; other-user/tenant access, scope escalation and service-surrogate identity are refused. Existing administrator/workload keys remain usable. |
| AC-026 | Normal dev:full seeds a usable local human and exposes UI/login readiness before its 500-request batch completes; a real browser logs in, inspects all signals/waterfall, manages own keys, appends evidence and logs out, then Ctrl-C cleans owned children. |
| AC-027 | Actual six-board comparison plus geometry/computed-token/interaction checks on desktop/mobile and verified light/dark themes match the approved reference contract; all named audit gaps are closed and each component/board has separate human acceptance. Generated captures/diffs stay outside the repository. |

Costs: new forward auth migrations and local credential/recovery/abuse lifecycle,
shared User-key issuance/projection work and bounded shared presentation seams.
Reuse of existing owners is mandatory but cannot substitute for missing
functionality. No readiness, implemented verdict or visual approval is granted
by this scope approval. Task 03 is ready for implementation; review and rendered
visual acceptance remain separate requirements.

## Revision 18 — Local MockServer and a working live Observe UI

The caller explicitly directed this scope on 2026-11-02: retire the old
TASK-004 revision assignments, create two replacements, and make `mise run
dev:full` run the canonical Python application with a mock LLM service so the
user can navigate the real shell and see traces, spans, logs and metrics.
This records approval of those requested outcomes, not implementation,
readiness, review PASS or human visual acceptance.

The mock is optional local development tooling. Use official MockServer and
the gateway's existing OpenAI endpoint override, including for independent
queued judges. Provider identity, credentials, routing, capture and settlement
retain their ordinary behavior. No Rust mock provider, runtime SDK/header
selection, nonbillable policy, schema change or verification propagation is
required. The earlier internal-mock designs are retired, not prerequisites.

[TASK-004-REV-01](tasks/TASK-004-revision/TASK-01-mockserver.md) owns the local
fixture service. [TASK-004-REV-02](tasks/TASK-004-revision/TASK-02-live-observe-workflow.md)
owns the canonical current-SDK batch and complete shell/Observe delivery,
including the former TASK-012/020/021/022 outcomes and outstanding TASK-004/R1
component acceptance. TASK-004 stays incomplete until that live workflow,
independent task review and individual human acceptance are complete. The
[retired revisions](references/retired-task-004/) preserve old evidence only.
Preserve the caller's cleanup deletions and unrelated component/UI work.

The development batch defaults to 500 requests at concurrency 8 (caller
amendment, 2026-11-02). Its model responses are scripted; its HTTP requests, tools, Runs, verification execution
and retained telemetry use the real application and server. MockServer adds
one local container dependency and no paid inference requirement. The existing
ephemeral environment retains evidence during a running session, not across
restart. Full-library and later MCP shipping obligations remain unchanged;
only this Observe slice has the existing early-live ordering exception.

## Objective and authority

Carry useful historical UI work onto current Wyrd in increments that a human can
run and inspect before the next dependent increment. First make the existing
production UI installable and reachable, with browser sessions owned by the Rust
server. Then migrate the implemented domain screens, reconciling their data
against current contracts.

Revision 14 aligns the remaining UI work with approved
[service-identity-and-telemetry revision 12](../service-identity-and-telemetry/spec.md).
The earlier UI contract attributed Service telemetry through resource labels;
the approved replacement separates the exact executing workload, observed Card,
and accountable accounts. It also replaces the public SQL catalog and table
schemas. Without alignment, filters can combine different releases or exclude
valid evidence, and existing authored queries stop resolving.

For example, two Service releases can use the same Model and stable account.
Selecting one release must show rows whose `workload_card_uid` identifies that
release, while its component breakdown uses the Model's `subject_card_uid`.
The shared Model, account, or `service_name` cannot identify the release.

The cost is coordinated SQL, field-mapping, example and fixture updates, plus
renewed proof for affected consumers, including completed foundation consumers.
Dynamic result handling, layouts, browser sessions, flat Card identity, and the
existing read/transport owners remain. No new table-administration page,
account-management UI, retry controller, migration or old/new schema
compatibility layer is added.

### Dependency and authority for revision 14

The service packet's [telemetry schemas](../service-identity-and-telemetry/supplements/telemetry-schemas.md)
and [table/correlation contracts](../service-identity-and-telemetry/supplements/table-storage-and-correlation.md)
define the target data this UI consumes. Their owning design/security/Bifrost
amendments and matching server, SDK and MCP contracts must land before affected
live UI acceptance; they shipped on main before this revision's approval.
Current architecture remains authoritative; this revision does not amend
their owners.

Service TASK-003 owns catalog/schema/attribution and existing example/fixture
consumer closure, TASK-004/005 supply resolved table descriptions, and TASK-006
supplies whole-trace verification semantics. UI loaders and pages own their
consumption and visible behavior. Planning must map affected consumers and
proof to these dependencies once, without duplicating upstream work. Preserve
prior completion records; renew verification only where the cutover affects
behavior or evidence. Affected live work waits for delivered contracts rather
than inferring fields or falling back to the previous catalog. Service chart
persistence, non-sensitive requirement-outcome readback, and Change Request/
repository APIs remain separate prerequisites.

The caller approved revision 14 on 2026-10-29. The following paragraphs retain earlier approval history.

This revision replaces an invalid draft that claimed revision 1 approval. That
claim is not carried forward. The caller approved revision 5 on 2026-10-19;
revision 9 incorporates the subsequent main update, full-library completion
requirement, replacement readiness reviewer, and the caller's loader/action/SQL
and read-parity decisions. Independent spec readiness review returned `Approve`;
the caller explicitly approved revision 9 and requested `$wyrd-plan` on
2026-10-19. Revision 10 records the caller's explicit instruction to revise this spec
with Rust-owned opaque browser sessions and write the remediation task after
rejecting additional UI deployment secrets and environment variables. This is
approval of that decision and its necessary implementation/task scope; it is
not a claim that revision-9 implementation or visual acceptance passed.

Revision 11 records the caller's explicit approval of the two primary deployed
journeys and request for `$wyrd-plan` on 2026-10-20. Conventional UI and
agent-harness MCP App investigations are required; AskPanel/Ask service are
deferred, and saved views are an independent deferred capability. This revision
changes scope, acceptance and ordering only. TASK-003's foundation obligations,
existing session remediation, and prior immutable review subjects remain intact.
No implementation, browser-host compatibility or journey PASS is implied.

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
  Preserve exact public-origin validation and the existing Rust provider
  callback at `/auth/callback`. REQ-033 replaces the Node OAuth-code completion
  at `/login/callback`; it does not weaken provider state/callback checks or
  change machine-client callback contracts. Forwarding targets the configured
  server, never
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
- **REQ-006 — Server-owned login exercised.** Extend the existing Rust login,
  identity, permission and session-lifecycle owners for REQ-033. Preserve the
  user workflows: tenant SSO, API-key recovery, chooser, tenant switching,
  signed-in identity, settings and logout. Node owns rendering and trusted
  forwarding, not OAuth code redemption, refresh, credential storage or session
  authority. Verify actual native and official-nginx public paths using the
  repository identity fixtures. Preserve existing platform, CLI/device, SDK,
  workload and provider-connection-test behavior; adding a platform browser UI
  or changing machine bearer authentication is outside this revision.
- **REQ-033 — Rust-owned opaque browser sessions.** Use ordinary server-side
  sessions in the existing Postgres database. This replaces the confidential
  `wyrd-ui` OAuth client/encrypted credential-cookie decision, not the external
  provider's own client authentication or Wyrd's CLI/SDK OAuth contracts.
  - **Ownership and persistence.** Rust alone creates, resolves, expires and
    revokes browser sessions. A high-entropy random session identifier is
    generated per successful login, carried only in a host-only HttpOnly cookie,
    and stored only by digest. The durable record binds one tenant and
    principal, creation and absolute expiry, revocation, and either the exact
    human connection revision or the originating API-key credential identity.
    No recoverable API key, provider token, access token or refresh token is
    stored in that record or cookie. Use the existing auth/SQL owners with
    tenant RLS; existing OAuth refresh rows are not a generic session store.
    Expiry and coordinated database times are Postgres-owned. Expired records
    use existing auth maintenance/retention ownership, not a new scheduler.
  - **Login and lifetime.** Extend the existing provider login transaction and
    callback owners for a browser-session completion. Preserve browser
    correlation, state, nonce, provider-side PKCE, exact issuer/connection/
    callback validation and one-use completion. Any temporary browser cookie
    is also an opaque, expiring transaction binding; Node never holds a PKCE
    verifier or performs the second OAuth code exchange. Login transactions
    expire after five minutes and sessions after twelve absolute hours,
    without sliding extension or a browser refresh-token grant. An existing
    session identifier is never adopted for a new login.
  - **Browser HTTP boundary.** Rust provides typed, documented browser
    operations under `/auth/browser`: `POST /login` starts tenant SSO;
    `POST /api-key` verifies a recovery key and creates a session;
    `GET /session` resolves safe tenant/principal/permissions/expiry metadata
    (optionally filtered by tenant route key); `POST /logout` revokes the
    selected tenant session and clears its cookie. Login and recovery accept
    only a tenant route key plus the applicable recovery input, not a caller-
    supplied principal or permissions. The key itself establishes its tenant;
    a mismatching route key is refused. The shared `/auth/callback` completes
    provider login and redirects to that tenant's fixed signed-in route; no
    caller-controlled return URL or public Node OAuth callback is needed.
    An unfiltered session read enumerates only live sessions proved by the
    presented tenant cookies, never by readable cookie labels alone. Discard
    and clear invalid or expired cookies during enumeration, returning only
    live per-tenant metadata without merging principals or permissions across
    tenants. If none remain, or an explicitly selected tenant has no valid
    session, refuse authentication instead of falling back to another tenant.
    Store unavailability still fails the request. No
    operation returns a session identifier, OAuth token or key in page data,
    URLs, JSON metadata or logs. Session/credential responses are `no-store`.
  - **Authenticated API use.** The Node server forwards only the selected
    tenant session cookie and trusted public-origin context to existing Rust
    HTTP APIs; it receives the existing response shapes, not newly minted
    credentials. Rust resolves the browser session into the existing principal,
    credential attribution, permission and non-blocking audit boundary. Cookie
    authentication is supported for the existing protected tenant `/v1` HTTP
    surface, not platform, MCP, OTLP, gateway or gRPC authentication. That API
    path requires exactly one selected tenant-session cookie; ambiguous multiple
    session cookies are refused rather than selecting the first. The metadata
    enumeration operation alone accepts multiple tenant-session cookies. An explicit
    `X-Wyrd-Access-Token` always takes the existing bearer path; a refused bearer
    must never fall back to a cookie. Cookie-only mutations, including login,
    recovery and logout, require an exact trusted public Origin; absence or
    mismatch is refused. The provider callback uses its bound one-use login
    transaction instead. Node checks inbound action origin before forwarding
    and cannot launder an untrusted or absent browser Origin into trusted
    mutation authority. Ordinary GETs never perform settings writes, logout
    or other authenticated mutations. Keep SvelteKit form checks as an
    additional boundary. Document this cookie alternative in served OpenAPI;
    do not make every existing protocol accept ambient browser cookies.
  - **Current authority and revocation.** Resolve current tenant/principal
    state and permissions on each browser-session request through existing
    authority owners. Human sessions require the exact Active connection
    revision. Recovery sessions require their originating credential to remain
    valid and retain its restrictions; they do not widen into the principal's
    full grants. Logout immediately revokes only that tenant browser session,
    not the API key, other sessions or the external IdP session. Store outage
    is an unavailable error, distinct from invalid/expired/revoked authority;
    neither is successful anonymous, cached or mock fallback. Existing tenant
    bearer tokens retain their five-minute snapshot semantics.
  - **Deployment and transition.** Use the existing database and deployment
    authentication configuration. No new operator-managed secret, shared Node
    key, environment variable, secret bootstrap step, session service, Redis,
    sticky routing or per-process session authority is added. Replicas resolve
    the same durable session; replacing Rust or Node does not lose it.
    Production cookies are Secure and SameSite=Lax; the existing loopback-only
    development HTTP exception remains local and explicit. Session and login
    cookies are host-only, HttpOnly and path-scoped to the application. Native
    and nginx forwarding preserve separate Set-Cookie headers and public origin.
    Old credential-bearing UI cookies are cleared and require sign-in again;
    no plaintext-cookie fallback or browser refresh compatibility is retained.
    Stop issuance/acceptance of the retired UI-only OAuth profile after its
    callers move, without deleting historical auth records or changing live
    CLI/SDK, device, platform, workload or provider-test contracts. Remove only
    UI-specific machinery whose consumer is gone; reuse existing issuer,
    provider, authorization, SQL and audit owners rather than duplicating them.

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
  - Keep the user workflows and source owners below. Revision 10's REQ-033
    deliberately changes session ownership in the TASK-001 remediation; this
    keep list does not freeze credential-bearing cookies or Node OAuth logic.
    Outside that explicit change, keep unchanged in behavior: `src/lib/server/auth/`,
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
  no Ask entry or embedded conversation in this delivery. AskPanel is deferred
  by revision 11. It is built new
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
  results come from `bifrost.verification.results`; `subject_card_uid` is the subject,
  `verifier_card_uid` is the Verifier, `workload_card_uid` is the binding's
  exact root, and `execution_id` is not application `run_id`. Drift is a check
  with detail; Agent judging is a check. Mock-only
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
  the current slice. Every in-scope inventory component must be fully implemented, tested,
  and visually accepted on desktop and mobile before the library ships or
  conventional pages begin, except the early live slice in REQ-039.
  Fixture-only progress cannot satisfy that gate.

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
  span; the UI must not assume them. Available automatic sources are Skald
  `invoke_agent`/`chat`/`execute_tool` spans (server/CLI Agents), the opt-in
  SDK `WyrdAgentsTraceProcessor` (same names), one Gateway span per attempt, and
  `bifrost.gateway.calls` (latency `terminal_at - started_at`, usage, cost,
  outcome, attributed under REQ-023). Generic telemetry need not identify a
  workload, subject or application Run. The UI never decides pass, fail, or
  drift: it renders
  recorded results (`bifrost.verification.results.verdict`, requirement outcomes,
  `bifrost.drift.result_features.verdict`); aggregates such as failure rate only
  count recorded results, not pending executions or presumed missing observations.
- **REQ-023 — Workload, subject and account correlation (platform prerequisite).**
  Consume the approved service packet's shared managed envelope on system and
  dynamic tables. `workload_card_uid` is the validated exact Service
  execution/binding root; `subject_card_uid` is the exact observed or judged
  Card; `run_id` is optional application Run correlation. These three values
  may be null independently where the server contract permits. Required
  `sub_principal_id` and `act_principal_id` identify the original action's
  accounts; `wyrd_request_id` correlates the request; `wyrd_ingested_at` is UTC
  microsecond admission time. Preserve returned nulls and original attribution,
  including on internally delivered verification results.
  - The server derives workload attribution from validated execution or
    binding context. Resource labels, stable accounts, shared components, trace
    membership and latest-version lookup cannot establish it. The source OTLP
    subject attribute remains `wyrd.card_uid`; stored schemas and SQL use
    `subject_card_uid`. The UI does not rename that source attribute or invent
    managed values.
  - Exact Service pages filter by the selected release's `workload_card_uid`
    and break down by `subject_card_uid`. A family-wide scope, wherever already
    required, resolves registered version UIDs through Cards and filters those
    UIDs; it cannot match only a name or resource label. This revision adds no
    family-selection control. Generic Observe/Verify ownership labels use
    Workload and identify the exact Service root. Account IDs and
    resource-service/environment labels remain separate query dimensions; this
    revision does not add dedicated account-filter controls.
  - A workload alone is not an execution join key. Use the applicable Run,
    trace, span, execution or workload-scoped group identity for the question.
    Trace/span/execution/group fields remain domain-specific, not mandatory
    columns on every metric or custom row.
  - Existing Bifrost table permissions authorize reads; a correlation key or
    observation graph grants no additional read access or implicit row policy.
  This replaces resource-derived Service attribution and the result/input/drift
  owner-column spelling with the upstream canonical fields; no duplicate root
  column is added. Upstream owns ingest, schema, authorization and durable
  origin production; dependent UI work waits for it.
- **REQ-024 — Kind-specific Card pages.** Each kind has its own layout and
  only real sources:
  - Agent: `invoke_agent` spans with `subject_card_uid` = the Agent (count, latency,
    `status_code` errors, recent invocations linking to traces); gateway calls
    with `subject_card_uid` = the Agent (tokens, cost); composition from the spec.
  - Prompt: spec only — messages, derived variables, response format, using
    Agents, version diff. No telemetry strip.
  - Model: interface, signature, artifacts; spans/metrics attributed to the
    Card grouped by name, shown only when present; drift from bound Verifiers.
  - Data: schema, `stats`, `splits`, `target_columns`, `sql`; per-feature drift
    from `bifrost.drift.result_features` where a drift check names it as
    `baseline_ref`.
  - Service: Overview (signals by `workload_card_uid`, broken down by component),
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
  over the list so filters and position survive. No duration, hash, retry
  internals, provenance, or table-name detail anywhere. For an execution already
  addressed by its existing execution ID, readback distinguishes a pending/
  running/retrying execution with no selected result from an empty result list
  or a completed judgment. Do not add queued executions to the recorded-results
  list or introduce an execution-list API. Show the addressed execution
  as waiting/in progress without inventing a verdict or intermediate result.
  The server alone owns evidence-availability retries; the UI neither resubmits
  verification nor exposes attempt counters or backoff controls. Direct missing
  evidence and exhausted queued evidence render the recorded trace-requirement
  inconclusive reason; other requirements retain their recorded outcomes and
  the Verifier's recorded aggregate decision.
  - List: WHERE bar, time picker and Live; two brushable charts (results
    stacked by recorded verdict; results by Verifier); left filter rail with
    counts (verdict, Verifier, failed requirement, Workload, Card, session);
    table Time, Verifier, Card, Workload, Run, Session, Failed requirements,
    Verdict. Group by none | run | Card | Workload | session. A session group
    header identifies the owner-scoped group and its recorded reviews. Opening
    a review shows that execution's selected recorded result and pinned member
    executions, never every historical row sharing the caller's `group_id`.
  - Result drawer, with "Expand" to a shareable full page:
    - Run result: the Verifier graph (REQ-027) coloured by this result's
      recorded outcomes, first failed requirement selected; under it the
      selected requirement's reason, actual, citations (jump to input or
      trace span), and judge; header verdict, Card, Workload, run, session,
      time; Inputs collapsed.
    - Session result: the same graph with that execution's pinned member
      executions as the input column, each opening its own result. Later group
      reviews remain distinct; an application Run is not a Verifier execution.
    - Drift result: one row per feature (recorded score sparkline with the
      recorded threshold dashed, score, threshold, verdict), every feature
      shown, drifted first; window header with previous/next.
  - The Verifier name opens the Verifier Card; a trace citation opens Traces
    at its exact trace/span identity under the viewer's table permissions. Do
    not apply the originating result list's time/workload/subject filters to
    the evidence read: a trace may cross workloads and include untagged spans.
    Keep those list filters when returning to the list. Bounded-read failures
    remain visible rather than presenting a truncated trace as complete.
    Inspection shows currently retained authorized spans; it does not claim to
    reconstruct the exact span collection seen by a past verification attempt.
    Late-arriving spans never recompute or revise the recorded result; a new
    judgment requires a new server verification execution.
- **REQ-027 — Verifier Card: the Verifier graph.** One component built only
  from `VerifierSpec`: inputs, then steps layered by the outputs they read
  and `depends_on` (edges labelled by the output read), step requirements
  under their step, then Verifier-level requirements. The Card shows it
  plain, with description, version, where it runs, "Results in Verify"
  (Verify filtered to this Verifier; the Card has no Results tab), and a
  YAML toggle. The result drawer reuses it coloured by recorded outcomes.
  - Sessions use current main's optional `group_id`, scoped to the tenant and
    validated exact workload root, not the stable account. `group_end {after}`
    is a quiet-period trigger,
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
    `bifrost.verification.results`; the UI never computes it. The drift view
    stops at recorded scores; no baseline distribution is drawn until one is
    recorded.
- **REQ-028 — Observe pages.** Observe and Verify stay separate: no verdict or
  verification content on Traces, Logs, or Metrics. Traces mode lists one row
  per trace (root span), with span/error counts fetched for visible rows only;
  Spans mode is a flat span list. Live mode pauses while the user scrolls,
  hovers, or has a trace open, and shows "↑ N new". A trace opens in a drawer
  over the list with a waterfall collapsed below two levels and span detail
  beneath (messages for LLM spans). Filters are open by default and writable
  into WHERE, and include Workload (`workload_card_uid`) and Card
  (`subject_card_uid`), with nullable attribution preserved. These filter the
  list; opening an exact trace follows REQ-026's evidence-navigation rules.
  There is no trace-summary table. Traces, Spans and Logs lists sort on the
  server by a clicked column and page with keyset cursors (revision 23).
  - Signal fields follow the upstream telemetry supplement and returned table
    descriptions. Keep resource-service/environment labels separate from Wyrd
    ownership, expose the new resource/URL/exception fields through applicable
    facets or record details, and preserve typed raw attributes and nested data.
    The shared envelope does not imply that all tables have every signal field.
  - Use signal event time for event charts and time filters:
    `start_time_unix_nano` on spans and `time_unix_nano` on logs/metrics remain
    UTC nanosecond timestamps; `duration_nano` remains integer nanoseconds.
    `wyrd_ingested_at` is admission time, not a replacement event-time axis.
  - Span `is_exception` reflects an exact-name exception event; log
    `is_exception` reflects a non-null promoted exception string, including an
    empty string. Error status/severity alone does not imply an exception. The
    UI consumes the server projection rather than recalculating these fields.
  - Metrics retain point kind, integer versus double values, temporality,
    monotonicity, histogram/summary structures and exemplars. Preserve absent
    versus zero and integer precision through supported encodings; do not
    coerce every point to a generic float, invent percentiles, or manufacture
    trace correlation for a metric aggregating many executions.
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
- **REQ-030 — The component library and its UI hosts.** The
  component library is the most important deliverable of this change. Every in-scope component works in use cases 1–3 below. The fourth host is
  retained as future context, not a delivery or acceptance dependency. References
  in unchanged TASK-003 material to four-host foundation proof retain their
  existing renderer/BFF checks; they do not require an Ask service. The two
  primary deployed journeys are conventional UI and agent-harness MCP Apps
  (REQ-034/035); custom-app portability remains required.

  | # | Use case | Who produces the view | How it is drawn | Who supplies the data |
  |---|---|---|---|---|
  | 1 | Shipped UI (conventional) | Wyrd writes the page | SvelteKit page; data loads run in the BFF | BFF, with the signed-in browser session's credential |
  | 2 | A user builds their own UI with an agent (e.g. Claude) | The agent writes Svelte code using `wyrd-ai-ui`, or writes a view document | The user's own app | The user's server, with the user's own Wyrd credential |
  | 3 | MCP App ("why did this fail? show me") | A Wyrd MCP tool returns a view document | The MCP client hosting the chat (Claude, VS Code, ChatGPT, …) draws `wyrd-ai-ui` inside its sandboxed iframe | The MCP App calls Wyrd's existing read tools as the same MCP caller |
  | 4 (deferred) | Runtime UI in Wyrd (A2UI-style) | An agent answers a question in the Ask panel | The same view-document renderer, inside the shipped UI | BFF, with the signed-in browser session's credential |

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
    `wyrd-ai-ui/loaders`, and run in all in-scope hosts;
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
  | `cards.get_by_ref {card_ref: CardRef}` | `cards.get_by_ref` with the exact flat reference | `Cards.get(card_ref)` |
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

  **Schema and catalog adoption.** Built-in, dynamic-dataset and operational
  SQL references use the delivered public `bifrost` catalog. Table discovery and
  description consume the same delivered contract on Node/MCP/BFF, including
  resolved schemas, layout and revisions; they do not impose a local fixed
  signal-row schema. Query remains a read-only workbench: layout configuration,
  schema alteration and revision-based PATCH administration are not added.
  Adopt changed generated contracts from their existing source owners. Added
  signal columns do not justify replacing Chart, executors or normalization.

  Built-in SQL, domain loaders, authored examples and recorded/live fixtures
  must agree with this catalog and the canonical stored fields. Regeneration
  alone does not update SQL strings. Preserve authored SQL verbatim and show
  the existing structured query error for an obsolete reference; do not rewrite
  it, fall back to an old catalog, or support both schemas. Dependency fixtures
  remain explicitly non-live until the upstream contract and real proof land.

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
    case 1 uses the request-scoped Rust-owned browser session from REQ-033,
    forwarding its selected cookie and trusted origin through the shared client
    HTTP transport, without Node minting, caching or renewing OAuth credentials;
    use case 2 builds it in the user's server from their credential
    (`WYRD_API_KEY`, workload token, or saved login). If the shared client cannot
    project that request-scoped session transport, its smallest HTTP extension
    and TypeScript projection belong to foundation, not a second UI client or
    session authority. Session cookies remain server-only and are not promoted
    to a general SDK credential or gRPC/MCP authentication mode.
  - MCP executor (`wyrd-ai-ui/mcp`): calls the MCP tools above through the
    ext-apps `App.callServerTool`, so reads run as the MCP caller with that
    caller's scopes. It makes no network request of its own.
  - In the shipped UI's browser (gallery and view-document consumers), the renderer uses a BFF
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
      {"id": "p1", "component": "Chart", "layout": {"x": 0, "y": 0, "w": 8, "h": 6},
       "input": {"type": "timeseries", "sql": "SELECT …", "x": "t", "y": ["failed"]}},
      {"id": "p2", "component": "VerificationResult", "layout": {"x": 8, "y": 0, "w": 4, "h": 6},
       "input": {"resultId": "…"}}
    ]
  }
  ```
  - `range` is either absolute `{from, to}` or relative `{last: "6h"}`
    (REQ-031). Panels inherit it unless their input sets one.
  - Layout (REQ-036): a panel's optional `layout` places it on the view grid.
    It is the only layout language; there is no `width` field.
  - A view document never contains query results, props, credentials, or
    tokens. It is re-run as whoever views it, including an unsaved view.
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

  **Use case 4 — Ask panel (deferred).** A future AskPanel would send a
  question to a separately approved agent-backed server endpoint and render its
  text-plus-view answer. This change does not implement that endpoint, an
  embedded agent runtime, conversation UI, or fixture AskPanel. The user's
  existing harness owns conversation and model execution for use case 3.
  Neither Ask nor saving is needed to generate, inspect, update or discard an
  unsaved MCP App view.

  **Component inventory.** These are the components the approved canvas
  needs. "Prereq" means the data source is a platform prerequisite not yet
  on main; the component is still built and tested against fixtures shaped
  by that prerequisite's contract, and only its live loader waits.

  | Component | Canvas boards | Input | Data source | Prereq |
  |---|---|---|---|---|
  | `Chart` (timeseries, bar, stat, table; title, source caption, Open in Query, in-chart error) | Main, Spans, Logs, Metrics, Verify*, ServiceCharts, QueryChart, Model, Data | chart definition (REQ-025 fields) + range + bucket | its own SQL on `bifrost.*` tables | Service charts: REQ-025 |
  | `StatStrip` (1–5 numbers with sparkline) | Agent, Service, Home, Main, Spans | stat definitions + range + scope (`subject_card_uid` or `workload_card_uid`) | `bifrost.traces.spans` (`invoke_agent`, `duration_nano`, `status_code`), `bifrost.gateway.calls` (`usage`, `cost`) | `workload_card_uid`: REQ-023 |
  | `FacetRail` (value counts, click to add to WHERE) | Main, Spans, Logs, Verify* | `{signal, where, range, facets}` | GROUP BY counts on the list's table under the same WHERE | session facet: `group_id` |
  | `RecordTable` (typed cells, live mode, group by) | Main, Spans, Logs, Verify*, Agent, Query | `{signal \| sql, where, range, columns, groupBy, live, limit}` | spans (one row per root span), `bifrost.logs.records`, `bifrost.verification.results`, or SQL | session grouping: `group_id` |
  | `TraceWaterfall` | TraceDrawer | `{traceId, range?}` | `bifrost.traces.spans` by exact `trace_id`; range is navigation context, not an implicit evidence predicate | — |
  | `RecordDetail` (span or log: Messages, Attributes, Events, Exception, Raw) | TraceDrawer, Logs | `{traceId, spanId}` or log key | span `attributes`, `events`, `exception_*`, `gen_ai_*`; log `body`, `attributes` | — |
  | `CardHeader` (name, kind, version select, status, uid copy, tabs, primary action) | all Card boards | `{cardRef}` | Card envelope + registry version list | — |
  | `RelationshipList` | Agent, Prompt, Model, Data, Service | `{cardRef}` | server-derived `relationships` | — |
  | `CardFacts` (key/value) | all Card boards | `{cardRef, fields}` | Card `metadata` and `spec` | — |
  | `CompositionGraph` | Agent, Service | `{cardRef}` | `AgentSpec` / `ServiceSpec`; node badge = latest recorded verdict | — |
  | `PromptMessages`, `PromptVariables`, `UsedBy`, `VersionDiff` | Prompt | `{cardRef, otherVersion?}` | `PromptSpec`; relationships | — |
  | `ModelInterface`, `ArtifactTable` | Model | `{cardRef}` | `ModelSpec` interface/signature; artifacts | — |
  | `VerificationBindings` | Agent, Service, Home | `{cardRef, range}` | bindings + `bifrost.verification.results` | — |
  | `AttentionList` | Home, Service | `{scope, range}` | failed `bifrost.verification.results` by `workload_card_uid` | CR rows: REQ-029 |
  | `SignalBreakdownTable` | Home, Service | `{groupBy: workload_card_uid \| subject_card_uid, scope, range}` | root spans + `bifrost.gateway.calls` | REQ-023 |
  | `CardInventory` (list, filters, kinds) and `CardInventorySummary` | Cards, Home | `{filters, page}` / `{}` | `cards.list` | — |
  | `VerifierGraph` (plain or coloured by one result) | Verifier, Verify, VerifySession, VerifyDrift | `{cardRef}` or `{resultId}` | `VerifierSpec`; `outcome` of `bifrost.verification.results` | — |
  | `VerificationResult` (drawer body: header, `VerifierGraph`, `RequirementDetail`, Inputs) | Verify, VerifySession, VerifyDrift | `{resultId}` | `bifrost.verification.results`; drift adds `bifrost.drift.result_features`; existing execution readback supplies pinned group membership | main already supplies `group_id`/`group_end`; non-sensitive requirement outcomes remain a prerequisite |
  | `RequirementDetail` | Verify*, ChangeVerification | `{resultId, requirementId}` | requirement outcome in `outcome` | — |
  | `DriftFeatureTable` | VerifyDrift, Data | `{resultId}` or `{cardRef}` | `bifrost.drift.result_features` | — |
  | `QueryEditor`, `TableCatalog`, `QueryDetails` | Query, QueryChart | `{sql, range, limit}` / `{}` | existing Bifrost list/describe/query contracts; resolved schema, layout and revisions | schema/description cutover |
  | `ChartEditor` | ServiceCharts | `{cardRef, chartId?}` | `ServiceSpec.charts` | REQ-025 |
  | `ChangeRequestList`, `ChangeHeader`, `SubjectsTable`, `RevisionList`, `ActivityFeed`, `ChangeVerifierList`, `RequestVerificationForm`, `ReviewSubmission`, `Participants` | Changes, ChangeDetail, ChangeVerification | `{crId, revision?}` | none on main | REQ-029 |
  | `RepositoryList`, `WyrdTomlView`, `VerifierFolderTable` | Repos | `{repo}` | none on main | REQ-029 |
  | `AskPanel` (deferred; excluded from shipping inventory) | future rail entry | future contract | none required in this change | separate future scope |

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
  - T6 page journey: added when the page is assembled; REQ-034 requires a real
    browser navigating the deployed page, not only imported handlers or jsdom.
  Fixtures for `populated` and `empty` are recorded from T5 runs by a script
  and checked in; `error` and prereq fixtures are hand-written and labeled.
  T1–T6 and deterministic MCP/browser regressions call no live model. REQ-035
  adds a separate, explicitly invoked live-agent journey using existing local
  readiness configuration. It is outside default tests, `gate` and unattended
  CI; absence of credentials, a compatible host or browser is BLOCKED, not PASS.
  Integrated change acceptance requires its recorded evidence. One successful
  stochastic run establishes that run only, not a reliability claim.

  **Gallery (visualized).** A dev-only route `/_gallery` in the shipped app
  renders every catalog component in every state, light and dark, through
  `WyrdView` from the fixture view documents, each labeled with the canvas
  board it must match. The human inspects every component against
  `references/canvas/png/` on desktop and mobile, in light and dark, before
  pages are built, except REQ-039's early live slice. Record component-level acceptance with the task evidence;
  accepting a family does not waive inspection of its individual components.
  Automated checks supplement human visual inspection rather than replace it.

  **Build order (caller, 2026-10-19; early live exception in revision 15).**
  REQ-037–040 and TASK-004-REV-02 may run after the existing foundation and Observe
  implementation, before the remaining families and step 4. The slice's actual
  browser workflow supplies its contextual visual checkpoint; existing required
  trace-component implementation is a prerequisite, but unrelated family or
  StatStrip gallery acceptance is not. Required correctness/review still applies.
  The gallery remains supporting state/visual evidence. This exception does not waive the
  full-library shipping gate, remaining family proof or final MCP App journey.
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
     (`SignalBreakdownTable`, `ChartEditor`); Changes and Repos. AskPanel
     and saved-view controls are deferred and excluded from this shipping gate.
  4. MCP App: resources capability, `ui://wyrd/view`, `ui.show_view`,
     `ui.catalog`, and the complete REQ-035 agent investigation journey.
  5. Only then the conventional pages (REQ-007 to REQ-019, REQ-021,
     REQ-024 to REQ-029), each assembled from finished components with its
     real-browser T6 journey (REQ-034). A page whose prerequisite has not
     shipped waits for it. Other platform prerequisites remain blocking; only
     Ask/saved views are removed from this delivery gate.

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
  - Verify's Workload filter and column read `workload_card_uid` from
    `bifrost.verification.results`, written by the server from the exact binding
    root (always a Service). Subject, workload and account stay separate per
    REQ-023. Ownership is never derived at read time from relationships or
    resource labels.
  - Data per-column drift: the loader reads the Verifier specs whose drift
    check's `baseline_ref` names this Data Card, collecting each Verifier and
    requirement id, then queries `bifrost.drift.result_features` filtered by
    that Verifier and requirement id. No schema change.

- **REQ-036 — View layout: placed, resizable panels.** A view document
  lays its panels out on a grid the way a dashboard does, and a reader can
  resize and move them. The layout lives in the document; the components do
  not know their size.
  - Grid: 12 columns spanning the view's width; rows are 40 CSS px. A panel's
    `layout` is `{x, y, w, h}` in whole grid units: `0 ≤ x ≤ 11`,
    `1 ≤ w ≤ 12`, `x + w ≤ 12`, `y ≥ 0`, `1 ≤ h ≤ 48`. A value outside these
    bounds fails document validation.
  - A panel without `layout` spans all 12 columns at its content height and
    is placed below every panel before it in document order. Documents may
    mix placed and unplaced panels.
  - Panels never overlap on screen: where two layouts collide, the later
    panel in document order moves down until it is clear. The document is
    not rewritten by rendering.
  - Components fill the box they are given. Content taller than its panel
    scrolls inside the panel; it never draws over a neighbour. No catalog
    component entry gains a size input; only the catalog's published
    view-document schema gains `layout`.
  - Narrow views: when the view is narrower than 720 CSS px, panels render as
    one full-width column in `(y, x)` order and keep their `h`; resize and
    move are off.
  - Editing: `WyrdView` makes layout editable only when its host passes a
    layout handler. Then a reader resizes a panel from its lower-right corner
    and moves it by its drag handle; both snap to the grid, both have
    keyboard equivalents (move with arrow keys, resize with Shift+arrow keys
    on the focused handle, Escape cancels), and each completed change calls
    the handler with the whole updated document. Without a handler, layout is
    read-only. Hosts decide what to keep; reloading a document shows its
    written layout.
  - Persistence: none in this revision. Saving an edited layout arrives with
    saved views (REQ-031); `ServiceSpec.charts` (REQ-025) gains no layout
    field. In an MCP App, edits last for that render only.
  - A2UI export (REQ-030) emits panels in `(y, x)` order and may drop
    `layout`; layout never blocks that mapping.
  - Non-goals: adding, removing or duplicating panels from the view;
    collapsible rows; per-viewer layout memory; layout in the shipped pages
    (use case 1), whose pages own their own layout.

- **REQ-031 — Saved views (deferred, independent future capability).** A saved view is a view
  document (REQ-030), never stored data. The remaining paragraph preserves
  future product intent only: no saved-view backend, controls, sharing action or
  SDK/MCP projection is required by this revision. Saving is independent of Ask
  and does not block either primary journey. A future approved change must
  define its server contracts before implementation. It is a tenant-scoped server resource,
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

- **REQ-034 — Primary conventional UI journey.** TASK-004-REV-02 establishes real
  browser automation of the deployed production BFF and Rust server using
  repository-managed Postgres, public login and session boundaries. Each
  assembled page extends it with its actual user workflow: sign in, navigate,
  inspect real data, interact, and verify the resulting visible state and server
  reads. Time-aware pages exercise range changes and no stale overwrite;
  identity-aware pages preserve exact Card versions and links. Cover populated,
  empty, denied/error and cross-tenant refusal without fabricated domain data.
  Browser navigation, JavaScript, focus/keyboard and applicable mobile behavior
  must execute in a real engine, which may run headlessly. HTTP-client cookie
  simulation, jsdom, imported handlers and screenshots alone are supporting
  proof, not completion. Preserve per-page human inspection. TASK-013–026 own
  page-specific closure; TASK-012 does not manufacture pages to test them early.

- **REQ-035 — Primary agent-harness MCP App investigation journey.** A user
  completes an unsaved investigation in an MCP App-capable agent harness with
  no visit to the conventional Wyrd UI and no application-building step:

  1. Ask a natural-language question. The real agent discovers `ui.catalog` and
     valid component inputs and calls `ui.show_view` with its generated document.
  2. The actual returned resource/document renders in the harness's sandboxed
     browser app; its reads reach the real server as that viewer.
  3. Send a follow-up in the same conversation. The agent uses that context to
     refine the investigation and return an updated view; inspect its rendering.
  4. Change the time range in the app (including chart brushing where applicable).
     Inspect the new bounds in actual reads and visible results, consistent
     bounds across affected panels and no acceptance of superseded results.
  5. Exercise empty results, permission denials, query errors and malformed panel
     inputs with honest visible outcomes, sibling isolation, tenant separation
     and no credential exposure. Deterministic deployed-host companion tests may
     force these failures; natural-language discovery/follow-up remain live-agent
     proof. Do not require a model to reproduce every injected negative case.

  TASK-011 owns this outcome. Reuse the existing coding-agent readiness owner
  for local configuration, checkout docs, owned `wyrd server dev`, agent process
  deadlines/cancellation, credential isolation and sanitized reports. Extend it
  for actual conversation turns and connect its actual MCP results to a real
  browser-capable App host using the existing ext-apps bridge. Do not implement
  a production chat service or agent runtime in Wyrd. Browser and agent drivers
  must belong to the same connected test session; manually copied documents,
  hardcoded model outputs, fixture AppBridge answers, or independent CLI and
  browser runs cannot satisfy this journey.

  Record the exercised host/agent versions, source identity, conversation turns,
  actual tool requests/results, browser interactions, visible outcomes and
  authorized server evidence with secrets removed. Claim only exercised hosts.
  Do not assume Claude Code/Codex CLI execution implies MCP App rendering. An
  external host may be used only through its supported automation/extension
  boundary. A minimal repository test host is acceptable only when it connects
  the real agent conversation and real MCP connection to the actual AppBridge
  lifecycle in a browser; it proves that host, not compatibility with an
  untested commercial client. Host wiring/runner layout are reversible test
  choices; missing supported integration blocks the live journey rather than
  authorizing a replacement production harness or security bypass.

### Live local development and Observe delivery — revision 18

- **REQ-037 — Optional local MockServer.** Task 01 starts official MockServer
  with static OpenAI-compatible application/tool/judge expectations, bounded
  initializer-aware readiness and owned cleanup. Use the existing development
  OpenAI endpoint override and ordinary dummy credentials. All app and queued
  judge model routes in this workflow resolve there; no external inference or
  real-provider fallback is configured. Unmatched requests fail visibly.
  Expectations are repeatable under concurrent requests, not consumed scripts.
  Retain ordinary provider identity (`openai`), usage/pricing/budget behavior,
  authentication, authorization, routing, admission, capture and settlement.
  This is optional dev tooling, not a new provider enum, mock deployment/key/
  tenant, SDK scope/header, billing policy, evidence field, migration or queue
  feature. Production endpoint restrictions stay intact. Existing real-provider
  development remains available outside this fixture-backed workflow.

- **REQ-038 — Canonical development workload.** `mise run dev:full` starts the
  existing real server/database/UI lifecycle, Task 01's fixture service and
  canonical `bohmian-ai/wyrd-example-py`, provisions normal tenant/Cards/provider
  configuration, and runs 500 application HTTP requests at concurrency 8 by
  default. Allow smaller bounded runs and `dev:batch` against the same running
  environment. No paid model key, synthetic server, fabricated evidence rows,
  benchmark framework or direct writes to protected Bifrost tables are required.

  Pin canonical source initially to `03428e1a4a35c8f23f71a95cae08e956ca317c9b`.
  Deliver application/batch/telemetry changes as a reviewable patch outside the
  active change packet, apply it to a clean pinned checkout and record effective
  source identity. Do not copy a second app into Wyrd or silently follow a dirty
  checkout/mutable branch. Local patch delivery needs no upstream publication.
  Build/import this checkout's host-compatible production SDK wheel through
  the canonical vendor source and prove artifact origin/digest, not just version.

  The default workload is 400 shipping asks, 50 refund asks, 25 model-free
  GET /openapi.json requests and 25 unknown-order asks returning HTTP 404.
  Actual application tools run; direct refund checks fail as expected and still
  submit queued checks. The local judge returns a valid scripted passed verdict
  with a reachable order /price citation, not a claim of evaluation quality.
  No driver retries enlarge the denominator. Report HTTP outcomes, Runs,
  immediate/queued checks, attempts and retained spans/logs/metrics separately.
  Public bounded execution waits and signal readback determine completion;
  missing evidence, unexpected errors or nonterminal work fail the batch.

  Emit observations through existing SDK-installed OTel providers, including
  ordinary HTTP-only/error traces and application logs/metrics. Keep one app
  Run owner; missing attribution stays null. Use normal export while running
  and graceful public shutdown drain, not a development flush endpoint.
  Start the batch once per backend lifecycle; Vite reload does not rerun it.
  Preserve/disclose current ephemeral reset and credential replacement, keep
  healthy services available after setup/batch failure, and clean up only owned
  children on interruption. Add no new reset/schema-repair/persistence workflow.

- **REQ-039 — Fully usable live Observe slice.** Task 02 owns the complete
  shell, Traces/Spans, Logs and Metrics outcomes formerly assigned to
  TASK-012/020/021/022. Use `/t/[tenantKey]/observe` with shareable
  `signal=traces|logs|metrics`, plus `mode=traces|spans` for traces. Default to
  traces. Preserve real login/logout, tenant/settings, theme and honest disabled
  future navigation; no gallery/fixture fallback in production data paths.

  Range/timezone, applied SQL predicate/facets, grouping, mode, selected exact
  record and return state remain shareable/restorable. One resolved range and
  predicate drives the selected signal's charts/facets/list. Supersession and
  cancellation cannot overwrite newer state. Live hold/pause/resume/new-row
  behavior preserves investigation. Traces retain root versus flat semantics,
  exact counts, waterfall/selected span/recorded messages/attributes/exceptions,
  and authorized untagged/out-of-list-range evidence. Logs include actual
  severity/body/attributes, timeline, wrap/paging/columns and attribute filter/
  exclude/open-trace actions. Metrics include selector/grouping/chart/legend,
  selectable series summaries and line visibility with truthful recorded kinds,
  units, temporality, precision, histogram/summary shapes and missing values.
  Unsupported plots/aggregates remain unavailable; no fabricated percentiles.

  Reuse shared components/loaders through authenticated BFF/executors; pages own
  host/layout state, not SQL or domain aggregation. Extend missing shared
  Metrics semantics without a second data service or wire encoding. Preserve
  exact displayed integer values through existing query projection/result
  capabilities; approximate geometry cannot silently replace exact evidence.
  Complete chart Open in Query at `/t/[tenantKey]/query` with exact SQL/typed
  binds, authorized bounded rerun/cancel and truthful terminal/error states.
  Full Query catalog/History/chart authoring remains outside this task.

  Carry all TASK-004/R1 obligations and proof into Task 02, including generic
  names/aliases, typed grouping, component fidelity, reusable Logs/Metrics and
  StatStrip presentation. Real-browser automation and individual desktop/mobile,
  light/dark human acceptance cover Traces, Spans, Logs, Metrics and named
  components. Gallery proof supplements, never substitutes for live data.
  This slice precedes unrelated families/production MCP registration without
  waiving their final obligations. TASK-004 is incomplete until this delivery
  and its independent review/visual acceptance finish.

- **REQ-040 — SQL predicate filtering with trace semantics.** WHERE accepts an
  optional single SQL predicate over the selected signal table's described columns, not a replacement
  SELECT. It supports parentheses, AND/OR/NOT, scalar comparisons, IS NULL /
  IS NOT NULL, IN, BETWEEN and LIKE/ILIKE with literals. Statements, subqueries,
  relation changes, parameters supplied inside the predicate, comments and
  unsupported constructs fail visibly. Empty input means no extra predicate.
  Applying invalid input retains the last successful data with an explicit
  validation error; the draft is not marked applied. The predicate cannot
  escape the selected range, tenant/table authorization or read/result bounds.

  Add one narrow read to the existing query/catalog owner:
  `POST /v1/query/predicate` and MCP `bifrost.validate_predicate`, projected
  through `wyrd-client::Bifrost` and the three first-class SDKs. Request fields
  are `namespace`, `name`, `expression`; response is normalized `expression`.
  The expression has an 8 KiB UTF-8 ceiling. Table visibility/query permission
  is checked before schema details are exposed. The server reuses its installed
  SQL parser, requires expression-end-of-input and validates column references
  against the described table. It returns canonical expression SQL or the
  existing stable validation/denial error, not an executable result or token.
  This adds no durable state or second query service. Full queries still pass
  existing canonical execution authorization/validation on every execution.

  Shared signal inputs carry optional `where_sql` beside existing structured
  `where` filters. Portable loaders validate the expression through that read
  before composing dependent chart/facet/list requests; parenthesized SQL
  composition retains the fixed template, typed binds and bounds. A failed
  validation issues no dependent reads. Node/custom-app and MCP executors
  project the same contract. Generic `RecordTable.sql` remains generic; it is
  not a substitute for trace-mode filtering/navigation/counts. Literal values
  in facet filters remain bound and do not rewrite the author's SQL text.

## Invariants, constraints, and non-goals

- **INV-001 — Current contracts.** Current `wyrd-spec` and architecture own
  vocabulary and behavior. Fourteen registrable native Card kinds; no Workflow,
  Eval, Tool, Skill, SubAgent, Bifrost, or External registration. Verifier is the
  generic verification Card; drift belongs within its check model. No legacy routes,
  compatibility aliases, per-kind verification models, or replacement contracts.
- **INV-002 — Server and credential boundary.** Rust retains durable behavior,
  authorization, tenant isolation, storage, policy, audit and browser-session
  authority. Node retains only the request-scoped opaque cookie required for
  server-side forwarding. Revision 19 proposes only the explicitly described
  transient no-store personal-key creation response and deliberate ephemeral
  development-password delivery exceptions; no general credential cache.
  Otherwise never emit token/API-key material in browser cookies,
  page data, browser storage, logs, URLs, screenshots, or fixtures; session
  identifiers appear only in their protected cookie and transport. A failed
  or denied live request
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
  REQ-034/035 authorize the minimal test-only real-browser dependency and host
  automation needed for those journeys, not a new test framework, production
  setting or environment selector. Reuse the existing readiness CLI/local
  configuration and ordinary runner selectors; provider credentials remain local.
- **INV-005 — TASK-001 exclusions.** TASK-001 ends at packaged/installed/launched
  UI, official container routing, lifecycle, REQ-033 server-owned browser
  sessions, and real login proof. Revision 10 authorizes the necessary auth,
  SQL, wire-contract, metadata/OpenAPI and retained UI-consumer closure; it
  does not authorize a domain client or analytical SDK integration.
  It must not onboard Cards, Bifrost, Query, Observe, verification results,
  generated HTTP clients, or analytical native addons. It proves login and
  session with the authenticated surfaces REQ-032 keeps (`t/[tenantKey]/`
  signed-in page and `settings`). It may land before or after REQ-032; if
  before, REQ-032 must keep its login proof passing. Do not install a domain
  client to prove auth.
- **INV-006 — Useful migration only.** No whole-branch merge, redesign of Wyrd,
  new Workflow Card, general mock system, or new feature inferred solely from a
  canvas. REQ-033 is the explicit caller-approved browser-session ownership
  correction, not permission to redesign unrelated auth or machine protocols.
  The Data workspace and Change Request UI are caller-requested
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
   REQ-033 replaces Node session authority with Rust-owned persisted browser
   sessions. This uses the existing database, not additional deployment secrets
   or infrastructure; the provider/CLI/SDK authentication owners remain.
4. Main's existing production identity journey is reused and extended where its
   synthetic `/auth/*` routing currently bypasses production routing. Native
   installation/lifecycle and Linux image routing have separate proof owners.
5. UI domain reads remain BFF-side against delivered public server contracts.
   The first actual consumer owns the smallest necessary client integration and
   its consumer closure; later pages reuse it. Revision 14 adopts the approved
   upstream catalog, envelope and signal schemas only with their owning
   architecture/SDK/MCP closure. Revalidate affected SQL, examples, fixtures and
   proof across in-scope hosts without introducing parallel schema support.
6. Plan real dependency edges and one observable outcome per task. TASK-001 is
   the caller-required plumbing/login closure. Later domain work is not a
   dependency of it. Visual approval governs dependent execution, not a new
   persisted change-state controller.

## Acceptance and evidence classes

| ID | Observable acceptance | Credible evidence |
|---|---|---|
| AC-001 | A verified native install launches the production BFF and Rust server without a source checkout on each supported OS/architecture; missing/incompatible Node and malformed packages fail usefully and preserve a working install | Existing build/release and installer owners, package contents/runtime smoke, native release matrix evidence; no Node executable bundled |
| AC-002 | `wyrd server dev` reaches UI readiness and handles first/repeat runs, conflict, partial startup, interruption, and child death without abandoned owned processes or data loss | Existing local-dev journey/installed CLI owners, focused process regressions, public UI smoke |
| AC-003 | Tenant browser SSO/recovery, callback, chooser, switching, settings and logout work at native public origin with REQ-033; existing platform and machine flows remain unchanged; invalid origin/state/issuer/tenant remain refused and no API key or OAuth token reaches browser state | Existing Keycloak/Dex and production UI journey through actual routing; focused platform/CLI regressions |
| AC-004 | Official images serve UI/BFF and existing Rust HTTP/gRPC surfaces through nginx at the intended public origin, with correct readiness, errors, and three-process cleanup | Existing Linux startup dev/prod owners against pinned image ID and source revision; production identity exercise through nginx |
| AC-005 | TASK-001 imports no deferred domain/client/native analytical integration and adds no prohibited build/test configuration | Actual diff/manifest review and relevant existing dependency checks |
| AC-006 | Card inventory and exact detail show current tenant data, exact versions, correct links and current kinds with honest empty/denied/error states | Focused BFF/UI tests and real registry journey plus separate inventory/detail visual checkpoints |
| AC-007 | Agent, Prompt, Model, and Service workspaces faithfully project their current specs and relationships | One focused consumer journey and human light/dark inspection per workspace |
| AC-008 | Query, traces, logs, and metrics each read current canonical data, respect authorization/time bounds, and do not present failed/partial reads as success | Existing Bifrost/identity fixtures and narrow UI consumer journeys, terminal/cancellation regressions, separate page inspections |
| AC-009 | Verification screens show generic requirements/verdicts/evidence and correct subject/Verifier/execution identities; summaries agree with those retained facts | Real generic verification-to-UI journey and focused projection tests; separate verification and summary checkpoints |
| AC-010 | All original draft/reference bytes remain available with provenance; every task is self-contained, source-backed, narrowly verified, and visually inspectable; independent task readiness findings are resolved or surfaced | Preservation hashes, evidence inventory, obligation/task map and independent plan-readiness-reviewer findings/dispositions |
| AC-011 | Every in-scope inventory component (excluding deferred Ask/saved views) is fully implemented, tested, and individually accepted on desktop/mobile in light/dark before library shipment; REQ-039 permits one early live slice with contextual individual inspection before other conventional page work; unmet platform prerequisites block completion rather than disappear behind fixtures | Per-component interaction/accessibility/state evidence, applicable live authorization journeys, gallery evidence and the early slice's individual live-page acceptance |
| AC-012 | Rust-owned opaque sessions support SSO and recovery, reject tampering/expiry/revocation/tenant mismatch and CSRF, preserve safe metadata/settings/chooser/logout, continue across two replicas/restarts, and require no additional deploy secret/env/service; old credential cookies require re-login | Real two-replica native and nginx browser journey, SQL/session negative tests, served cookie-auth contract, source/config audit and unchanged bearer/device/platform regressions |
| AC-013 | Conventional UI users complete each page's real workflow in a browser against deployed BFF/server, with truthful range/identity, failure and tenant behavior | REQ-034; TASK-004-REV-02 browser foundation/Observe journeys and remaining page journeys, plus per-page human acceptance |
| AC-014 | A real agent conversation produces an MCP App view, refines it on follow-up, and supports in-app range reloads under the viewer's permissions without opening Wyrd UI or building an app | REQ-035; TASK-011 gated live agent plus real-browser host evidence and deterministic deployed-host negatives; missing credentials/host are BLOCKED |
| AC-015 | A view document places panels on the 12-column grid; with a host layout handler a reader resizes and moves panels by pointer and keyboard with grid snapping and no overlap, the handler receives the whole updated document, charts fill their resized panels, narrow views stack, and a document without `layout` renders as before; out-of-range layouts fail validation | REQ-036; unit tests for schema bounds, collision resolution and handler output; real-browser gallery and AppBridge artifact evidence at desktop and mobile in light and dark; recorded human gallery acceptance |
| AC-016 | After the upstream cutover, discovery, descriptions, built-in SQL and shipped examples/fixtures agree on the `bifrost` catalog and canonical fields in each in-scope host; obsolete authored SQL fails visibly without rewriting or fallback | REQ-014/030; focused SDK/MCP/BFF query and description journeys, example smoke and affected foundation proof, including denied and obsolete-reference cases |
| AC-017 | Two releases sharing a component and stable account remain distinct by workload; component and account queries select their own dimensions; rootless/subjectless rows keep truthful nulls and correlations grant no read access | REQ-023/024/028; real accepted telemetry/results with different workload, subject and account identities, exact-release queries, null cases and existing tenant/table-denial proof |
| AC-018 | Observe preserves signal-specific types, event/admission time, integer precision, missing values and returned exception semantics after schema adoption | REQ-022/028/030; recorded typed signal fixtures and live reads covering distinct timestamp units, integer/double points, histogram/summary/exemplar data, nullable promotions and exception-versus-error cases |
| AC-019 | Verification renders waiting separately from recorded outcomes, keeps selected results and workload-scoped membership exact, opens authorized cross-workload/untagged trace citations beyond originating list filters, and preserves completed judgments after late spans arrive | REQ-018/026/027; real queued-arrival, direct-absence and late-arrival consumer journeys plus focused citation/read-denial and pinned-membership checks; no client-owned retry or presumed trace snapshot |
| AC-020 | Initialized local MockServer serves ordinary gateway app/tool/judge calls concurrently with explicit unmatched/setup failures, existing provider/accounting behavior and no external inference | REQ-037; real gateway fixture journeys, startup/readiness/cleanup and static unchanged-contract proof |
| AC-021 | Normal dev:full runs the pinned canonical example with this checkout's production SDK, dispatches 500 requests and confirms actual retained spans, logs, metrics and terminal checks while UI remains usable | REQ-038; source/wheel identity, exact case accounting, public waits/readback and lifecycle/failure proof |
| AC-022 | A signed-in user investigates the batch through the real shell and Traces/Spans, Logs and Metrics, including range/SQL/facets, live holds, exact details/links and truthful metric semantics | REQ-039; production-BFF/server real-browser journeys, all transferred page/component scope and individual desktop/mobile light/dark acceptance |
| AC-023 | The same valid SQL predicate governs signal charts/facets/lists in Node and MCP without losing counts/navigation; malformed, injected, unauthorized or out-of-schema input fails before dependent reads | REQ-040; pure parser and portable-loader regressions plus real first-class validation/read journeys and adversarial browser cases |

Task planning must choose exact focused commands from inspected current owners;
the evidence document identifies known commands and coverage gaps. No build,
test, or runtime success is claimed by this specification. Final integrated review owns
combined proof and cross-task seams; task gates remain bounded to their owners.

## Open material decisions and approval

Revision19 is draft. The caller has approved the requested outcomes but has
not yet approved the concrete workspace-provisioning/enrollment, security,
persisted-key/session and light-palette proposals above. Task03 is proposed,
not ready. Existing revision18/earlier approval records below remain historical
and do not approve revision19. Approve the proposed admission/security/visual contract as a complete
revision before implementation; no user/admin role may be inferred at signup.

The caller's 2026-11-02 instruction approves revision 18's replacement local
MockServer and complete live Observe outcomes. New implementation tasks remain
proposed; no implementation, review or visual PASS is implied. There is no open
mock-selection/judge-propagation decision: the existing dev endpoint override
serves both. The full Logs/Metrics pages are explicitly included in this slice.

Revision 14 approves the catalog/schema consumer alignment,
managed attribution and Workload labels, signal fidelity, and verification
waiting/evidence behavior described above. The service packet's approved
contracts supply the upstream decisions and are delivered on main; this UI
revision adds no new server API or durable authority.

The caller approved this bounded revision on 2026-10-29. Task instructions use
the delivered `workload_card_uid`/`subject_card_uid` columns and `bifrost`
catalog. Earlier approvals, completed-task records and review subjects retain
their original meaning and are not retroactively rewritten. The foundation's
renewed real-server proof passes on main.

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

PRR-001 through PRR-006 have corrections above; the original review is not
rewritten into a pass. The independent [revision-9 spec re-review](review/plan-readiness-revision-9/review.md)
returned `Approve`, architecture PASS and no blocking findings. Missing separately specified platform contracts
remain real component dependencies and hold the full-library shipping gate;
they must not be invented by the UI implementer. Proposed task review also uses
`$plan-readiness-reviewer`.
Native origin topology and ordinary fixture/private implementation mechanics
remain unchanged. The caller explicitly approved revision 9 on 2026-10-19;
`$wyrd-plan` may derive the replacement tasks. Approval of planning does not
authorize implementation.

Revision 10 supersedes only the browser-session architecture and its necessary
contract/consumer closure, plus the recorded TASK-001 remediation. The caller
explicitly approved this direction by instructing: “Revise the spec with this
decision” and “Write the remediation task to fix the known issues and add
server-owned sessions.” No product/security/deployment decision is left open
for that task; reversible internal types, schema layout and helpers remain
implementation-owned. Revision-9 readiness and task-review reports retain their
original subject and verdict, not an inferred approval of revision 10. The new
remediation and its independent review are still required; no implementation,
platform/runtime pass or human visual acceptance is claimed by this edit.

Revision 11 records the caller's explicit acceptance of the proposal and
`$wyrd-plan` request on 2026-10-20. REQ-034/035 and AC-013/014 define the two
primary automated journeys; deferred AskPanel/REQ-031 no longer hold the library,
MCP or conventional-page gate. All other in-scope prerequisites and human visual
checkpoints remain. TASK-003 production scope and existing tests are preserved;
new deployed-browser/conversation acceptance belongs downstream. Existing
review verdicts are not rewritten. A host lacking supported integration is a
reported live-proof blocker, never an assumed capability or silent pass.

## Revision history and authorities

- Revision 25 approved (2026-11-04): `series_id` is creation-only, like earlier built-in column additions; warehouses that predate it are rebuilt rather than evolved, so every stored point has a series id and the null-id fallback and AC-035 are removed.
- Revision 24 approved (2026-11-04): Metrics aggregate by default, group by chosen keys read with `variant_get`, opt-in capped every-series view, a server-searched rail scoped to the WHERE, and a server-computed `series_id` column with a Bloom on `bifrost.metrics.points`, hashed with SHA-256.
- Revision 23 approved (2026-11-04): server-side header sorting and keyset Older/Newer paging for Traces, Spans and Logs, with the sort and cursor in the URL; WHERE stays filter-only; authored SQL sorts in the page.

- Revision 22 approved (2026-11-04): REQ-048 corrected; DEBUG logs show by default and chart as their own level, since no default severity filter exists or is added.

- Revision 21 approved (2026-11-03): REQ-051 and AC-030 narrowed to the conventional Metrics page; MCP App histograms deferred to the MCP App delivery because no MCP App metrics view or served `ui://wyrd/view` exists yet.

- Revision 20 approved (2026-11-03): realistic dev workload (provider-injected failures, OTel span status, production metrics and logs, reconciled batch), plotted explicit-bucket histograms through the shared Chart and metric loader for every UI host, and the example delivered as a pinned wyrd-example-py commit instead of a patch. Approved as written, with estimated percentiles and publication of dev-workload.

- Revision19 draft (2026-11-02): proposed local signup/login, User-owned scoped
  personal keys, real seeded dev human and exact Observe alignment, based on
  caller direction and recorded auth/UI research. Awaiting approval of the provision-then-enroll admission and new security/visual
  contracts.

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
  Spec re-review approved; caller explicitly approved revision 9 on 2026-10-19
  and requested task planning. No implementation authorized.

- Revision 10 (2026-10-19): caller approves Rust-owned opaque browser sessions
  in the existing Postgres database, with Node rendering/forwarding and no new
  deploy secrets, environment variables or session infrastructure. Replaces
  REQ-006's nonexistent confidential-client/encrypted-cookie premise with
  REQ-033, updates REQ-004/032 and the future Node executor seam, and adds AC-012.
  Reconciles the owning design/security/deployment text. The TASK-001 review at
  `843484f6c` stays immutable; its two validated findings and this separate
  human-directed change are assigned to one remediation task. Approved by the
  caller's explicit instruction to revise the spec and write that task; no
  implementation or review PASS is claimed.

- Revision 11 (2026-10-20): caller explicitly accepts the two primary journey
  proposal and requests task planning. Defers AskPanel/Ask service and independent
  saved views, removes their shipping dependencies, adds real-browser page and
  multi-turn agent/MCP App acceptance, and permits the isolated opt-in live-model
  lane through existing readiness ownership. TASK-003 remains unchanged in
  scope. This approval concerns the stated outcomes, not implementation success
  or compatibility with an untested host.

- Revision 12 (2026-10-20): caller asks for Grafana-style panel
  layout after inspecting the chart foundation. Replaces the view document's
  `width: full | half` ordering with REQ-036: a per-panel grid `layout`,
  reader resize/move inside `WyrdView` behind a host layout handler, and no
  persistence until saved views (REQ-031). Components stay size-agnostic and
  no catalog component entry changes. Adds AC-015. Builds on uncommitted revision 11
  without changing it. Approved by the caller's explicit "yes" to the
  revision as written; no implementation is claimed.

- Revision 13 (2026-10-22): the UI work is rebased onto the flat Card identity
  change (service-identity-and-telemetry TASK-001), which removes `space` from
  `CardRef`, Card metadata and the SDK list requests. The exact reference above
  is the flat `kind`/`name`/`version` reference with optional `uid`, and UI Card
  identity renders as `Kind/name@version`. No other requirement changes.
  Approved by the caller's instruction to carry out the proposed rebase steps,
  which named this revision; no implementation is claimed.

- Revision 14 (2026-10-23): draft aligning the UI with approved
  service-identity-and-telemetry revision 12. Replaces resource-derived Service
  attribution with managed workload/subject/account dimensions, adopts the
  single `bifrost` catalog and canonical envelope/signal fields, and updates
  generic ownership labels and component data sources. Specifies signal/time
  fidelity, queued verification waiting and exact trace citation behavior,
  retains separate platform prerequisites, and adds AC-016–019 for affected
  consumer closure. Flat Card identity, transport/session/layout boundaries,
  deferred capabilities, prior completion records and immutable reviews remain.
  Approved by the caller on 2026-10-29, after main shipped the service
  packet's schemas and `workload_card_uid` was confirmed to name a Service only.

- Revision 15 (2026-10-30): approved by the caller after readiness review and
  the addition of five separately invoked execution stages. Follows the request for one
  remediation delivering an internal gateway mock, a canonical 1,000-request
  dev:full workflow using the current SDK, and a fully functional live Traces
  workbench. Fixes the minimal mock and SQL predicate contracts, transfers
  shell/trace/browser-tooling ownership to TASK-004-R2, and allows that live
  review before unrelated component families and the production MCP journey.
  Existing component work, tenant/security boundaries and final library/MCP
  obligations remain; approval makes no implementation or visual acceptance claim.

Authorities: `AGENTS.md`; `agent_docs/index.md`;
`architecture/wyrd-design.md`;
`architecture/bifrost-design.md`; `architecture/wyrd-security-posture.md`;
`architecture/operations/deployment-and-release.md`;
`changes/spec-driven-development.md`;
`TESTING.md`; repository `wyrd-ui`
guidance for styling. These are repository-relative authority paths, not copies
of skill instructions.

- Revision 16 (2026-10-30): caller-approved correction of rejected deployment-based mocking.
  Runtime SDK scope selects dispatch fixtures in the same tenant/deployment;
  direct/queued verification snapshots and retained provenance close the path.
  Existing TASK-004-R2 and its five stages are revised, not replaced.

- Revision 17: draft following the caller's rejection of all extra machinery.
  Only client runtime selection and provider dispatch remain; no calls-schema,
  verification-persistence, routing-policy or ledger-policy additions.
  TASK-004-R3 corrects Stage 1; old results remain historical evidence.

- **Revision 18 (2026-11-02, caller-directed scope approved):** retire R1/R2/R3
  as execution assignments; use optional external MockServer through existing
  endpoint configuration; create TASK-004-REV-01 and TASK-004-REV-02. Expand the
  early live slice to complete shell/Traces/Spans/Logs/Metrics and a default
  3,000-request canonical batch. Preserve all transferred component/page proof,
  no-paid-inference development and existing ephemeral lifecycle. No internal
  mock, SDK scope/header, ledger/schema change or verification propagation.
  Historical revisions 15–17 below/above record superseded decisions only.
