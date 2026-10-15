# Domain review: security and tenancy

Result: **PASS**. No material security or tenant-isolation finding in the cumulative candidate.

## Subject and reviewed boundary

Immutable subject: base `c1aac2d132b19848ca1940c3907e1fa68a8d08a5` → candidate `a21a804bc56f039f8a407f6a5b9c4f95905bf3f0`. HEAD was checked at the candidate. Reviewed original materialization/navigation changes and the remediation, using the latest fix diff only for navigation. Source was not modified; this report is the sole owned write.

Boundary: authenticated registration → permission/audit decision → reference resolution and tenant transaction → inline Prompt version selection and exact Agent normalization → persisted relationships → local Rust/Python/TypeScript hydrated navigation and invocation. CodeGraph exploration preceded code searches; the supplied map was expanded through route authorization, Caller construction, SQL exact lookup/insert/version locks, RLS, reference binding/recheck, bundle validation, and language projections.

## Authority coverage

| Authority | Domain obligation | Assessment |
|---|---|---|
| Approved `changes/active/agent-prompt/spec.md` r2, REQ-002/003/005, INV-001/003, AC-002 | Authenticated tenant/effective space, ordinary version/conflict rules, exact references, atomic writes, unchanged permission/audit and local-read boundaries | PASS within this domain |
| Original `tasks/TASK-001-agent-prompt.md` and R1 remediation | Existing registration and hydration owners; current-operation Pending eligibility only; original exact Agent pin preserved | PASS |
| Owner direction recorded in R1 and immutable subject | FIND-TASK-001-1 deletion interleaving is accepted risk; no new lock/recheck/concurrency-test obligation | Applied without reopening |
| `AGENTS.md` §§2/4/9/10/11 and `architecture/agent-rules.md` | Server-owned durable decisions, typed identities, TenantConn/RLS, validated trust boundaries, non-blocking audit | PASS within this domain |
| `architecture/wyrd-security-posture.md`, security principles, trust boundaries, authorization, tenant isolation and audit | Verified credentials select tenant; receiving route authorizes; SQL remains RLS-bound; audit cardinality follows decisions | PASS |
| `architecture/wyrd-design.md` doctrine 17/reference forms and `architecture/wyrd-doctrine.mdx` | Registered Agent inline Prompt gains ordinary independent identity; local/path/other inline semantics retain their boundaries | Approved scoped exception implemented |
| Reference router, spec-driven-development and maintainer-style | Source-backed immutable acceptance review and concrete consequences; no speculative hardening | Applied |

## Source and proof coverage

| Boundary / attempted failure | Source evidence | Proof assessment |
|---|---|---|
| Implicit Prompt creation bypasses authorization | `components/cards/routes.rs:433-450` checks `card:write` before the sole production `service::register_card` entry. Materialization runs later in `RegistrationWriter::write:1127-1135`. `audit/mod.rs:118-130,150-180` checks and stages allowed/denied decisions before proceeding, without awaiting persistence. | `pg_card_registration_route::inline_agent_prompt_materializes:4136` uses a viewer and checks refusal plus absence of Agent, generated Prompt and operation writes. No additional permission decision is introduced by an internal child write, so no additional audit event is required. PASS. |
| Generated child changes tenant or caller attribution | `components/auth/caller_extractor.rs:55-66` derives tenant from the verified principal. `service.rs:1091-1107` creates its own operation UUID and opens Caller-bound TenantConn; generated Prompt uses that same connection and Caller. `inline_prompt_submission:1582-1605` copies effective space and validates the derived name; it accepts no tenant selector. `queries/cards/register.rs:298-361` inserts `wyrd.current_tenant()` and bound attribution. | `register_and_hydrate::agent_prompt_registration_identity:432-494` repeats identities in a second authenticated tenant, asserts different UIDs, and refuses foreign exact reads. PASS. |
| Widened version query selects another request's Pending row or another tenant's row | `version_resolve.rs:49-90` applies `(active OR (pending AND registration_operation_id = bound operation UUID))`, then kind/space/name/stable-version bounds. `resolve_existing:1415-1485` is its sole production caller, threading the writer-created operation ID; that ID is not obtained from request fields. Both auto and scoped modes use the predicate. `TenantConn` binds transaction-local tenant state; `20260601000006_cards.sql:104-111` forces RLS with tenant USING/WITH CHECK. | `inline_agent_prompt_reuses_transaction_selection:4462` covers identical repeated selection, authored same-request Prompt reuse, different-body allocation, and atomic conflict. Other-operation Pending exclusion is established directly by the bound predicate and caller provenance, rather than a newly executed isolation test. No public read query is widened. PASS. |
| Exact-Agent lookup borrows another identity or weakens drift checking | `pin_existing_inline_prompt:1620-1645` requires exact Agent version and space and calls `find_card_by_ref` with Agent kind/name/space/version. The query in `get.rs:20-27,147-167` binds the full identity under the existing TenantConn. Only a stored Prompt ref matching generated kind/name/space supplies the child version. `persist_node` then uses ordinary canonical hashes and exact conflict comparison (`resolve_existing:1427-1449`), and the Agent receives the selected UID-bearing outcome. | `inline_agent_prompt_reapply_preserves_pin:4358` proves A@Agent1 → B@Agent2 → fresh-key A@Agent1 retains both UIDs and no extra Prompt; changed body/Agent field still conflicts without writes and the Service binds the original Agent. FIND-TASK-001-2 is closed for this boundary. PASS. |
| Authored ref/path input injects a foreign UID or server filesystem read | `EffectiveSpecs::resolve:157-198` resolves external identities under TenantConn; `resolve.rs:900-969` replaces submitted UIDs with resolved UIDs and refuses unresolved paths. `RegistrationWriter::write:1096-1100` uses the existing exact Active-reference recheck; `relationships.rs:36-68` binds exact identity/UID and FOR SHARE. Generated child identity comes from its persisted outcome. | Existing authored-reference controls are retained; path resolution remains client-loader owned. No new server file read, resolver or transport exists. PASS. |
| New generated edge reaches a foreign tenant or overwrites a name collision | `persist_node:1185-1234` reuses canonical hash, tenant-qualified version-line locking and normal insert/reuse. `version_sql.rs:63-88` includes tenant/kind/space/name in lock identity. Relationship migration `20260601000018_card_relationships.sql:19-25,39-47` has tenant-qualified source/target foreign keys and forced RLS. | Materialization and identity tests inspect ordinary independent Prompt versions, independently authored collision, derived-name rejection, and old Agent pins. No reserved namespace, overwrite mode, or separate version store. PASS, subject to the explicitly accepted deletion race below. |
| Local bundle or navigation fabricates identity, changes the selected target, or performs hidden IO | `state.rs:95-124` looks up the Agent's exact Prompt key in the existing typed map; `1639-1641` validates every Agent before publication. Bundle loading `1240-1530` confines paths, validates manifest/envelope identity and required UID, refuses unresolved path/sibling/missing refs, and checks relationships against spec. CardRef display includes UID. HydratedAgent accessors return borrowed local values. | Existing unit bodies cover inline-only old bundles, absent/non-Prompt targets, strict unknown alias and typed mismatch. Navigation journey reads after server shutdown. Local bundle integrity is structural, not a credential/signature claim; no new server authority follows from locally edited data. PASS. |
| Language wrappers or invocation replace the shared trust boundary | Python `src/state/mod.rs:1116-1146` obtains the shared state's resolved Prompt key and attaches an existing holder; `wyrd-cards/src/agent.rs:80-89,152-169` returns that local holder. TS native `cards.rs:597-619` projects the same shared relationship, and public `index.ts:2646-2677` wraps local envelopes. `observe/invoke.rs:40-58` uses the same Agent PromptCard while retaining its existing authenticated gateway caller. | Changed Python/TS navigation and typing tests, Rust offline navigation, and invocation consumer were inspected. No language-specific durable resolver, network/file access during navigation, tenant override, or authorization shortcut. PASS. |

FIND-TASK-001-3's corrected producer accounts for current-operation rows without widening eligibility to arbitrary Pending rows. FIND-TASK-001-4/5 concern public export/documentation rather than a new security boundary; their changes introduce no executable authority or data access.

The full changed-path diff includes no dependency/lockfile, deployment secret/configuration, token/crypto, redirect/webhook, subprocess, or external-fetch change. Changed generated names are validated with `CardName::new`, SQL values are bound, and no new credential values or secret logging were observed in the changed production code/fixtures.

## Security Audit

### Critical

None.

### High

None.

### Medium

None requiring correction within the approved scope.

### Low / Defense In Depth

No additional recommendation: optional hardening is outside this acceptance audit.

### Positive Controls

- Generated and authored Cards share the same Caller-bound TenantConn, version authority, canonical identity, lifecycle transaction and replay outcomes.
- Current-operation Pending eligibility uses a server-created, bound operation UUID under forced tenant RLS; public reads remain unchanged.
- Existing route permission and non-blocking audit owners remain intact; authored references retain exact UID binding and Active recheck.
- Shared local graph validation precedes publication; language navigation consumes the same exact relationship without writes.

## Accepted risk and verification limits

FIND-TASK-001-1 remains the owner's expressly accepted concurrent-deletion risk: a matching reused generated Prompt can be deleted during registration, leaving an Agent whose Prompt cannot be hydrated. This report does not reopen it or require locks, Active rechecks, or a concurrency test.

Static source review only. No Cargo builds/tests or live requests were executed. Inspected actual test bodies and R1's recorded focused GREEN results, all 30 registration-route passes, original identity regression and tenant-isolation check; those results were not independently rerun. No current-round peer reports or completion summary were used.

Skipped broad suites, dependency advisory research and unrelated repository debt. Residual risk is the accepted deletion interleaving and the limits of static verification; no new domain finding or blocking open question remains.
