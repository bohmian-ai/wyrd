# Task 005 security boundary review

Result: **PASS** for the scoped source review. No material security finding proposed.

Subject: base `4f6bc10d4967b1cd19b5c2b314d9fd9d0b42d2d6` through candidate `a1b1a59c6f28b63d08973e2eccce168bbe80bd92`, first review. HEAD was the candidate during inspection. The changed source was not edited. This report covers Card/Prompt/files/usage authorization, tenant separation, credential handling, input validation and rendering, including their unchanged owners. It does not substitute for full-task acceptance or the orchestrator's execution evidence.

## Authority and coverage

| Authority / obligation | Source traced | Assessment |
|---|---|---|
| Root AGENTS.md; `architecture/wyrd-security-posture.md` security principles, browser-session boundary, request principal, permission check and RLS sections | `src/hooks.server.ts`; `src/lib/server/auth/browser-sessions.ts::BrowserSession` and `BrowserSessions.read`; server `http/middleware/authenticate.rs:80–158`; `components/auth/{principal_extractor,caller_extractor}.rs` | PASS: route tenant selects the cookie to present, while Rust resolves the effective principal. The new pages use `session.client()` and do not construct authority from route/query data. Hooks run on data requests as well as first loads. Rust refuses multiple session cookies and reuses verified principal/permission checks. |
| Approved spec revision 28 INV-002 and TASK-005 Scenario 4: credentials remain server-side; another tenant's Cards/files/usage never appear | Node executor `packages/ui/src/lib/node.ts:25–82`; MCP executor `mcp.ts:10–55`; existing BFF `routes/t/[tenantKey]/ui/load/+server.ts`; new Cards page loaders | PASS: the Node credential is retained by its SDK client and the BrowserSession cookie is a private field. MCP uses its host connection. Returned page/loader props contain input and read results, not a client, session or cookie. Hooks set `private, no-store`. No global Card result cache or cross-session executor was introduced. |
| `architecture/wyrd-security-posture.md` requires receiving-boundary authorization and verified-tenant SQL; TASK-005 authorized reads | Server `components/cards/routes.rs::get_card_for`, `get_card_objects_by_ref_for`, `authorize_card_ref`, `list_cards_for`, `list_artifacts_http`; `service.rs::get_card_objects_by_ref`, `card_manifests`; `state.rs::registry_tenant_conn` | PASS: Card reads, list and artifact inventory use existing `card_read` decisions through the canonical audited owner. SQL uses `TenantConn` bound to `caller.data_tenant_id`. Files listing first resolves an authorized Card, then lists that Card UID, and the server separately authorizes the inventory request. No Bifrost query grant is added or required. |
| TASK-005 exact selected identity and server-derived usage; security authority forbids payload-selected tenancy | Client `cards/handle.rs::list_artifacts`; server `routes.rs:185–202`; `service.rs:131–151`; SQL `queries/cards/relationships.rs:114–150`; UI `cards.ts::loadUsedBy`, `loadVersionDiff` | PASS: optional reference UID is checked against the resolved exact identity. Inbound relationships are read in the same tenant transaction; SQL joins source and edge tenant IDs. UsedBy takes refs from the authorized Prompt's server relationships and each Agent read is authorized independently. Comparison reads retain the authorized Prompt's name and exact version. No user-selected tenant is passed to an executor operation. |
| TASK-005 prerequisite failures issue no dependent reads and preserve actual refusal | `loaders.ts::loadCardHeader`; `cards.ts::listVersions`, `readPrompt`, `loadUsedBy`, `loadVersionDiff`; `cards/detail/+page.server.ts:44–59` | PASS: rejected Card/Prompt reads stop their dependent listing/Agent/comparison work. Detail header refusal stops all dependent panels. Errors are returned as denied/error states without a fixture or another-tenant fallback. |
| `architecture/wyrd-design.md` Prompt section; TASK-005 derived variables and authored form | `wyrd-spec/src/card/prompt/spec.rs::PromptSpec::status`; `envelope.rs::Status`; server `service.rs::hydrate_card`; generated TS status projection | PASS: only compiler-derived names are added, after the existing tenant-path, Card identity and integrity checks. The server replaces the read-side status; it does not trust a registration-supplied status to define variables or authority. No authored Prompt data is executed by this projection. |
| TASK-005 typed validated inputs/actions; security trust-boundary validation | `schemas.ts:35–50,105–143`; `loaders.ts::validatePanel`, `loadPanel`, `validateIntent`; `src/lib/cards-host.ts::writeDetail`; new conventional pages | PASS: new host input passes strict catalog schemas; Card identities require an exact version. Inventory query length and paging are bounded. URL fields use URLSearchParams individually and tenant path segments use encodeURIComponent. Navigation intents carry Card refs, not arbitrary external URLs. Server contract validation remains authoritative. |
| Injection and sensitive rendering | `PromptMessages.svelte`; `CardFacts.svelte`; `ArtifactTable.svelte`; `RelationshipList.svelte`; `UsedBy.svelte`; `VersionDiff.svelte`; `format.ts`; inventory filter shaping | PASS: authored strings, YAML, file paths, content types and native bodies use escaped Svelte text interpolation. Media sources are displayed as text, with no fetch, HTML insertion or link execution. Inventory text is sent as the existing registry filter language rather than interpolated SQL; the convenience name match escapes regex and language-string metacharacters. |
| Spec INV-002; deployment/fixture exposure | changed `scripts/dev/example-app.sh` setup and `fixtures/cards/ui_cards/*.yaml`; changed Observe server load; browser-safe contract imports | PASS: added seed Cards contain representative content and use existing CLI credential handling; no new secrets/config/auth bypass. Observe streaming still creates its executor from the selected BrowserSession and returns only read promises/results. Contract SDK imports remain type-only in browser-capable code. No dependency manifest, lockfile, migration, token or crypto changes occur in this range. |

CodeGraph was used before code discovery. Scope map and actual Git diff were checked against the approved task, applicable security authority and the owning Card/Prompt design sections. The SDK's nested AGENTS.md and task-review/maintainer workflow authority were read. The authority-map contradiction about older storage-inventory wording does not change the conclusion: the narrower current Card read owner uses the PostgreSQL manifest in the Card's tenant transaction, as the Card design explicitly requires.

## Security Audit

### Critical

None found.

### High

None found.

### Medium

None found.

### Low / Defense In Depth

No additional hardening recommendation is needed to accept this scoped change.

### Positive Controls

- Existing receiving-boundary authorization, audit and `TenantConn` owners are reused for the new projections.
- Tenant-private page results remain no-store; session/client values do not enter the new page-data objects.
- Files are metadata from the existing manifest operation, with no new local path reader or object-store bypass.
- Prompt rendering preserves native data as escaped text; variable names come from the Rust compiler projection.
- Bounded paging/Agent fan-out and prerequisite refusal avoid unbounded dependent read expansion.

## Verification and limits

Inspected the actual new proof source in `src/lib/ui-library.integration.test.ts:537–667`: it checks Node and MCP inventory isolation, foreign exact Card refusal, authorized files, all new panels' Node permission denial, no dependent request after refusal, and session-bound BFF tenant isolation/no API-key text. The MCP Files path has no server tool and is tested as an honest refusal; this is a host capability limitation, not an authorization bypass.

Inspected `src/lib/ui-pages.integration.test.ts:946–1040`: production-BFF/public-login journey source includes cross-tenant inventory/detail absence, invalid and nonexistent exact identities, and no-role denial. These observations describe test assertions, not independently executed results. The security reviewer ran no live server/browser or credential attack experiment. At report time `verification.md` was not yet available; the orchestrator owns execution evidence and any resulting acceptance decision. Source review found no security defect that requires a separate live probe to resolve.

Skipped unrelated existing auth/crypto and dependency vulnerability auditing; the range does not change those owners. Static PASS does not claim a penetration test or final production-host/MCP acceptance.
