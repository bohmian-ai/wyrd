# TASK-001 security domain review

Result: **PASS** for the reviewed security boundary. Proposed finding ledger: **empty**.

## Immutable subject and boundary

Repository: `/home/thorrester/Documents/GitHub/wyrd`. Reviewed source: detached `/tmp/wyrd-task001-review-dd98`, base `7b36f8fc6` through candidate `dd98e9ef78d20e7de47e7af623644fed0ff9075a`. Candidate checkout remained clean and at the recorded commit at completion. This is first-round discovery; no prior acceptance was carried forward.

Scope: exact Card identity, tenant and permission propagation, denied-prerequisite ordering, inbound-owner reads and truthful refusal presentation across portable loaders, Node, MCP, BFF and conventional page. The approved revision-2 spec in the original workspace governs REQ-014; the original task at candidate governs its scoped cases. The review does not treat the recorded BLOCKED label as a finding.

## Authority and source coverage

| Authority or owner | Inspected contract and path |
|---|---|
| Root AGENTS.md; security posture Security principles/Trust boundaries and First-party browser sessions | Server authorization and tenant authority; browser has no credential; selected tenant session cannot fall back; refusals remain distinct |
| Spec REQ-014, task REQ-014.states and proof | Denied prerequisites produce no dependent sensitive reads; foreign/missing/error are truthful; same components and read contract across hosts |
| `packages/ui/src/lib/schemas.ts:138,521–535` and `loaders.ts` registration | Verifier-only exact CardRef schema; strict catalog input feeds existing loadPanel contract |
| `packages/ui/src/lib/verifier.ts:180–194,228–260` | Exact Verifier read, projection, bounded owner reads, denied prerequisite early return, refusal per owner |
| `packages/ui/src/lib/components/RunsOn.svelte:20–49`; `VerifierDefinition.svelte:42–50` | Display of authorized relationship metadata and each refused read's stable problem; Svelte text/YAML escaping; no supplied HTML execution |
| Existing `node.ts:25–83`, `mcp.ts:10–55` | One host credential/principal; `cards.get_by_ref` maps to SDK Cards.get or existing MCP cards.get; no alternate transport or authority |
| Existing `src/routes/t/[tenantKey]/ui/load/+server.ts:17–38`; detail `+page.server.ts:29–59` | Session client supplied to executor; validated load input; successful header gates all dependent page panels |
| Existing server `components/cards/routes.rs:150–219`, `mcp/cards.rs:50–67` | HTTP and MCP converge on get_card_by_ref_for; authorize_card_ref audits cards:read; optional UID must match exact identity |
| Existing server `components/cards/service.rs:130–155,391–445`; SQL `queries/cards/relationships.rs:114–154` | Caller tenant connection resolves exact Card, inbound relationships and hydration; stored blob tenant/UID/identity integrity checked; inbound metadata comes from same tenant transaction, not UI synthesis |
| Existing Rust client `cards/reads/get.rs:37 onward` | Exact selector uses exact by-ref route and validates returned identity; no UI newest-version substitution |
| Added UI unit/integration tests | Refused prerequisite, refused owner, foreign browser session, missing version, non-Verifier, same Node/MCP/BFF/custom/artifact reads |

CodeGraph was consulted before source discovery. Source was then inspected in the frozen candidate, including unchanged downstream authorization and SQL owners.

## Boundary acceptance matrix

| Obligation | Source evidence | Verification evidence | Result |
|---|---|---|---|
| Effective tenant comes from verified host credential/session, never panel input | WireRequest/CardRef has no tenant authority; Node retains client; BFF uses session.client(); server service uses caller.data_tenant_id/TenantConn | New real-server host test submits local Verifier ref through foreign-tenant BFF session and expects registry 404; existing server boundary owners unchanged | PASS |
| Every new read retains normal current cards:read authorization | Both new loaders issue only cards.get_by_ref; Node and MCP route to shared authorize_card_ref, independently for each owner | Integration test wraps real restricted-key executor, expects denied for each loader and exactly one request per loader | PASS |
| Refused Verifier issues no owner reads | loadRunsOn immediately returns non-populated result at lines 245–246 before reading inbound refs | Unit test records only cards.get_by_ref after prerequisite denial; integration restricted key verifies one read each | PASS |
| Refused owner is visible and cannot supply its binding body | Owner WireResult failure mapped with failed(), preserving identity already provided by successful Verifier read; no spec read from failed result | Unit test expects owner bindings.state denied and rendered stable denial code | PASS |
| Missing version and wrong kind remain failures rather than empty or another version | readVerifier propagates missing; rejects non-Verifier; exact references retained for owner calls | Both new loaders tested against real missing version and Service ref; schema refuses non-Verifier panels and latest pin | PASS |
| Bounded owner limit never becomes empty success | slice limits requests to 50; each unrequested owner becomes explicit error row, not missing/empty bindings | Static source proof; limit branch has no dedicated test | PASS |
| Superseded results cannot expose an earlier selection as current | New loaders use existing AbortSignal/executor and WyrdView generation owner; no new cache or retained authority | Added delayed old-version unit case confirms old response never renders after replacement | PASS |
| Browser and custom components receive no credential from new loaders | Returned props contain specs/graph/CardRefs/read states only; host client stays in executor | Custom-app integration checks response text excludes server API key; built MCP renders only host tool results | PASS |

## Reachability and materiality assessment

`RunsOn` displays owner identity from the already-authorized Verifier's server-derived relationships, then separately authorizes reading each owner's binding declaration. A denied owner row does not newly derive or disclose owner spec. The existing Card read already exposes those relationship identities to the caller; adding a second UI permission or hiding already-returned metadata would invent a new security contract.

The UI has not created a privileged read, tenant selector, mutation, verification resubmission, credential store, or authorization cache. Its new requests converge on existing public read owners and preserve failures rather than interpreting them as absent bindings. The 50-owner limit is visible as per-row errors; lacking a dedicated limit test is a verification limitation, not a demonstrated security failure.

No material security finding met the skill threshold. No correction or remediation is proposed.

## Verification limits

This independent review inspected the candidate source and actual test assertions; it did not rerun server/Postgres/Chromium tests or accept visual comparisons. Passing executions are task-recorded claims, not newly reproduced results. Real refused-owner proof is fixture-based; real restricted-key prerequisite and foreign-tenant refusal assertions exist in the added integration test. Human visual acceptance and the full integration command remain outside this security verdict and must be reconciled by the orchestrator.
