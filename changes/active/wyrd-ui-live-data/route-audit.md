# Live UI server and BFF route audit

Status: source audit for approved spec revision 3; implementation and runtime proof pending.

## Decision: reuse the server APIs and existing SvelteKit routes

Reuse mounted Wyrd APIs for registry/identity/control-plane reads and the canonical
Bifrost query API for analytical reads. Extend the existing server-only UI facade
and SvelteKit loads. Do not create a parallel Rust `/ui/*` endpoint family or
a SvelteKit JSON proxy endpoint for every page.

The inspected owners cover the approved workflows. The missing behavior is
live orchestration, typed projections, bounded queries, asynchronous load wiring
and truthful presentation. A proven requirement for a new public endpoint
requires spec revision; this packet does not authorize one.

SvelteKit `+page.server.ts` is already a BFF route: it serves SSR and framework
data navigation. Loads call the server-only facade directly. Existing form
actions handle authentication mutations, and `/login/callback` is the existing
callback handler. No new `+server.ts` JSON route is currently required for the
included page controls; search, filters, pagination and refresh use normal
navigation/server loading. A private incremental handler is justified only by
a demonstrated interaction that cannot use that path, with the same auth checks.

This audit is descriptive planning evidence, not a handwritten input to code
generation. Mounted registrations and the served OpenAPI document remain the
HTTP authority. Analytical row schemas remain separate from HTTP DTO generation.

## Inspected ownership and callers

Paths below are repository-relative.

| Boundary | Inspected source and consumers |
|---|---|
| Mounted surface and OpenAPI | [build_router](../../../crates/wyrd/wyrd-server/src/http/router.rs), [Card handlers](../../../crates/wyrd/wyrd-server/src/components/cards/routes.rs), [query handlers](../../../crates/wyrd/wyrd-server/src/query/routes.rs), [verification handlers](../../../crates/wyrd/wyrd-server/src/components/verification/routes.rs), [catalog handlers](../../../crates/wyrd/wyrd-server/src/bifrost/routes.rs), [served contract test](../../../crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs) |
| BFF session and loading | [hooks](../../../crates/wyrd/wyrd-server/wyrd-ui/src/hooks.server.ts), [BrowserSession/BrowserSessions](../../../crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/browser-sessions.ts), [domain facade](../../../crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/wyrd.ts), every in-scope load under [routes](../../../crates/wyrd/wyrd-server/wyrd-ui/src/routes) |
| Wire contracts | [registry reads](../../../crates/wyrd-spec/src/registry/reads.rs), [Bifrost API](../../../crates/wyrd-spec/src/vala/api.rs), [verification control](../../../crates/wyrd-spec/src/verification.rs), [human connection contracts](../../../crates/wyrd-spec/src/auth/human_connection.rs) |
| Analytical transport | [Rust Bifrost facade](../../../crates/shared/wyrd-client/src/bifrost/facade.rs), [QueryClient](../../../crates/shared/wyrd-client/src/bifrost/query.rs), [TypeScript SDK](../../../sdks/wyrd-sdk-ts/wyrd/src/index.ts), SDK OTEL/verification integration tests and support-desk example |
| Auth and proxy | [auth registrations](../../../crates/wyrd/wyrd-server/src/components/auth/routes.rs), [authorize/discovery](../../../crates/wyrd/wyrd-server/src/auth/authorize.rs), [device/revoke](../../../crates/wyrd/wyrd-server/src/auth/cli_login.rs), [CLI token exchange](../../../crates/shared/wyrd-client/src/auth.rs), [identity administration](../../../crates/wyrd/wyrd-server/src/components/admin/identity.rs), [audience middleware](../../../crates/wyrd/wyrd-server/src/http/middleware/authenticate.rs), [nginx](../../../docker/official/extras/nginx/nginx.conf.template), [official image](../../../docker/official/Dockerfile) |
| Tests and discovery | [identity host](../../../crates/wyrd/wyrd-server/tests/integration/identity_ui_e2e.rs), [existing production auth cases](../../../crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts), UI Card/Service/Observe/TraceDetail tests, [mise](../../../mise.toml), UI Vite config/package/lock |

## Required server read contracts

All paths include their mounted prefix. Domain responses/errors must be consumed
through generated HTTP types. Preserve structured `WyrdProblem` errors and their
actual media types; do not flatten authentication/denial/not-found/upstream failures.
Protected calls use the current session's `x-wyrd-access-token: Bearer …`.
The authenticated tenant comes from that token, not a URL tenant parameter.

| ID | Existing method/path | Input and response | Authority and constraints |
|---|---|---|---|
| S-CARDS | GET `/v1/cards` | Query: `kind, space, name, version_range, status, filter, include_prerelease, limit, cursor`. `filter` is canonical metadata-query text. 200 `ListCardsResponse { items: CardSummary[], next_cursor }`. | `cards:read`; tenant-scoped registry. Keyset cursor is opaque and scope-bound. SQL owner currently orders creation time/UID ascending and caps the page at 200. No total/count/facet/owner/labels response fields. |
| S-CARD-UID | GET `/v1/cards/by-uid/{kind}/{card_uid}` | Kind-qualified exact UID. 200 `GetCardResponse { card, created_at, updated_at }`. | `cards:read`; 400 malformed, 401 unauthenticated, 403 denied, 404 tenant-visible absence, 503 unavailable. Current UI UID-only links need bounded kind resolution; no unbounded list scan. |
| S-CARD-REF | GET `/v1/cards/by-ref` | Required query `kind, space, name, version`; exact version, not floating latest. Same `GetCardResponse`. | Same Card read authority/errors. Use after resolving the identity line when a URL selects another version. |
| S-VERSIONS | GET `/v1/cards/{kind}/{space}/{name}/versions` | Optional `include_prerelease`; 200 `ListVersionsResponse { versions }`. | `cards:read`. Versions are values, not mock author/note/timestamp rows. |
| S-BINDING | GET `/v1/verification/bindings/{binding_id}` | ID comes from owner Card `status.verification.binding_ids`. 200 `VerificationBindingStatus`: exact owner/subject/Verifier, activity, readiness, schedule and optional `last_run_id`. | `cards:read`; 400/401/403/404/503. Bounded reads of declared bindings; no binding inventory scan. |
| S-RUN | GET `/v1/verification/runs/{run_id}` | Known control-plane run ID; 200 `VerificationRunStatus`: execution state, requester, optional result pointer/error, dispatches. | `cards:read`; 400/401/403/404/503. Its run ID is not an analytical `result_id` or application `run_id`. No list endpoint. A completed pointer does not establish query visibility. |
| S-QUERY | POST `/v1/query` | `BifrostQueryRequest { sql, params, deadline_ms }`; SELECT-only, ordered typed `$1…$n` binds. 200 `application/vnd.wyrd.bifrost-query-stream`, length-delimited protobuf QueryStreamFrame/Arrow data, not JSON rows. | `bifrost_query:read` plus authoritative resolved object/column scope. Bifrost surface accepts ordinary Wyrd credentials and Bifrost-bound delegation as permitted by existing verifier. Use SDK/shared QueryClient; validate result schema and terminal. Preserve 400/401/403/413/422/429/503/504/default structured refusals. |
| S-DESCRIBE | GET `/v1/bifrost/tables/{namespace}/{name}` | Existing `BifrostTableDescription` with physical schema. Catalog list: GET `/v1/bifrost/tables` → `BifrostTableEntry[]`. | `bifrost_table:read` plus existing catalog visibility. Optional SDK metadata support, not a mandatory catalog request before every page query. Query result schema is authoritative even for zero rows. |
| S-CONTRACT | GET `/openapi.json` | Runtime composed OpenAPI document from mounted registrations. | Fresh local served input to codegen, not a checked-in/remote authoritative copy. |
| S-READY | GET `/readyz` | Existing readiness response; `/healthz` is also mounted. | Existing unprotected health surface; bounded setup/sign-in readiness, not a domain health judgment. |

## Existing authentication and setup contracts

| ID | Existing server routes | Role |
|---|---|---|
| S-OAUTH | GET `/.well-known/oauth-authorization-server`; GET `/auth/authorize`; GET `/auth/callback`; POST `/auth/token`; POST `/auth/revoke` | RFC 8414 discovery, Wyrd authorization broker/provider callback, confidential UI authorization-code + PKCE redemption, refresh/API-key exchange, best-effort logout revocation. OAuth form contracts/errors differ from domain JSON problems. Reuse openid-client/BrowserSessions; no handwritten OAuth implementation. The provider callback is Rust `/auth/callback`; the UI callback is BFF `/login/callback`. |
| S-DEVICE | POST `/auth/device_authorization`; GET/POST `/auth/device`; POST `/auth/token` | Existing CLI device authorization, browser approval/provider sign-in, token polling. CLI stores the resulting credentials through the existing writer. The device approval HTML belongs to Rust, not a new SvelteKit device route. |
| S-IDENTITY | GET `/v1/identity/oidc/connections`; PUT `/v1/identity/oidc/candidate`; POST `/v1/identity/oidc/candidate/test`; POST `/v1/identity/oidc/candidate/activate`; POST `/v1/identity/oidc/active/deactivate`; DELETE `/v1/identity/oidc/connections/{id}` | Already-used Settings/session setup operations. Generated bodies: HumanConnectionsResponse, ConnectionInput, ConnectionTestRequest/Response, ConnectionActivate and actual empty/status responses. Existing `identity_connections:write` governs administration, including the list. Preserve settings behavior/type its existing consumers; no Settings product expansion. |
| S-PRINCIPALS | GET `/v1/principals` | Existing privileged setup/assertion read can confirm no duplicate human principals. Routine page loads do not gain principal-admin access; no browser auto-registration write route is needed. Wyrd creates the human through the existing broker identity flow. |
| S-REGISTER | POST `/v1/cards`, existing upload lifecycle if authored Cards require artifacts, POST `/v1/cards/{card_uid}/complete` | Public SDK setup registers real native Cards. CreateCardRequest/CreateCardResponse and Idempotency-Key retain their existing meanings. These are fixture/workload pathways, not new browser authoring actions. |
| S-EXECUTE | POST `/v1/verification/execute`; POST `/v1/verification/runs` | Actual direct Verifier execution and supported manual binding run submission. Existing ExecuteVerificationRequest/Judgment and StartVerificationRunRequest/Response, verifier:run/Card scope, baseline refusals/deadline/idempotency rules remain server-owned. Setup uses them only where the supported workload requires them. |
| S-EMIT | POST `/v1/traces`, `/v1/logs`, `/v1/metrics`; existing SDK Bifrost gRPC ingest; existing supported Gateway invocation routes where used by the workload | Real OTLP/SDK execution evidence. Reuse stock export/public clients and deterministic local providers. These are ingestion/workload paths, not page read APIs or browser BFF routes. |

The existing official proxy forwards /v1, /auth, /platform, /mcp and
/openapi.json, /healthz, /readyz to Rust; other paths go to the BFF.
BFF OAuth discovery uses its internal custom fetch rewrite, so it does not depend
on external proxy discovery routing today. The CLI does not use discovery: inspected `TokenExchange::new` in
`crates/shared/wyrd-client/src/auth.rs` builds `/auth/device_authorization`
and `/auth/token` directly from the configured Wyrd server origin. The server
owns identity-provider interaction and issues the Wyrd credentials; the CLI
does not exchange tokens with Keycloak. The browser may follow the server-owned
provider redirect during approval. Verify that complete flow through the real
public proxy.

Separately, the inspected nginx template does not explicitly forward
`/.well-known/oauth-authorization-server`. Check/fix published discovery
reachability as deployment configuration in task 002 for clients that consume
that endpoint. It is not a source-proven CLI login blocker and does not justify
an auth architecture change or browser request interception.
Verify advertised authorization/token/callback URLs, host/proto forwarding,
local TLS trust and the SDK's actual configured endpoints.

## Analytical page contracts

Query shapes below define required data and scope, not exact SQL text or private
helper structure. Bind user values; never accept raw SQL/table names from these
page URLs. Use bounded projection/aggregation and deterministic continuation.
Time filters derive one explicit server-side UTC interval from the accepted
range (15m, 1h, 6h, 24h, 7d, 30d); list/count/chart queries in one load use
equivalent scope. Optional timestamp/ID/correlation values remain optional.

| ID | Canonical sources and required result semantics |
|---|---|
| Q-TRACES | `vala.traces.spans`: trace_id binary16, span_id/parent binary8, name/kind, start/end/duration, status presence/code/message, service_name/version, managed run/Card correlation. List projects trace-grain rows and supported counts/status/search under service/time scope; detail restricts actual trace identity and applicable scope. Nested events/links live in the same table; there are no separate events/links tables. Unattributed spans do not magically resolve to registered Cards. |
| Q-LOGS | `vala.logs.records`: time_unix_nano/observed_time_unix_nano, severity_number/text, optional body/body_text, service_name/version, optional trace/span, resource/scope/attributes. Scope/filter/count/page/detail use the same matching records. No persisted log_id exists: URL record selection is presentation state, not a new durable identity. Equal-valued records must not be discarded to invent unique IDs. |
| Q-METRICS | `vala.metrics.points`: metric_name/type/unit, time/start, resource/point attributes, service_name/version, int/double value, kind-specific counter temporality/monotonicity, histogram/exponential/summary fields. Derive supported measures at correct series grain and preserve units, nullable fields and gaps; unsupported measures are unavailable. No sum of cumulative counters or fabricated zero/percentiles. |
| Q-RESULTS | `vala.verification.results`: result_id, implementation, execution_status/verdict, verifier_version, managed Verifier card_uid and optional application run_id, subject/owner/binding/source-record/trigger identity, windows/start/end, drift_report/eval_summary/assertion_result. One row per completed execution that produced a verdict; errored/running executions do not have rows. Task Verifiers use assertion_result directly, not an invented task-item table. |
| Q-TASKS | `vala.eval.result_items` joined by result_id: task_id, outcome_kind, nullable passed/actual/expected/operator/message/stage/start/duration, skip_reason/upstream_task, source-record and owner/subject/binding identity. Counts of executed tasks exclude skipped items; skipped evidence remains visible. Optional correlated input comes from `vala.eval.observations` by source record with actual trace/span IDs. |
| Q-DRIFT | `vala.drift.result_features` joined by result_id: owner/subject/binding, comparison window, method/feature, nullable score/threshold, verdict. Baseline declaration/state comes from exact Verifier Card/status and S-BINDING readiness; known failed execution comes from S-RUN. Do not infer baseline/health from empty history. Optional observed feature series uses `vala.drift.observations` (`record_id, series, num_value/str_value, session_id, created_at` plus managed correlation). |

Verification result tables have no telemetry `service_name` column.
Resolve service-scoped verification through authorized exact Service/component
Cards and declared binding owner/subject/Verifier identities; optional
application-run/eval-observation/trace correlations refine attributable evidence.
A telemetry resource name alone is not an exact Service CardRef. Shared
components across Services must not widen owner-scoped binding results.
Direct results without owner/binding are displayed only where actual subject/run
correlation supports attribution; otherwise show their real attribution and
unavailable service association. Never assign them to whichever Service is selected.
Keep UI `evalCard/driftCard` URL keys, but resolve them to native Verifier selectors.

## BFF route and consumer matrix

`T` below means `/t/{tenantKey}`. File routes use SvelteKit's
`[tenantKey]`, `[uid]`, `[id]` placeholders. Every included protected
request is checked by hooks, including framework data navigation/preloads;
cached layout data is not authority. Async live facade methods require
`await` in every in-scope load/caller and catch/error mapping after awaiting.

| BFF route/method | Current consumer and URL state | Required upstream | Owning tasks / missing behavior |
|---|---|---|---|
| GET `/`; POST `/?/tenant`, `/?/switch`, `/?/logout` | Root load and server actions; tenant chooser/switch/logout. Existing mock login/scenario actions stay development-only. | S-READY; S-OAUTH for session renewal/revocation | 002; validate production journey and generated auth consumers, not a new login controller. |
| GET `T/login`; POST `T/login?/sso` | Existing load/action; tenant key, failed login state, PKCE/state cookie. | S-OAUTH | 002; use current broker/session. |
| GET/POST `T/login/api-key` | Existing recovery default server action; not routine SSO. | S-OAUTH token exchange | 002 regression; retain secret redaction/tenant binding. |
| GET `/login/callback` | Existing `+server.ts` GET handler; authorization response validated by BrowserSessions. | S-OAUTH token redemption | 002; do not confuse with Rust provider callback. |
| GET `T` | Home server load (`home` already awaited) | S-CARDS initially; Q-TRACES/Q-LOGS/Q-METRICS/Q-RESULTS and declared control status for complete summaries | 003 then 018; truthful bounded summaries, unavailable unsupported counts/sections. |
| GET `T/cards` | Cards load; q/kind/space/status/label/owner; pagination to be completed | S-CARDS plus bounded S-CARD-UID metadata where required | 004; await live data, map typed filters/cursor, no page-local global counts/sorting. |
| GET `T/cards/{uid}` — Model | Shared Card load/shell; version | S-CARD-UID/S-CARD-REF/S-VERSIONS; exact related Card reads | 005; bounded kind resolution, exact selected version, typed full envelope. |
| Same route — Data | Shared generic shell; version/schema metadata | Same Card contracts | 006; separate Data page proof, no new route/workspace by default. |
| Same route — Agent | Shared generic shell; version/Prompt links | Same Card contracts | 007; use actual server-derived relationships. |
| Same route — Prompt | Shared generic shell; version/declaration links | Same Card contracts | 008; safe actual declaration, no inferred runtime outcomes. |
| Same route — Service | Service workspace view/version/range/sel; state is mock-only | Same Card contracts, S-BINDING/S-RUN where declared, Q-TRACES/Q-METRICS/Q-RESULTS for supported operational content | 009; shared load currently reads only version, so forward/read operational scope server-side and fetch matching data. Client-only scope changes must not keep stale summaries. |
| GET `T/observe` | observe(scope); service/range, default 1h | S-QUERY with appropriate bounded signal/result summaries; exact Cards/bindings for verification attribution | 010; await live query results and preserve destination scope. |
| GET `T/observe/traces` | observeTraces(filters); service/range/status/q/limit | S-QUERY / Q-TRACES | 011; trace-grain filtering/continuation. |
| GET `T/observe/traces/{id}` | observeTrace(id, span); service/range/status/span; current back state misses q/limit | S-QUERY / Q-TRACES; Q-LOGS for supported correlation | 012; pass applicable scope into actual reads and restore list context, not only render scope chips. |
| GET `T/observe/logs` | observeLogs(filters); service/range/level/trace/q/record/page/per | S-QUERY / Q-LOGS | 013; same scope for records/facets/count/pages/selection. |
| GET `T/observe/metrics` | observeMetrics(filters); service/range (default 6h)/metric | S-QUERY / Q-METRICS | 014; actual scoped series and correct type/temporality. |
| GET `T/observe/evaluations` | observeEvals(filters); service/range (default 24h)/status/origin/evalCard/q; pagination to be completed | S-QUERY / Q-RESULTS/Q-TASKS; exact Card and S-BINDING/S-RUN reads as applicable | 015; actual result-grain counts/charts/continuation, real RBAC instead of mock evals:read. |
| GET `T/observe/evaluations/{id}` | observeEval(id, task); service/range/task; current facade receives no scope | S-QUERY / Q-RESULTS/Q-TASKS; assertion_result for task implementation; S-RUN only by a real known control ID | 016; id maps actual result identity, selected task matches that result, scoped evidence/links. |
| GET `T/observe/drift` | observeDrift(filters); service/range (default 30d)/driftCard/feature; history pagination to be completed | S-QUERY / Q-RESULTS/Q-DRIFT plus exact Verifier and S-BINDING/S-RUN control state | 017; baseline/readiness and comparison are different facts; all supported selectors affect actual reads. |
| GET `T/settings`; POST named stage/test/activate/deactivate/remove actions | Existing connection administration with handwritten ConnectionView/Connections casts | S-IDENTITY | 001 types existing consumers; 002 regression/setup only. No new Settings behavior. |

Existing `T/changes/**`, `T/observe/dashboards/**`, `T/observe/genai`
and `/styleguide` are inventoried but receive no new page gate under this
approved scope. Do not accidentally connect them to live fixtures, create extra
signal tables or expand their workflows. Existing unsupported live pages keep
truthful unavailable states. GenAI fields legitimately present on trace spans
may render within trace detail without implementing a new GenAI page.

## Findings and bounded implementation closure

1. All included page route files already exist. No separate Model/Data/Agent/Prompt/Service HTTP page routes are needed.
2. Production hooks create WyrdClient with context/mock=false but no authenticated upstream session/client. Domain calls reject upstream. Connect that seam without exposing the private token.
3. Home awaits its facade; other included loads currently invoke synchronous mock methods. Await live methods and handle rejection after await; close affected mock callers too.
4. The Service load reads version but ignores view/range/sel for upstream work. Trace/evaluation detail reads omit service/time scope. These are real load-contract gaps.
5. Card lists have a cursor and bounded items but no global counts/facets/owner/label fields. Map actual metadata, keep supported filters, and communicate unknown totals. Never sort one page to claim globally newest order.
6. Canonical telemetry/result schemas differ materially from mocks. Nested span events/links, nullable log correlation, metric point types and completed-only results require projections, not new server endpoints.
7. Verification service filtering needs declared exact attribution; there is no result service-name column. Binding readiness and known run errors use existing control reads.
8. Settings/auth currently use handwritten TS types/casts. Include existing consumed HTTP operations in generation/typed closure while preserving behavior and permissions.
9. The TypeScript SDK reuses Rust QueryClient's HTTP POST /v1/query decoding. Existing Bifrost.connect also establishes ingest-related gRPC configuration: production package/native binary/endpoint/image closure is required even for the chosen query usage. Do not write a new browser/Node decoder to avoid packaging work. Following independent plan finding PR-001, task 009 now explicitly owns an authenticated analytical BFF smoke under the official image using the existing production startup lane; host UI checks are supporting evidence only. The focused startup selector is planned implementation, not an existing passing test.
10. Existing identity UI tests are HTTP fetch tests, not browser journeys. Their filtered wrapper runs one selected test but recreates setup/builds per invocation. Focused projection/component checks belong in RED/GREEN iteration; the live filtered browser command is the page acceptance gate, not every scenario's unit-test command.
11. Setup must use the existing lifetime and public SDK workloads; baseline/verification evidence cannot be inserted directly. Visibility readiness is bounded test support, not a production freshness guarantee.
12. The exact required HTTP response/media/error coverage must be proven against the served document. This source audit does not claim runtime schema completeness.

No source-proven need for a new public UI-specific server endpoint was found.
If exact global registry totals, arbitrary telemetry-name-to-Service attribution,
all unfinished verification history or a mock-only deployment-health resource
becomes mandatory, that exceeds the inspected existing contracts and must
return to specification authority rather than being invented in a task.

## Evidence boundary

This is a static source/caller/contract audit, not a live browser or served-API
validation result. Task 001 proves generated/runtime contract fidelity; each
page task proves actual serialization, query schema, exclusions and UX against
the real shared environment. No implementation or runtime tests ran for this
planning revision. The server-mediated device interaction agrees with
[RFC 8628 section 3](https://www.rfc-editor.org/rfc/rfc8628.html#section-3);
the inspected Wyrd CLI constructs its own server endpoint URLs rather than
using the RFC discovery metadata.

