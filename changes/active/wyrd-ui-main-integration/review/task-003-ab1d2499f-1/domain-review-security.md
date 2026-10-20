# TASK-003 independent security and tenant-boundary review

Result: **PASS**. Proposed material findings: **none**.

## Immutable subject and coverage

Repository and scope are those in `subject.md`: base `695ca914df61a8683059f6f32279ec2148424e06`, candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. HEAD still equals the candidate at the end of source inspection. Inspected the cumulative changed-path set and history, then the complete changed security-relevant implementations and their existing consumers. The separately approved layout successor and expressly excluded commits are not classified as foundation drift. No production source was changed.

| Authority | Boundary audited | Result |
|---|---|---|
| Approved spec revision 12, REQ-030 (executors, SQL, component actions), REQ-033; original TASK-003 scenarios 2–4 | Component-only BFF reads; credential-bearing host execution; exact identities; input/action validation; cross-tenant refusal | PASS |
| AGENTS.md §§2, 3, 9–11; architecture/agent-rules.md | Rust/server authorization ownership, tenant isolation, non-blocking audit, existing transport reuse | PASS |
| architecture/wyrd-security-posture.md, First-party browser sessions and trust boundaries | Selected opaque session, current authority, no browser token projection, no cookie promotion to MCP/gRPC | PASS |
| architecture/wyrd-design.md identity/authentication boundaries; architecture/references/doctrine/architecture-constraints.md | Caller-derived tenant and sole shared client surface | PASS |
| architecture/references/languages/agent-harness.md | Closed typed read projection; no tool-supplied authority; owning server permissions | PASS |
| architecture/references/languages/spec-driven-development.md and maintainer-style.md | Acceptance/scope calibration and source ownership tracing | Applied |

## Source paths and producer-to-consumer traces

**Shipped BFF session.** `src/hooks.server.ts::handle` resolves the route's tenant key on every request through `BrowserSessions.read`; it refuses a missing selected session instead of using another cookie. `src/lib/server/auth/browser-sessions.ts::read` selects only `wyrd_session_<tenantKey>`, asks Rust for metadata filtered to that key, and requires the returned tenant to match. The newly added `BrowserSession.client` forwards the private cookie and origin into the existing `WyrdClient.forBrowserSession`. The new `src/routes/t/[tenantKey]/ui/load/+server.ts::POST` requires that session, validates range and catalog component input, and calls only `loadPanel` with `nodeExecutor(session.client())`. Its response contains props or a stable problem; no session object is returned.

**Existing owner reuse.** Shared Rust `transport/credential.rs::BrowserSession::new` accepts one cookie pair and a bare HTTP(S) origin, marks the cookie header sensitive, and redacts Debug. `transport/http.rs::{request_credential,authenticated}` attaches that session rather than exchanging it for a bearer. `http/middleware/authenticate.rs::{authenticate_tenant_http,authenticate_browser_session}` refuses ambiguous cookies, preserves explicit bearer precedence, resolves current session authority, and inserts the same authenticated principal as normal tenant handlers. These existing owners provide the complete transport/authorization workflow; the candidate does not create a Node session authority or replace it with manually constructed HTTP requests.

**Read dispatch.** `packages/ui/src/lib/contract.ts::WireRequest` contains six reads with no tenant/principal or arbitrary tool/URL field. `node.ts::nodeExecutor` calls existing Cards/Bifrost/Execution SDK handles; `mcp.ts::{tool,mcpExecutor}` maps that closed union to existing MCP tools on the host's authenticated connection. `loaders.ts::validatePanel` uses own-property lookup and the strict catalog schema before selecting its loader. `loadCardHeader` performs exact Card resolution before dependent version listing, using the returned Card identity and no listing after refusal. `loadChart` sends authored SQL and typed binds separately, preserving the server's existing read-only floor rather than introducing UI SQL authority.

The downstream shared owners were inspected: `components/cards/routes.rs::get_card_by_ref_for` authorizes/audits cards:read and rejects a mismatched supplied UID; MCP cards dispatches into these same route owners. `bifrost/service.rs::{list_tables,describe_table}` authorizes/audits and supplies `caller.data_tenant_id` to the catalog. `query/service.rs::stream_query` admits capability, derives Oracle context from the caller and dispatches through the existing query owner; MCP bifrost calls that owner through BoundedQuery. `components/verification/service.rs::get_execution` authorizes cards:read and reads execution state with `registry_tenant_conn(caller.data_tenant_id)`; MCP verification calls that same owner. No new identity-resolution, permission, audit or SQL tenancy implementation appears in the reviewed adapters.

**Browser-session Bifrost.** `wyrd-client/src/bifrost/facade.rs::connect_with` now selects the existing query-only construction for browser-session credentials, through `read_only` and common `from_sink`. QueryOnlySink rejects ingestion batches and no ingest channel is dialled. `client.rs::is_browser_session` examines the credential variant; it grants no additional authority. The HTTP query operations continue through the shared transport and authenticated server boundary. This extends the existing shape rather than adding a second query service or allowing session authentication on gRPC/MCP.

**Action and document authority.** Strict `schemas.ts::{ViewDocumentSchema,PanelSchema,components}` allows data and catalog actions without handlers, executable code, tools or destination URLs. `WyrdView.svelte::act` validates intents against the emitting component, keeps chart-range changes local, prevents repeat pending actions, and delegates only validated navigation intents to the host. Neither the MCP App nor custom example installs a mutation handler. Unavailable host actions therefore gain no write authority. `CardRefChip` accepts an href only from direct component props; href is not a view-document catalog input.

**Browser/native boundary and hosts.** `index.ts`, `view.ts`, `loaders.ts`, `schemas.ts` and `contract.ts` have no runtime wyrd-ai import; SDK domain types use erased `import type`/`export type`. Only the Node executor imports native SDK values. AppBridge `src/app/main.ts` installs the MCP executor, not the Node executor. `remoteLoader` posts component/input/range and holds no credential. The dev gallery loader rejects production access. The custom app's `src/executor.ts` constructs its server-side application principal; `+page.server.ts` and `view/load/+server.ts` return loaded props only. Its endpoint intentionally executes reads as that application's configured principal, as explicitly approved for custom-app use case 2; it does not claim per-viewer browser-session authority.

## Verification assessment and limits

Inspected `src/lib/ui-library.integration.test.ts`, particularly the real-server SDK/MCP denial checks and scenario 4's Rust-session BFF calls. Scenario 4 creates a separate tenant, proves its Card/query reads, refuses a foreign Card, rejects malformed component/range input and absent session, and checks responses against API keys and cookie values. The custom-app checks similarly ensure props do not contain the fixture credential. These are meaningful server-backed assertions for the introduced adapters.

The BFF scenario imports and invokes the endpoint handler directly with a real resolved session. It does **not** exercise deployed SvelteKit HTTP routing, the hook, reverse-proxy origin construction, or browser cookie delivery. Source tracing establishes that integration wiring; the scenario must not be described as an independent deployed-browser proof. Broader browser-session machinery predates this task and was not re-reviewed as a new authentication implementation.

No commands or live security journeys were rerun in this independent static domain pass. Reported green checks remain supplied verification evidence, not newly observed execution. Linux packaging and live degraded-terminal parity remain the limits recorded in the original task; neither changes the authorization result established here. No exploitability or tenant-separation claim depends on human visual acceptance.

## Finding ledger

Empty: no reachable material security, authorization or tenant-isolation defect attributable to TASK-003 was found. No optional hardening or unrelated pre-existing issue is proposed as remediation.
