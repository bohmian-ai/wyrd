---
id: TASK-012
status: proposed
approved_spec_revision: 12
requirements: [REQ-015, REQ-016, REQ-017, REQ-018]
acceptance: [AC-010, AC-011, AC-012, AC-013, AC-014]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: []
---

# Integrate code mode without losing dynamic pipelines

A developer can use generated Python to gather evidence through declared tools
inside B's existing dynamic Verifier pipelines. Steps, fanout, repeats,
speculation, approvals, deferred inputs, queued recovery and GitHub delivery
keep their behavior. The port also supplies the shared resource ledger, reusable
workers and workspaces, streamed evidence, throughput fixes, instrumentation,
three-language benchmark consumed by TASK-013 and TASK-014.

## Authority, inputs and integration choice

This task derives from approved GitHub pipeline spec revision 12 and contracts,
code-mode spec revision 9 as amended by pipeline revision 12, and the settled
maintainer directions recorded in TASK-013 and TASK-014. Revision 12 decides
nested exclusions, crates.io Monty, capacity isolation, recovery spend and the
contributor-contract reconciliation. The current directions also replace A's
200-tenant requirement with 100 tenants and assign two-replica proof to
TASK-014. Reconcile those contracts at their owners before affected
implementation. The present planning handoff changes only tasks and their index.

Inputs are B at `fa7c86dba8c3148a9621248dac94a7353ead24c8`, A at
`0eb5ccdd9ac0b7b35fa4d4f3c6afd8e091a176f0` (`change/verifier-code-mode`), and
merge base `11474b4a9aa95499c3fbd3a69393ff7590a5b245`. Revalidate identities
before execution; record any maintainer-selected successor, never silently
change the imported range. Read A's tracked packet through the pinned tree, at
`changes/active/verifier-code-mode/`: `spec.md`, `design/notes.md`,
`design/capacity-diagnosis.md`, TASK-001/002 and TASK-003/R1/R3 task files,
their cited review evidence and `spec-revisions/`. Read with `git show
change/verifier-code-mode:<path>` after confirming that ref still matches the
pinned A commit; otherwise read the pinned commit directly. B's applicable
supplements are `design/approved-nonblocking-execution.md`,
`design/task-010-benchmark-remediation.md`, and its runtime, extraction,
deferred-input and report-delivery agreements.

Use a **staged port of A's committed delta from the merge base into B's
owners**. B contains the larger orchestration workflow; reviewable,
owner-by-owner composition makes preservation explicit and avoids installing A's
incompatible capacity policy as an intermediate accepted implementation. This
costs a source-to-port coverage record instead of recording A as a merge parent.
Retain source commit identities and account for every A-only change as carried,
adapted or excluded with an authoritative reason. Do not cherry-pick shared
resource-scaling ancestry again or port only a chosen subset of A's runtime. Git
scheduling and commits remain maintainer-owned; this task authorizes no staging,
commits, checkout, reset, stash, merge or push.

The integration inventory and verification recipe are in
`/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode/target/codex-merge-plan.md`,
sections 2–7. Resolve all 37 listed textual paths, inspect the 38 shared
textually clean files and all 17 semantic conflicts; textual cleanliness is not
acceptance. Its reserved-decisions section is superseded by the decisions above.
Use its file table as a coverage inventory, not an instruction to replace whole
files. Preserve the local modified task index and untracked TASK-011 exactly
apart from separately authorized index additions. Exclude A's untracked
payload-sizing packet and unrelated software-factory work.

## Existing owners and required composition

Extend the complete existing workflows, rather than introduce another engine,
dispatcher, persistence service or resource authority:

- `vala-verify/src/blocks/{engine,agent,tools}.rs` and `blocks.rs` retain B's
  logical step/item/iteration/attempt identities, producer restrictions and
  checkpoint flow. Add A's tool ownership and settlement around B's borrowed,
  unspawned block futures. Keep B's `walk.rs` scheduling during this port;
  TASK-014 owns any later execution redesign, without a preselected scheduler. Cancellation dropping a block does
  not prove already-issued host work has settled. Settle it before retry,
  result selection, checkpoint or release of its resource ownership.
- One direct/nested tool dispatcher preserves declared-name and typed-argument
  validation, per-call tool/data permissions and nonblocking audits, bounded
  arguments/results, shared request/concurrency budgets, start-order records,
  recipient tags, original evidence/citations and check-Agent records. Forward
  B's provider `call_id` and producer through the recorder: ordinary model
  publication remains replay-safe and step-only. The nested bridge excludes
  `code.run`, `fs.write` and `publish`; Python already writes private `/work`,
  and nested publication has no provider call identity. **Decided in revision 12: nested `fs.write` and `publish` are excluded.**
  Ordinary declared model calls retain both tools; check Agents still cannot
  publish. Nested publication identities are outside tasks 012–014.
- Restore tool-call spend through the same attempt records B uses for
  `max_agent_invocations` after `db5520e38`. Count an unsettled attempt's whole
  tool allowance as spent; accepted settled attempts restore their counted
  spend. Share allowance across authored retries and preserve the original
  deadline across park/takeover. No per-call persistence or direct resumable
  state is introduced.
- `wyrd-server/src/verification/hosted/` composes A's reusable fresh-session
  sandbox, runnable-turn bound and charged streamed workspace with B's mounted
  publications/collections, extraction, writes and immutable snapshots. Charge
  growth before mutation, preserve replacement/no-overwrite distinctions and
  unchanged content on refusal, and release scratch only after deletion or
  ownership transfer. Keep suspended session envelopes charged through reset or
  reap; retain charges for surviving references after cache eviction.
- The existing `wyrd-runtime` memory/scratch owner remains the single process
  ledger, including Bifrost, edge bodies, live queued state, frozen copies,
  encoding and terminal/outbox custody. Reserve before copying, transfer charges
  without gaps or double promises, and retain B's memory-first progress,
  bounded background group commit, ACK-before-retirement and claim backpressure.
  Do not add persistence waits to engine mutations or direct verification.
- Keep arrival-based direct deadlines through target lookup/admission/model
  work, lowered by the authored timeout. Retain B's verification-only gateway
  capture bypass, destination-batched direct evidence, individually fenced
  queued ACKs, mode-aware target cache and invalidation, queue-only refusal
  before steps, compact/details distinction and HTTP/MCP-only inspection.
  Preserve RBAC-only verification authorization and `verifier:approve`.
- Monty stays on its crates.io release; exclude A's Monty `[patch.crates-io]`
  entries, their git-source type-checking patch, and the fork-only sandbox code
  that depends on them (the `service_mounts`/`resume_serviced` split), recording
  each as excluded by maintainer decision in the coverage record. Keep the
  sandbox's runnable-turn bound working against the crates.io API. Keep B's
  pinned code-review library; resolve lockfile, workspace-hack
  and feature graph together. Benchmark dependencies and instrumentation stay
  behind diagnostic builds and out of production SDK packages. Compose authored
  Python stubs/exports and native sources; regenerate wire types, errors,
  loaders/declarations, testing declarations and docs from final sources.

## Capacity boundary and architecture handoff

**Isolation is necessary for a correct port, so its minimum execution boundary
lands here rather than waiting for TASK-014.** A shared reusable Python pool or
shared runtime would weaken the maintainer's direct protection. Keep separate
direct/queued sandbox pools; route queued engine, model and Python mediation
onto its own long-lived runtime with a bounded CPU share and memory reservation.
Preserve direct reserved capacity that queued work never consumes. Direct may
borrow idle queued capacity only without delaying direct reclamation; lack of
safe borrowing is not permission for queued borrowing. The ledger accounts both
reservations and actual ownership once. Admission is tenant-fair and replica
budgets are local and non-overlapping, never independently promised host memory.

Separate runtime thread counts or byte/slot lanes alone do not establish CPU
isolation: include queued engine work, model callbacks, blocking work, Python
workers and nested tool work in the bounded share. Use portable mechanisms
consistent with root rules; no OS isolation controls or platform-specific path.
Record the bound, enforcement point, in-flight reclamation limits and measured
effect. If this cannot be enforced within the agreed boundary, report that
specific blocker before claiming the port correct; do not reinterpret isolation
as merely reserved permits. TASK-014 owns the full target capacity design,
optimization and final saturation qualification, not repair of a known unsafe
port.

Before affected changes, reconcile only the shipped contract in
`architecture/wyrd-design.md` (Verifier / Agents, tools, and workspaces),
`architecture/bifrost-design.md` (Admission and memory / Resource and failure
invariants) and the accepted-authority security section. B's orchestration,
recovery, GitHub and Repository contracts stay normative; A's `call_tool` and
ledger mechanics stay normative. Remove forbidden OS-identity/network-isolation
wording and conflicting no-borrowing/dynamic-sharing claims; describe portable
interpreter/allocator limits and parent-mediated access honestly. TASK-014
rewrites the in-revision direct/capacity design; do not claim HA qualification
from functional integration. Keep Repository as kind 13, including the stale
architecture count. Reconcile the exact six-control union (`flush_bifrost`,
`wait_for_baseline`, `make_binding_due`, `hold_publications`,
`occupy_hosted_memory`, `wait_for_hosted_waiters`) across REQ-195, contributor
guidance, Rust guard, Python and TypeScript. Preserve strict detection of every
unsanctioned control, blocking wait semantics and production exclusion.

## Outputs, obligation coverage and observable proof

| Obligation source | Trigger and required observation |
|---|---|
| Code-mode revision 9 functional requirements/invariants and AC-001–018; diagnosis conflicts 1–3,7–8,11,13 | Real declared Agent step and check call nested tools inside fanout/repeat; dependent calls complete at concurrency one, record recipients and retain citations. Preserve the existing reduced-evidence efficiency journey and its byte bound, least-privilege recovery and authored-code isolation. Model-originated publication replays its original receipt; nested write/publication and check publication refuse. Cancel/timeout during host work, retry, park and take over: settled work and receipts persist, unsettled attempts consume their allowance, authority/deadline never replenish, and ownership releases only after settlement. |
| Pipeline REQ-001–014/016, AC-001–009/011 and INV-001–007; conflicts 9–13,16 | Retain connect→Repository→PR admission→pinned extraction→review→approval/deferred evidence→same execution→report/Check/comment journeys and negative flows across shipped surfaces. Source supersession/access loss fences continuation and delivery; provider retry never recomputes verification. No newly added SDK/CLI inspection where HTTP/MCP alone is sanctioned. |
| REQ-015/017–018, AC-010/012–014; code-mode resource requirements and AC-028–034; conflicts 1,4–6,10,12,14 | Pressure on publication mounts, scratch replacement, queued snapshots and held persistence remains fully charged and bounded; compute frees before queued ACK without losing custody. Saturate queued CPU/model/Python/engine work and compare direct c64 latency and throughput with direct-only under identical reserved settings: no degradation beyond measured run variability. No warm-cache direct SQL/state, queued-only cache bypass, lost write, starvation or premature release. |
| REQ-195 and both packets' projection/isolation obligations; conflicts 14–17 | Exact six-control guard rejects any additional hook in all projections. Dependencies resolve against crates.io Monty with no fork patch; regenerated consumers compile, historical records decode, production packages exclude testing/benchmark machinery. |
| Diagnosis sections 5/7 and inherited TASK-003/R1/R3 obligations | Retain A's tracked packet and referenced evidence in B's change tracking with source identities and an explicit carried/changed/pending disposition for every inherited row. Keep TASK-003-R2 superseded and TASK-003-R3 open. TASK-013 owns the Rust-only 100-tenant mixed comparison; TASK-014 owns sustained 100-tenant saturation/recovery, remaining qualification, artifact analysis and two-real-replica evidence. Record the 100-tenant amendment against inherited REQ-033/AC-034 and R3 V6 rather than retaining their older threshold; no historical FAIL/UNVERIFIED becomes PASS by importing it. Preserve the shared resource-scaling packet, historical deprecated verifier-review packet and B's intentional execution/inspection distinctions. Reconcile packet coverage/status only against actual evidence, never invent review verdicts. |

Use diagnosis section 6's functional coverage on the composed tree, subject to
this task's superseding decisions and scoped verification below: real SDK →
WyrdTestServer + repository Postgres → SDK in Rust/Python/TypeScript; real
HTTP/MCP/CLI at their existing owners; engine unit/integration and server/SQL
seam tests. Add combined-path tests described above at those owners, not new
test binaries. Record each new test's actual focused command before RED.

Existing exact selectors (the code-mode selector arrives from A; build the
worker executable first):

```bash
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_durable_verifier_work::only_queued_verification_keeps_run_state)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations)'
```

Combined server seam coverage, retaining the existing target and module names:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E 'test(/^(pg_durable_verifier_work|pg_hosted_artifacts|pg_hosted_files|sandbox_worker|pg_verification_routes|pg_verification_runtime|pg_github_pull_requests|pg_github_pull_request_extraction|pg_deferred_evidence|pg_approvals|pg_github_delivery)::/)'
```

Regenerate with `mise run codegen:regen`, `mise run ts:build`, `mise run
ts:build:testing`, `mise run docs:generate`. Use the combined-runtime selector
above and affected owner lanes: `test:verification:sandbox`,
`test:principals:integration`, `test:rust-sdk:journey`, `py:test:integration`,
`ts:test:integration`, `test:bifrost`, `test:sql`, plus affected MCP/CLI
journeys. Run `mise run check`, `check:deps`, `check:tenant-isolation`,
`codegen:check`, `py:lints`, `py:typecheck`, `ts:lints`, `ts:typecheck`,
`ts:napi:check` and `docs:check` through their existing mise tasks; format
changed languages. Explicitly inspect generated testing declarations: public
`ts:napi:check` does not cover them. Full shipped-surface suites and any
required broad aggregate belong to integrated change review. These are future
implementation commands, not planning-run actions.

After functional verification, the maintainer supplies an immutable composed
candidate. Capture full clean, quiet-host, unprofiled `mise run
bench:l1:verification`, `mise run bench:l1:gateway` and `mise run
bench:l1:bifrost`. Record exact tree/dependency/build identities, configuration,
resource assignments, fixture/method and every repetition; preserve
Rust/Python/TypeScript c1/c64 and matched-rate diagnostics, achieved server
concurrency, arrival lateness/GC, latency/overhead phases, parent plus
live/reaped worker CPU/memory/churn, offered/accepted/completed/refused counts,
evidence reconciliation, backlog/drain, memory/scratch and queued custody. Use
existing functional pressure journeys for the port's isolation boundary. The new
standard mixed benchmark and its baseline belong to TASK-013; sustained
saturation and two-replica qualification belong to TASK-014. Baseline validity
and target attainment are separate: record misses honestly, without declaring
either packet complete. Historical parent-only CPU stays parent-only. Dirty,
smoke, profiled or selected-step runs cannot qualify.

Deliver the composed source/contracts/projections, conflict coverage and
inherited-obligation dispositions, functional evidence, and immutable benchmark
records with exact commands to TASK-013 and TASK-014. Local diagnostic artifacts
referenced by the diagnosis must have retained, readable evidence or be marked
unavailable and rerun; a vanished target path is not closure. No source
modification of external reviewed libraries, payload-sizing redesign or nested
publication identity extension belongs here.

## Obligations

| ID | Promise (contract source) | Planned proof |
|---|---|---|
| OBL-1 | Port integrity: A's delta `11474b4a9..0eb5ccdd9` is ported into B owners; every A-only path is carried, adapted or excluded with a reason; source identities retained; A's payload-sizing and software-factory work excluded; local task index and TASK-011 untouched (task: integration choice) | Coverage record below; `git diff` audit |
| OBL-2 | Contract reconciliation before affected code: wyrd-design Verifier/Agents/tools/workspaces, bifrost-design Admission/memory, security accepted-authority; no OS-identity/network-isolation wording; no conflicting borrowing claims; Repository kind 13 incl. stale count; six-control union in REQ-195 and contributor guidance (REQ-015, rev 12) | Content review of owner sections |
| OBL-3 | Engine: B's step/item/iteration/attempt identities, producer restriction, checkpoints and `walk.rs` scheduling kept; A's tool ownership settles host work before retry, result selection, checkpoint or resource release, including when a block future is dropped (AC-010, INV-001..003; code-mode AC) | vala-verify unit/integration tests incl. cancel/timeout during host work |
| OBL-4 | One direct/nested dispatcher: name/typed-arg validation, per-call tool/data permission + nonblocking audit, bounded args/results, shared request/concurrency budget, start order, recipient tags, citations, check-Agent records; provider `call_id` and producer forwarded; nested bridge excludes `code.run`, `fs.write`, `publish`; check Agents cannot publish (rev 12) | blocks_agent/tools tests; hosted tools pg_tests; code-mode journey |
| OBL-5 | Tool-call spend restored from attempt records like `max_agent_invocations`; unsettled attempt counts whole allowance; allowance shared across retries; original deadline kept across park/takeover; no per-call persistence (rev 12) | engine recovery tests; pg_durable_verifier_work takeover |
| OBL-6 | Hosted workspace: A's reusable fresh-session sandbox, runnable-turn bound, charged streamed workspace composed with B's publication/collection mounts, extraction, writes, immutable snapshots; charge before mutation, replacement vs no-overwrite, unchanged on refusal, release after delete/transfer; suspended envelopes charged until reset/reap (REQ-015, code-mode resource reqs) | workspace tests/pg_tests, sandbox_worker, pg_hosted_* |
| OBL-7 | One `wyrd-runtime` memory/scratch ledger covers Bifrost, edge bodies, live queued state, frozen copies, encoding, terminal/outbox custody; reserve before copy; transfers without gap/double count; B's memory-first progress, group commit, ACK-before-retirement, claim backpressure kept; no persistence waits on engine mutation/direct path (REQ-017/018, AC-012/013) | runtime/state unit tests; pg_durable_verifier_work |
| OBL-8 | Direct path: arrival-based deadline lowered by authored timeout through lookup/admission/model; verification-only capture bypass; destination-batched direct evidence; individually fenced queued ACKs; mode-aware target cache + invalidation; queue-only refusal before steps; compact/details; HTTP/MCP-only inspection; RBAC-only authz and `verifier:approve` (REQ-016, AC-011) | pg_verification_routes/runtime, existing selectors |
| OBL-9 | Isolation boundary: separate direct/queued sandbox pools; queued engine/model/Python/nested tool work on its own long-lived runtime with bounded CPU share and memory reservation; direct reserved capacity never taken by queued; direct may borrow idle queued capacity only without delaying reclamation; ledger counts reservation and ownership once; tenant-fair admission; replica-local budgets; portable only. Record bound, enforcement point, reclamation limits, measured effect (REQ-015, AC-014) | pressure journeys; direct c64 with/without queued saturation |
| OBL-10 | Dependencies: Monty on crates.io, no `[patch.crates-io]`/git type-checking patch/fork-only `service_mounts`/`resume_serviced`; runnable-turn bound works on crates.io API; B's code-review pin kept; lockfile/workspace-hack/features coherent; benchmark deps diagnostic-only and out of SDK packages (rev 12) | `check:deps`, Cargo.toml inspection, package contents |
| OBL-11 | Projections: authored Python stubs/exports and native sources composed; wire types, errors, loaders/declarations, testing declarations, docs regenerated; historical records decode; six-control guard rejects any extra hook in Rust/Python/TS with blocking waits and production exclusion (REQ-195) | codegen:check, ts:napi:check, testing d.ts inspection, guard tests |
| OBL-12 | Pipeline journeys preserved: connect→Repository→PR admission→pinned extraction→review→approval/deferred→same execution→report/Check/comment and negatives; supersession/access loss fences; provider retry never recomputes (REQ-001–014/016, AC-001–009/011, INV-001–007) | combined server seam selector; SDK journey lanes |
| OBL-13 | Packet carry: A's tracked packet + evidence retained with source identities and carried/changed/pending disposition per inherited row; TASK-003-R2 superseded, R3 open; 100-tenant amendment recorded against REQ-033/AC-034 and R3 V6; no historical FAIL becomes PASS; diagnosis artifacts retained or marked unavailable | content review |
| OBL-14 | Benchmark handoff: immutable candidate full `bench:l1:{verification,gateway,bifrost}` records with identities and every listed diagnostic; misses recorded honestly (task outputs) | benchmark records (maintainer supplies candidate) |
