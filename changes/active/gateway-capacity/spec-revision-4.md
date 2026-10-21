---
id: SPEC-gateway-capacity
revision: 4
status: approved
base_revision: 3
---

# Proposed revision 4 — complete startup and gateway closeout

This is a minimal amendment to [approved revision 3](spec.md), not an approval
or a replacement of that file. Unchanged revision-3 requirements, matrices,
windows, evidence rules and acceptance criteria remain in force. Steven's
current request explicitly supplies the target ownership and startup direction;
this amendment records the conflicts with revision 3 before implementation.

## Necessary scope correction

REQ-006, INV-001/002, and the final revision-3 approval/authority paragraphs
exclude snapshot caching, migrations, admission/accounting changes, and Bifrost
contract changes. The current uncommitted tree already includes these changes.
They cannot be certified against revision 3 merely by adding a task.

Amend those exclusions only to admit the gateway optimizations already in this
worktree: tenant snapshot caching with committed-write invalidation and
uncached fallback on listener loss; the two gateway notification/admission SQL
migrations; avoiding unnecessary SQL for calls without applicable limits or
budgets; batched internal writes with per-row verified attribution; and
reservation of removed peer capture attribution fields. These changes require equivalent authorization,
tenant isolation, refusal, pricing, limit/budget charge and settlement behavior.
Covered buffered calls still settle before answering. No new admission policy,
reservation policy, accounting durability promise, or after-answer settlement
change is authorized. Repair any working-tree implementation that fails these
constraints rather than treating its current behavior as approved.

## Added requirements

**REQ-012 — Complete shared startup.** Production and `WyrdTestServer` use one
`AppState::start(&ServerSettings, StartInputs) -> Result<_, BootError>` path.
Inputs differ by settings and injected resources, never by a second assembly
workflow. Retain all existing role/profile validation, federation seeding,
secret rewrap, Operator-key checks, storage/resource composition and rollback.
Construct AppState once from complete settings, Postgres, storage, Bifrost,
auth, gateway, verification, MCP, health and shutdown owners. Remove partial
construction, setters, mutable field assembly, overrides and harness copies.
Disabled or absent role capabilities remain explicit engine states with the
existing unavailable/refusal behavior, not unfinished AppState fields.

**REQ-013 — Engine ownership and bounded lifecycle.** Auth owns authentication,
authorization and human logins. Gateway owns its configuration, execution,
snapshot/API-key caches and tasks. Verification owns hosted blocks/workspaces,
Verifier cache, Postgres submissions writer, runner, scheduler, health and
Operator keys. Bifrost alone owns the process Scribe outbox and its caches.
MCP owns its tasks; health owns readiness, peer status and the actual gRPC
reporter. Settings hold profile, role and HTTP limits. Every engine registers
its background work with the existing supervisor. Retain and flush the supplied
telemetry guard through lifecycle ownership; remove unread duplicate fields.

Shutdown stops admission and new background claims, then completes transports
→ gateway drain → MCP drain → verification runner/hosted-pool shutdown → Bifrost
(Scribe outbox flush first) → telemetry. Do not abandon producer descendants,
Scribe-acknowledgement callbacks or already-reserved submissions: the verification
submission writer remains available until final Scribe acknowledgements have
been committed, before Bifrost dependencies close. Preserve the existing overall
deadline, loss accounting, role fences and runtime/data-root lifetime ordering.
Failed or cancelled startup releases everything it acquired and advertises no
usable partial server. Caches are bounded and fail closed to uncached reads on
listener loss; no unbounded cache exemption is introduced.

**REQ-014 — One portable Monty worker implementation.** Move allocator type,
worker execution and subprocess dispatch into `verification::hosted::worker`
in the server library. The binary retains allocator installation and the early
`worker::dispatch()` exit, alongside its ordinary CLI behavior. No library
installs a global allocator. The startup input supplies the worker executable:
production uses `current_exe()`, server integration tests use Cargo's
`CARGO_BIN_EXE_wyrd-server`, and the testing harness uses the same-profile server
binary. No runtime worker-path environment variable or hardcoded release path.
Prewarm count is a setting (tests use zero); retain lazy first-use behavior and
eager startup failure behavior. Explicit pool shutdown uses the pinned
dependency's portable facilities after checkouts settle.

**REQ-015 — Verified rows and tenant isolation without a transport identity.**
Delete `SERVER_WRITE_PRINCIPAL`, its export, the Rows-requires-that-principal
check and its test, and its token-issue/verify refusals and tests. Introduce no
`Sender` type, replacement identity, or equivalent discriminator. The outbox
fills each frame's required principal with the existing
`PLATFORM_AUDIT_PRINCIPAL` no-verified-caller marker for every table; this
marker grants no authority and never replaces the verified row identities.
Retain audit decision identities as content and all unrelated token/SYSTEM
restrictions.

Keep the performance fix: `StampedAttribution::Rows` and frames grouped by
table and schema within a tenant slice. Public Gate ingestion always derives
`Frame` attribution from the verified token; supplied SUB/ACT columns are
refused. Only server-owned outbox writes, directly or via the existing mTLS
peer capture service, construct Rows frames from verified callers' pairs.
Retain per-row principal-id validation. Outbox lanes/batches remain tenant
scoped; Scribe checks authenticated tenant against frame principal tenant and
constructs the physical tenant binding. Different verified subject/actor pairs
may share a tenant's batch. These existing trust and physical boundaries,
not a special transport principal, provide the required properties.

Replace the removed-identity paragraphs in the design and security posture
with those two properties, and correct stale per-frame/peer attribution prose.

## Added acceptance obligations

| ID | Outcome and evidence |
|---|---|
| AC-010 | One complete startup path and engine ownership; static caller closure plus real-server success, disabled-role, bad-input and rollback journeys. |
| AC-011 | Owner-managed refresh and ordered, deadline-bounded shutdown, including Monty reaping, late Scribe acknowledgements and telemetry lifetime; fault/cleanup journeys and deliberate source-order assertion update. |
| AC-012 | Gateway/verification benchmark closeout on the completed startup path: case 1 at 4/16/64 clients, full overhead matrix, both capacity evidence modes from TASK-001, and verification with one Service/key per client; report parameters, p50/p99/mean, CPU per call, cores, throughput, capture/loss/memory and drain/shutdown evidence. |
| AC-013 | No cross-tenant data: two tenants call the gateway concurrently with distinct per-client keys; after drain/flush, real clients read captures and associated spans only from their tenant's physical location and every row's SUB/ACT equals that call's key's verified pair. Extend the existing gateway capacity journey and reuse peer coverage; add only missing assertions/traffic. |
| AC-014 | Forgery refused: a real client submits otherwise-valid public ingest requests carrying SUB, ACT, and both columns with valid forged principal IDs; the server refuses them and subsequent authorized reads show no forged rows. Extend the existing SDK attribution journey to send raw ingest past local validation. |

The requested improvement target is p99 overhead ≤500 µs for every applicable
gateway overhead point, including 64 clients. Retain revision 3's independent
strict <1 ms PASS gate and honest unmet-target reports; never relabel a slower
row or a lossy run as meeting the improvement target. A missed target requires
measured attribution and an explicit outstanding result, not relaxed gates.

Closeout also requires the drift lane's existing 51 tests plus any additions,
with TypeScript dist/native and Python prerequisites owned by the lane,
capacity-binary verification unit coverage, formatting/lints, dependency and
tenant checks, generated-contract checks and generated docs. Benchmarks stay
opt-in and outside CI performance gates. No new dependency or Cargo feature.

## Approval

Approved by the user on 2026-10-21. These exact deltas are revision 4; approval
does not approve unverified code or results.
Steven's subsequent explicit decision removes the special transport principal
and rules out a Sender replacement; REQ-015/AC-013/014 record that direction.
No branch creation, commits, git-config edits or implementation are part of this
planning request.
