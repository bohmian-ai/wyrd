# Domain review: security and tenancy

Result: **PASS**. No material security or tenancy finding in the cumulative candidate.

## Immutable subject and boundary

Reviewed base `c1aac2d132b19848ca1940c3907e1fa68a8d08a5` to candidate `02c06e21bd5fd613341935a843fabe5350ea0a9d`; HEAD remained the candidate. This is an independent source review of generated Prompt registration and hydrated Card navigation, not an implementation-summary review. Source remained unchanged; this report is the only owned write.

Boundary: authenticated HTTP registration → permission/audit decision → tenant-scoped reference resolution and composite transaction → generated Prompt identity/version and Agent pin → authenticated graph hydration → confined local bundle validation → Rust/Python/TypeScript local navigation. I independently expanded the supplied map with CodeGraph before targeted searches and reads, including Caller construction, route authorization, SQL row insertion, reference recheck/binding, RLS policies, registry version locks, bundle loading and foreign-runtime projections.

## Authority coverage

| Authority | Applicable obligation | Assessment |
|---|---|---|
| Approved `changes/active/agent-prompt/spec.md` r2, REQ-002/003/005, INV-001/003, AC-002 | Server-owned materialization; same effective space/authenticated tenant; ordinary registry conflicts; exact pins; no IO or writes during navigation; preserve permissions/audit | PASS against source paths below |
| Original `tasks/TASK-001-agent-prompt.md` | Extend existing transaction/hydrator; preserve file loader and auth boundaries; no permission/audit redesign | PASS within this domain |
| `AGENTS.md` §§2, 4, 9, 10, 11; `architecture/agent-rules.md` | Verified tenant identity, TenantConn/RLS, scrubbed security boundaries, non-blocking audit and public negative proof | PASS within this domain |
| `architecture/wyrd-security-posture.md`, security principles/trust boundaries/principal lifecycle | Tenant comes only from verified credentials; receiving-boundary permission; RLS; decisions staged without waiting | PASS |
| `architecture/wyrd-design.md` doctrine 17, Agent and reference forms; `architecture/wyrd-doctrine.mdx` | Registered inline Agent Prompt is an ordinary independently versioned Prompt Card; other inline/local semantics retained | Candidate authority change matches approved exception |
| Reference router; spec-driven development and maintainer-style references | Immutable evidence-backed review; concrete reachable consequences required for findings | Applied; no optional hardening reported |

## Source and proof coverage

| Trust boundary / attempted counterexample | Source evidence | Proof inspected and result |
|---|---|---|
| An underprivileged caller uses implicit child creation to bypass Card write authority | `components/cards/routes.rs:427-450,737-750` checks `card:write` before `service::register_card`; repository search found this sole production caller of that service entry. Prompt materialization occurs later in `service.rs:1127-1135` | `pg_card_registration_route.rs::inline_agent_prompt_materializes` exercises viewer refusal and asserts neither Agent nor generated Prompt nor operation bookkeeping persists. PASS |
| Generated child selects another tenant or drops caller attribution | `caller_extractor.rs:55-66` derives tenant from verified principal. `service.rs:1092-1095,1132,1215` threads that Caller and the same TenantConn. `inline_prompt_submission:1563-1586` copies only Agent space and validated derived name, never tenant/UID. `queries/cards/register.rs:298-361` inserts `wyrd.current_tenant()` and caller subject/actor via bound values | Rust SDK `agent_prompt_registration_identity` registers the same Agent/Prompt names in a second tenant, observes different UIDs and refuses a foreign exact Prompt read. PASS |
| App or database relationship reaches a foreign Prompt UID | `resolve_external:1016-1024` uses caller TenantConn; `EffectiveSpecs::resolve:157-198` resolves external identities under RLS; `bind_card_references:900-969` replaces authored UIDs. `recheck_active_card_refs:36-68` locks exact active identity/UID before write. Generated child is persisted through that same connection. `20260601000006_cards.sql:104-111` forces RLS; relationship migration forces RLS and tenant-qualified source/target foreign keys | Existing owners remain on the new path; no widened SQL capability or new tenant selector. PASS |
| A generated name overwrites an independently authored Prompt or changes older Agent pins | `persist_node:1179-1246` uses ordinary version-line lock, canonical content hash and `resolve_existing:1398-1466`. `version_sql.rs:63-88` lock key includes tenant, kind, space, name. Materialization takes the persisted outcome's exact UID-bearing ref before persisting Agent; no separate overwrite/version policy | HTTP materialization proof and Rust `agent_prompt_registration_identity` inspect old/new pins, content deduplication, independently authored same-name line, and distinct Agents with identical content. PASS |
| Generated writes or replay silently change the permission/audit contract | `audit/mod.rs:130-187` permission check precedes allowed/denied outbox staging; no audit persistence await. The new child evaluates no additional permission. `service.rs:654-701` replay stays caller-attributed and tenant-bound; generated outcomes enter the same committed replay seed at `1161-1164` | Existing decision plane is reused unchanged. A child write is not an independently evaluated permission decision requiring a second audit event. No new audit sink or failure propagation. PASS |
| Navigation trusts an unresolved or wrong Card, fabricates identity, performs a hidden request, or repairs a bundle | `state.rs:1242-1528` confines/loads bundle projections, requires exact UID-bearing manifest/envelope identity, rejects paths/siblings and missing graph targets, and checks relationship/spec agreement. `CardRef::Display` includes UID (`reference.rs:488-503`), so typed map lookup does not discard UID. `TypedCards::agent:95-124` requires the exact Prompt holder; `hydrate_typed_cards:1639-1641` validates all Agents before publication | Unit cases `agent_with_inline_prompt_is_an_invalid_bundle`, `agent_prompt_rejects_non_prompt_target`, missing-target coverage, and Rust `hydrated_agent_prompt_card_navigation` inspect local refusals and navigation after shutdown. PASS |
| Language projection replaces the shared resolver or adds durable authority | Python `src/state/mod.rs:1116-1147` obtains the shared Agent's prompt reference and attaches an existing Prompt holder; TS native `cards.rs:597-619` projects the same resolved reference/envelope; TS public `index.ts:2646-2673` and Python holders expose local values. `Run::invoke` consumes the shared hydrated relationship | Rust/Python/TS changed navigation tests and local holder changes inspected. No new network, file, SQL, registration or version-selection capability in navigation. PASS |

Local bundles remain local input, not authentication credentials or a server-signed authorization capability. Structural validation establishes graph consistency and path confinement; it does not claim cryptographic provenance of arbitrary locally edited Card content. This change does not widen that pre-existing trust boundary, and adding bundle signatures would exceed the approved task.

The cumulative diff adds no manifest/lockfile, deployment configuration, authentication/token, webhook/redirect, crypto, SQL interpolation, subprocess, or external-fetch change. Generated names pass `CardName::new`; derived-name rejection rolls back the existing transaction and is directly covered. No new secret or credential value was observed in changed production code or fixtures.

## Findings

None. No source-local issue with a demonstrated behavioral security or tenancy consequence requires correction. No open question blocks this domain verdict.

## Verification limits

Static review only: no builds, tests or live exploit requests were run, per orchestrator coordination. Inspected actual test bodies and the task's recorded exact commands/results; those reported GREEN results were not independently rerun here. Repository-wide security debt, dependency advisories and unrelated domains were not audited. PASS is scoped to the changed registration/navigation security and tenancy boundaries.
