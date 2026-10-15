# TASK-001 concurrency and durability review

Subject: base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70` to candidate `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183`; immutable source `/tmp/wyrd-task-review-ef2fe8bd`. Source inspection only; no builds, tests, edits to the subject, or other reviewer reports. No CodeGraph index exists in the archive.

## Boundary and authority

This review follows values and retained state across invocation-local route construction, shared Agent/provider/judge handles, admission snapshots, credential replacement, batch claims, accounting, cancellation and the process Scribe outbox. It does not prescribe stronger outbox durability than approved authority.

Governing obligations: approved spec r5 REQ-002–004, REQ-009–010, REQ-012–013; INV-001–002 and INV-004–007; AC-002–003, AC-005, AC-007 and AC-012. Repository authority includes AGENTS §§2, 5–6, 9, 11–12; agent-rules (transaction ownership, effective-value validation and nonblocking audit); architecture/wyrd-design.md Agent/invocation, attribution and audit boundaries; the reference router, spec-driven-development, maintainer-style, architecture-constraints, patterns, Rust ownership and testing guidance. The approved in-memory outbox boundary is retained: abrupt death can lose unacknowledged audit/capture, without changing the initiating operation.

## Source and boundary coverage

All source paths below are relative to the immutable archive.

| Boundary | Producer → retained owner → consumers inspected | Assessment |
|---|---|---|
| Declared/effective route | `crates/shared/wyrd-client/src/observe/invoke.rs:36-126`: hydrated Card route or `invoke_on` argument → newly constructed local `Agent`/registry → `Agent::run_prompt`; Python/TS project this shared owner | Invocation does not write the hydrated Card or shared global registry. `&ExecutionRoute` stays immutably borrowed for the invocation; provider override belongs to its new Agent. PASS for isolation. |
| Full Agent tool loop | `crates/skald/skald-agent/src/agent.rs:294` provider override; `loop_runtime.rs:58-247,286-350` → local conversation/request → dispatch using effective registry | Registry selection occurs once before the bounded body; conversation and per-iteration request are local. Tool futures remain owned by the loop. Timeout/drop ends outstanding work; already written session/journal entries remain. PASS for ownership. |
| Gateway binding | `crates/skald/skald-agent/src/gateway.rs` `wyrd_gateway_registry` → immutable `WyrdGatewayProvider.model` → per-call `WyrdGatewayCall` | No shared route/deadline mutation. Calls use an immutable fixed address. Its cancellation token is never fired by this adapter, but outer timeout/drop propagates by dropping the public exchange or server invocation, whose drop guard cancels its tracked attempt; no detached provider work is introduced here. PASS for cancellation ownership. |
| Public and server callers | `crates/shared/wyrd-client/src/gateway_caller.rs`; `crates/wyrd/wyrd-server/src/components/gateway/caller.rs` | Client transport/auth and captured server authority are shared, while native request/deadline/answer are local. Public caller races cancellation and timeout; server caller uses ordinary invocation settlement. Protocol target-projection correctness is separately referred to the orchestrator, outside this report's state-race finding. |
| Judge cache/media | `crates/vala/vala-eval/src/orchestrator/judge.rs:60-235` → mutex-protected exact judge-ref/Arc<Agent> cache → per-call copied Prompt/media bindings | Cache lock spans resolution but is released on cancellation; only fully constructed Agent is published. Different judge ref is explicitly refused. Per-record media never changes cached Prompt. PASS. |
| Snapshot consistency | `gateway/service.rs:657-678` → SQL `gateway_snapshot` / `SNAPSHOT_SQL` → `GatewayTenantSnapshot`; `gateway/invocation.rs:481-523` → `CallPlan`/`AdmittedCall` → engine | Single SQL statement reads resource documents and opaque credential envelopes consistently. Normal call authorization, fallback filtering, routing and credential resolution use the same owned snapshot. PASS except batch bypass below. |
| Credential rotation/ref integrity | `gateway/service.rs:95-137,271-342`; `wyrd-sql/src/queries/gateway.rs:26-95,381-490`; defining migration composite credential FK; `wyrd-gateway/src/credential.rs:104-178` | Active credential replacement stores provider/source/ciphertext atomically. Deployment validation holds `FOR SHARE` through commit; composite tenant/name/provider FK prevents orphaning identity. Resolver uses snapshot credential and adapter identity; managed envelope retains tenant/name/provider binding. Existing admitted calls retaining old state is intentional. PASS. |
| Secrets and resources | `wyrd-gateway/src/credential.rs:196-204`; `shared/wyrd-utils/src/secret.rs`; engine `attempt` and adapter request ownership | Per-attempt secret is owned until attempt returns/drops. Mounted secret uses metadata on opened handle and bounded read. Blocking read is on blocking pool. No task-owned plaintext is stored in a shared route or registry. PASS for changed boundaries. |
| Admission/spend concurrency | `gateway/ledger.rs:140-261,278-437,599-610`; `gateway/invocation.rs:809-860`; SQL admission lock, leases and accounting fences | Tenant advisory lock serializes admissions across replicas. Bounds now count one attempt per named candidate, matching engine traversal; reservation/hold writes share caller commit. Accounting uses admitted pricing and attempts, with durable fence/reconciliation. PASS. |
| Cancellation after admission | `gateway/invocation.rs:395-434,1048-1243` `CallTask`; `wyrd-gateway/src/engine.rs:468-625` | Cancellation guard lives across awaiting answer; tracked execution owns snapshot, plan, admission and settlement. Provider attempt records dispatch possibility on interruption. Buffered response awaits accounting, stream accounting waits for terminal relay. Capture staging is after response and deadline-bounded. PASS. |
| Provider-held batch state/replay | `gateway/batches.rs:293-355,383-517,737-789`; `routing.rs:113-127`; SQL claim and pinned deployment fields | Existing claim fence prevents duplicate create; uncertain dispatch retains claim, undispatched failure releases it, and pinned follow-up cannot fallback to another deployment. However its pre-authorized boolean crosses a snapshot replacement and permits a newly restricted target: CONC-001. |
| Audit/capture retries/restart | `gateway/invocation.rs:701-750,950-980`; `gateway/capture.rs`; `server/src/scribe_outbox.rs` staging, projection and `ScribeSink::write`/stable batch identity | Decisions and captures stage on existing process owner. Frame identity is content-derived, retry resubmits identical slice, terminal rejection is consumed/logged/counted. No new WAL/queue or synchronous audit acceptance was added. Ledger persistence remains distinct from lossy capture. PASS. |
| Dev/setup interruption | CLI deployment PUT file owner; per-resource server PUT/credential commit boundaries | List writes are intentionally individual commits; interruption leaves completed resources, rerun uses same PUT names. No cross-request transaction is claimed. Task evidence reports restart/key reuse scenario T13; not rerun here. PASS for scoped state semantics. |

## Failure-path assessment

| Interleaving or interruption | Established behavior | Result |
|---|---|---|
| Two simultaneous effective routes on the same hydrated Agent | `Run::invoke_on` constructs distinct Agents and registries; neither writes Card/default registry | PASS |
| Judge media for concurrent records | Media binding clones Prompt before replacement, cached Agent remains unchanged | PASS |
| Credential rotation/revocation versus deployment PUT | Row share lock plus atomic update/FK enforce valid committed binding | PASS |
| Administrative replacement after ordinary call snapshot | In-flight call retains old consistent config; subsequent call loads new config | PASS |
| Batch preclaim authorization followed by deployment replacement | Later admission reloads config and skips its requested decision | FAIL, CONC-001 |
| Caller drops during provider call | Invocation guard cancels task-owned attempt; tracked task still settles ledger/capture | PASS |
| Cancellation before/after uncertain batch create | No-dispatch failure releases claim; dispatched uncertainty stays pending to prevent duplicate remote object | PASS |
| Process death after reservation or before accounting | Durable reservations/leases remain; expiry and reconciliation conservatively settle unknown spend | PASS within existing contract |
| Scribe retry/terminal failure or abrupt death | Identical replay IDs; terminal loss counted; unacknowledged in-memory loss allowed and initiating operation unaffected | PASS |

## CONC-001 — Batch authorization uses a different deployment snapshot from dispatch

Classification: **INCORRECT**. Blocking concurrency/security consequence. Violates REQ-002 (same invoke coverage for batch access, requested target authorized before upstream IO), INV-007 (no credential escalation), and the effective-value rule in architecture/agent-rules.md.

Exact changed boundary: `crates/wyrd/wyrd-server/src/components/gateway/batches.rs:410-415` supplies snapshot-dependent `decide`; `:438-441` refreshes a replay with `authorized=true`; `:490` executes a claimed creation with `authorized=true`. Consumer: `components/gateway/invocation.rs:481-486` loads a new snapshot and skips requested authorization. The new `DeploymentAccess::Granted` check in `invocation.rs:713-737` makes this boolean insufficient: permission now depends on the deployment document, not just the captured principal/address.

Reachable sequence:

1. A caller holding only `gateway:invoke all` uploads a batch input on deployment `d`, whose access is `open`.
2. `GatewayBatches::create` reads snapshot A and its decision allows `d/model`. It then awaits claim/transaction work.
3. An authorized administrator replaces `d` with `access=granted` (and may replace its credential or compatible endpoint) and commits.
4. Creation settlement, or completed-create replay refresh, enters `GatewayInvocation::run(..., true, ...)`. It reads snapshot B containing that replacement, skips the requested decision, builds the pinned plan from B and dispatches using B's credential.
5. The all-only caller reaches a granted deployment for which it has no scoped grant. Its recorded allow was evaluated against A, while the actual target configuration and credential come from B.

This is different from intentionally allowing an already-admitted call to finish on its old snapshot: the implementation mixes A's authorization with B's configuration. Multiple await boundaries between the loads and independent per-deployment PUT make the interleaving reachable without unsupported hooks or global mutation. The regular invocation path has neither split.

Sibling paths traced: normal upload, retrieve, cancel, file-content and delete-file call admission with `authorized=false`, so they check their actual snapshot. File metadata/listing each checks the snapshot used for visibility without upstream dispatch. The two bypass paths are fresh creation settlement and existing-create replay refresh; both derive from this one pre-authorization boolean. No other production caller supplying `true` was found in the gateway owners.

Smallest safe correction: reuse `GatewayInvocation::decide` at actual call admission against its own snapshot for every dispatched batch call; retain the existing preclaim decision so unauthorized requests still cannot claim a creation. Remove the requested-decision bypass plumbing once its only users are gone. Both actual evaluations must stage their decisions through the existing outbox. This needs no new permission type, lock, cache, transaction lifetime, credential semantics, or API. Existing undispatched-refusal handling already releases a newly claimed batch when admission refuses; preserve the pending-claim fence and tracked settlement for uncertain dispatch.

Focused proof: extend the existing server/Postgres batch owner with a deterministic interleaving using its existing claim transaction seam: hold the claim insert while snapshot-A authorization completes, PUT `d` to `granted`, release the insert, and assert scoped denial before any provider create/refresh request, denied audit and no stranded fresh claim. Include completed-request replay through the same gap. Retain scoped-grant success and existing no-double-create/uncertain-dispatch behavior. A controlled private seam or database barrier is preferable to sleeps. Run the new named exact nextest selector through the existing managed-Postgres wrapper; the existing `components::gateway::pg_invocation_tests::gateway_batches_converge_replays_on_one_upstream_batch` and `...::gateway_batch_creations_release_claims_only_without_dispatch` are the nearest regression owners. No command was run in this review. Exact existing regression commands for remediation (not executed here):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::gateway_batches_converge_replays_on_one_upstream_batch)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_invocation_tests::gateway_batch_creations_release_claims_only_without_dispatch)'
```

## Proof assessment and limits

Recorded task evidence claims exact T08/T09 concurrency route tests passed. Source confirms T08 joins declared and overridden public calls and checks distinct upstream credentials and unchanged Card route; T09 joins two independently bound Agents across a tool iteration. They support invocation-local registry ownership, not snapshot-A/B authorization. Credential lifecycle/snapshot tests and existing batch replay/claim tests cover their narrower seams; no inspected test holds the batch claim across an open-to-granted replacement.

No runtime reproduction, test discovery, build, lint or timing stress was performed, as instructed. No source was edited. Review does not demand replay or durable storage for unacknowledged outbox writes. Protocol target-projection concerns were routed to the orchestrator for behavior review rather than duplicated as a concurrency finding. Remaining risk: CONC-001 permits batch IO under a stale deployment-access verdict.

**FAIL — CONC-001.**
