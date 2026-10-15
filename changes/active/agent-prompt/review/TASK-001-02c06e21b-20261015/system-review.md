# System resilience review

Result: **PASS**. No material system-resilience findings established.

Subject: cumulative `c1aac2d132b19848ca1940c3907e1fa68a8d08a5..02c06e21bd5fd613341935a843fabe5350ea0a9d`. HEAD was checked at the candidate. Reviewed source remains unchanged; this report is the only owned output.

## Authority and deployed path coverage

Applied approved spec revision 2, original TASK-001, AGENTS.md, agent rules, protocol registry lifecycle and reference rules, public doctrine, spec-driven development, maintainer style, architecture constraints and architecture patterns. CodeGraph was used before locating code; its starting map was expanded into route authorization, SQL transaction/version locks, upload initialization, Card completion, reconciliation, and language projection owners.

The serving topology remains one logical `wyrd-server` surface, potentially multiple replicas, sharing tenant-qualified Postgres registry state and artifact storage. Cards, gateway, verification and other routes share `AppState`; generated Prompt registration introduces no process, readiness dependency, runtime, worker, queue or listener.

| Deployed path | Evidence and affected capability | Assessment |
|---|---|---|
| HTTP/SDK registration into server | `components/cards/routes.rs:427-451` authorizes before `service::register_card`; service `:554-577` uses existing registration owner | Generated child stays inside the existing authenticated registration capability. No new outage propagation or audit wait is introduced. |
| Inline Agent to durable Prompt and Agent | `service.rs:1086-1164`, especially `:1127-1140`; `persist_node :1179-1246`; `inline_prompt_submission :1563-1586` | Both rows, exact reference, relationship, operation record and replay seed share one caller-owned TenantConn commit. Generated child uses normal tenant/version/lifecycle machinery. |
| Concurrency/version identity | `service.rs:1197-1207`, `:1398-1466`; `wyrd-sql/.../version_sql.rs:63-85` | Existing transaction-scoped tenant/kind/space/name advisory lock selects Prompt versions. No in-memory cross-replica identity authority is added. Old Agent rows retain exact pinned references. |
| Post-commit publication | `service.rs:727-855`, `complete_card_inner :1875-1951` | Generated Prompt is included in response outcomes and enters the same metadata-only completion and blob activation path as existing Cards. Independent Card lifecycle is preserved. |
| Local hydrated state | `wyrd-client/src/state.rs:239-294`, `:85-179`, `:1607-1642`, `:954-984` | Graph construction checks every Agent relationship before publishing the immutable index; lookup returns existing local holders. |
| Python/TypeScript navigation | Python `src/state/hydrator.rs:79-92`, `src/state/mod.rs:1120-1151`; Cards `agent.rs:142-164`; TS native `cards.rs:569-609`, TS `src/index.ts:2612-2665` | Python retains the already projected Prompt holder; TypeScript projects stored envelopes and closes over the projected Prompt. Neither path adds remote lookup or registration. |
| Invocation | `wyrd-client/src/observe/invoke.rs:40-54` | Invocation consumes the same local Prompt relationship before entering the existing governed gateway path. Tool/config/gateway lifecycle ownership stays intact. |

## Failure paths, scope and recovery

| Failure/interruption | What stops and survives | Recovery and sibling availability |
|---|---|---|
| Invalid generated name or Agent persistence failure after child insert | The registration request returns its existing stable error; all uncommitted child, parent, relationship and operation writes roll back. `TenantConn` documents drop rollback (`tenant_conn.rs:29-35`); the only commit is after the complete graph (`service.rs:1162-1163`). | A corrected request can register normally. This is request-level refusal, with no process exit or readiness mutation. |
| Postgres acquisition/write/commit unavailable | Registration fails through existing structured registry errors. An ambiguous commit can leave the entire graph and durable seed committed, never only a generated Prompt from the transaction. | Same-key retry consults the stored operation (`service.rs:654-700`). No new process-local state must be reconstructed. Existing shared Postgres dependence affects its ordinary consumers; the candidate adds no failure escalation. |
| Cancellation/process death before transaction commit | Uncommitted graph/operation state does not survive; version locks are transaction scoped. | Retry repeats normal registration. No generated-child side effect precedes the transaction commit. |
| Death/lost response after commit and before activation/response | Committed Pending Prompt and Agent rows, exact relationship and replay seed survive in Postgres. `insert_card_row` initializes Pending reconciliation state (`wyrd-sql/src/queries/cards/register.rs:313-349`). | Same-key replay re-enters upload/completion using all stored outcomes (`service.rs:727-855`). Existing reconciler can also claim committed pending work; no special child rediscovery is needed. |
| Blob/storage failure after commit | Publication/activation fails at the Card/request boundary through existing completion, failure and cleanup handling (`service.rs:801-853`, `:1899-1928`, `:893-910`). Already active independent Cards can remain active, consistent with the existing post-commit lifecycle contract. | Existing reconciliation owner records/reschedules failures; generated Prompt has no artifact uploads. No new required health check or shared-server crash is introduced. |
| Replica replacement while reconciliation is pending | Durable row state and leases survive; no per-request Prompt cache owns recovery. | `claim_card_reconciliation` uses database lease state and `FOR UPDATE SKIP LOCKED` (`lifecycle.rs:66-89`). Reconciler `run/process_claim` (`reconciler.rs:20-129`) logs per-tick/claim failures, preserves retry/dead-letter handling, and observes shutdown between passes. No new worker mechanics were added. |
| Missing/wrong-kind Prompt, unresolved path or old inline-only bundle | Construction of that WyrdState is refused with `WYRD_SDK_400_INVALID_STATE_BUNDLE`; no partially published state or fabricated reference is produced (`state.rs:85-125`, `:1639-1642`). | Re-register/rehydrate the affected graph as explicitly required by REQ-005. This is bundle/component refusal, not permission to stop the shared server or unrelated state instances. |
| Server unavailable after hydration | Remote registration/invocation remain unavailable according to their existing dependency requirements; alias lookup and Agent-to-Prompt reads operate on local retained holders. | Reads need no server recovery and remain usable after shutdown. Language wrappers introduce no retry loop or dependency amplification. |

## Recovery and proof assessment

Source establishes reuse of the existing transactional and reconciliation owners rather than a new recovery protocol. The change adds one metadata-only Card per inline Agent; it does not alter retry/backoff, shutdown supervision, leases, service health or storage protocol. No material expanded failure boundary was found.

Inspected the added HTTP registration test (`pg_card_registration_route.rs:4108` onward), Rust registration identity/navigation journeys (`register_and_hydrate.rs:432` onward), Python/TS navigation journeys, malformed-bundle tests in the cumulative state diff, and existing production reconciler recovery test (`pg_card_registration_route.rs:1912-1985`). The added HTTP test checks child Active status, exact persisted reference/relationship, version reuse/change, old pin preservation, invalid-name rollback and permission refusal. Rust navigation explicitly shuts down the deployment before loading/reading the bundle. Existing recovery test exercises storage becoming available and production reconciliation reaching Active; it is supporting seam proof, not a generated-child crash test.

Task evidence records successful focused A/B/R/P/T/I commands and relevant owner checks. These were not rerun by this static reviewer, and claimed PASS labels were not used as substitutes for tracing source. No source or fault-injection test was changed by this review.

Residual verification limits: no generated-Prompt-specific forced process kill, post-commit response loss, concurrent collision, or storage-outage test was executed or located in the added tests. The existing protocol mechanics are reused unchanged, so this does not establish a new reachable recovery defect. Python and TypeScript tests redirect the ambient server URL instead of stopping their still-running server fixtures; they are weaker outage proof than the Rust shutdown journey. Their inspected accessors are synchronous retained-state projections, so no hidden IO path was found, but those tests alone should not be described as actual server-shutdown proof.

## Material findings

None. Finding IDs: none.

Skipped: builds/tests and live crash/outage experiments, per review coordination. Risk: recovery conclusions are source-based with existing lifecycle test support; per-language shutdown proof has the limits above.
