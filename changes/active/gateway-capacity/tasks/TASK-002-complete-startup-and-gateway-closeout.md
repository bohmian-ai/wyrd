---
id: TASK-002
kind: implementation
status: in-progress
spec: SPEC-gateway-capacity
spec_revision: 5
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, INV-001, INV-002, INV-003, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018, AC-019]
depends_on: [TASK-001]
---

# Complete server startup and close out gateway benchmarks

## Outcome and Value

One fully constructed server graph starts identically in production and the
test harness, owns and shuts down all its resources, and produces credible
gateway/verification benchmark results without losing capture evidence.
This is exactly one outcome-complete task, including its consumer, test,
architecture and benchmark closure. Do not implement separate follow-up tasks
for startup, Monty, cache ownership or benchmark execution.

Authority: [revision 4 is approved](../spec.md) (user approval 2026-10-21).
The [revision-4 amendment](../spec-revision-4.md) records the deltas from
revision 3 that this task implements.

TASK-001 supplies the existing benchmark harness, measurements and workloads,
not a requirement to repeat completed work or wait for its old completion
label. Its remaining capacity/profile evidence is closed here. Work from the
current uncommitted tree on `change/gateway-capacity`; never create branches,
commit, stage changes to satisfy checks, or touch git config.

### Context for an implementer starting cold

This section carries the decisions and evidence from the planning
conversation. Re-verify line numbers; do not re-litigate the decisions.

**First step.** The working tree has never been compiled since the Monty
worker-path edits (env var removed, `T/build.rs` added, `#[ignore]`s removed
from `sandbox_worker.rs`/`pg_hosted_artifacts.rs`). Compile and run the
sandbox regressions before anything else.

**Field → owner map** (current `AppStateInner`, `S/state.rs:2217`):

| Current field | Target owner | Notes |
|---|---|---|
| `postgres`, `storage`, `shutdown_token` | AppState | Unchanged |
| `bifrost` | AppState (`Bifrost`) | Also sole owner of the Scribe outbox; starts its own cut-cache refresh (`S/app/server.rs:620`) |
| `scribe_outbox` | inside `Bifrost` | Same `Arc` as `bifrost.scribe_outbox()` today; ~70 read sites become the Bifrost accessor |
| `auth`, `authz` | one `Auth` | Human/platform logins are assembled into it, not written afterwards |
| `gateway`, `gateway_engine`, `gateway_api_keys`, `gateway_tasks`, `gateway_snapshots` | one `Gateway` | All private; snapshot listener (`S/app/server.rs:647`) started by Gateway. `gateway_snapshots` is a cache over a SQL query and must not be visible on AppState |
| `verification` (health), `verification_blocks`, `verifier_cards`, `operator_keys`, submissions outbox (now on Bifrost wrapper, `S/state.rs:1838`) | one `Verification` | Never `Option`; disabled/non-API roles are an explicit state with today's unavailable response. Verifier-card listener (`S/app/server.rs:634`) started by Verification |
| `limits` (`LimitsConfig`, HTTP body limits), `deployment_profile` | `ServerSettings` | Values, not handles |
| `mcp_tasks` | `Mcp` | |
| `readiness`, `peer_plane`, `grpc_health` | `Health` | `grpc_health` is never read today; keep only the reporter actually paired with the served health service |
| `telemetry` | lifecycle owner | Never read today. The harness never passes its guard in (`T/src/server.rs:4697` defaults it, nothing installs it) — fix |
| `mcp_context_probe`, `query_stream_fault` | one `cfg(feature = "test-support")` hooks value | |

Rule from Steven: caches belong to the engine that uses them (Bifrost,
gateway, verification, auth already does this for JWKS); AppState never holds a
cache, and `BoundServer::run` never spawns an engine's refresh loop.

**Why the worker path differs in tests (do not try `current_exe()` there).**
The Monty pool runs each Python job in a child process started as
`<executable> subprocess`; only the `wyrd-server` program's `main` handles
that argument and installs the counting allocator. `WyrdTestServer` runs the
server library inside the test's own process, so `current_exe()` is the
nextest binary (`target/debug/deps/integration-*`, which would read
`subprocess` as a test filter), `/usr/bin/python3.12` (prints "can't open file
'subprocess'"), or `node` (module-not-found). Verified on this host. Therefore:
production passes `current_exe()`; wyrd-server's own tests pass
`env!("CARGO_BIN_EXE_wyrd-server")` (Cargo builds it in the same profile); the
harness passes `<WYRD_TESTING_PROFILE_DIR>/wyrd-server` built by
`mise run build:server` (dev profile). No env var, no `target/release`. Users
are unaffected: `wyrd server dev` launches the installed `wyrd-server` binary
(`crates/wyrd/wyrd-cli/src/server/dev.rs:707`), so its pool spawns itself.

**Evidence behind per-row attribution (what the benchmark must confirm).**
Pre-fix case 1, 16 clients, one key per client, profiling build, Ryzen 9
9950X (32 CPUs): 32,839 calls/s, overhead p50 ≤0.25 ms / p99 ≤0.5 ms / mean
0.210 ms, 0.376 ms CPU per call, 12.34 cores, **78,653 capture rows lost**.
Traces showed the shared outbox full (`SCRIBE_CAPACITY` 65,536): one write in
flight per tenant lane took ~136 ms because it carried ~33 frames (frames were
keyed by table + attribution + schema, so ≈ 2 × clients + 1), with
`FRAMES_IN_FLIGHT` 8 and ~18 ms per Scribe ACK. 4 clients lost 0 (14,676/s,
p99 ≤0.5 ms, 0.285 ms CPU/call, 4.19 cores). The fix groups by table + schema
with each row carrying its verified SUB/ACT, so a write is ~3 frames
regardless of client count. It is unit/journey-tested but unbenchmarked; run
16 clients first and compare loss, frames per write, write latency and
throughput against these numbers.

**Steven's standing rules for this work.**

- Every results report states the experiment parameters plus p50, p99, mean,
  CPU per call, cores and throughput (and loss). Target p99 overhead 500 µs
  across the board.
- Stay on `change/gateway-capacity`. If asked to commit: identity
  Thorrester <sjforrester32@gmail.com>, no attribution of any kind (no
  Co-Authored-By, session links or "Generated with" lines), no links.
- Do not add `depends` to lanes to paper over a smell; a lane depends on
  `build:server` only because the harness genuinely needs the worker binary.
- One way to do each thing; delete what does not earn its place rather than
  adding machinery. Do not defer work into follow-up tasks.
- Diagnose with WYRD_LOG/RUST_LOG traces and profiles before fixing. No
  synthetic host load, no OS-specific mechanisms, no `#[allow]`.
- `gateway_onboards_compatible_provider_at_runtime` (`S/components/gateway/pg_invocation_tests.rs:1588`) failed once
  earlier and then passed repeatedly; if it fails again, diagnose it from its
  trace rather than retrying.

### Obligation closure

| Obligations | Closure in this task |
|---|---|
| REQ-001/002/008, AC-001/006 | Preserve real release-server journey, matrix/filters and opt-in profile/artifact standards; rerun after shared startup. |
| REQ-003/004/011, AC-002/009 | Preserve arrival/handoff timing, production phase metrics, raw client roundtrips and matched attribution; report regression plus release evidence. |
| REQ-005, AC-003 | Zero loss/drop, correct caller attribution, bounded memory, drain and shutdown; local/peer journeys and every release row. |
| REQ-006, AC-004 | Rank residual phases and preserve FAILs; distinguish ≤500 µs target from strict <1 ms PASS. |
| REQ-007, AC-005 | Reuse shared capacity/window/profiler/resource owners; preserve verification and query consumers. |
| REQ-009/010, AC-007/008 | Complete TASK-001's outstanding paced/open-loop capacity and overload/recovery evidence under revision-3 windows. |
| INV-001/002/003 | Preserve security, accounting/refusal and loss behavior except the enumerated revision-4 amendment; no new dependency/feature or OS-specific mechanism. |
| Proposed REQ-012/013/014, AC-010/011/012 | Shared complete startup, engine lifecycle/cache ownership, portable library Monty worker, consumer and benchmark closure below. |
| Proposed REQ-015, AC-013/014 | Delete the redundant transport identity with no Sender replacement; prove verified row attribution and tenant separation through concurrent gateway and public-ingest forgery journeys. |

## Owners, Scope, Consumers, and Prohibited Changes

The server owns composition and durable behavior; the testing crate supplies
inputs and drives the same composition. The requested entry contract is
`AppState::start(&ServerSettings, StartInputs) -> Result<_, BootError>`.
Current settings/error owners are `WyrdServerConfig` and `ServerBootError`;
close all consumers if names change, without compatibility aliases. Do not
introduce a second facade that still delegates to divergent assembly paths.

AppState contains only complete owners: settings (profile, role, HTTP body
limits), Postgres, storage, Bifrost, auth, gateway, verification, MCP, health,
and the shutdown token. No optional engine slots, caches, state setters,
`DerefMut`, or post-construction field writes. Explicit disabled/role-specific
states inside an engine preserve existing unavailable behavior; this does not
require starting Verifiers on disabled or non-API roles.

- Bifrost solely owns the Scribe outbox, cut/registration/metadata caches and
  their refresh lifecycle. Every current staging consumer reaches that same
  outbox through its owner; no fallback ownerless outbox.
- Auth combines authn, authz and human logins. Preserve RBAC, audit injection,
  signing/verification, issuer/binding resolution and profile screening.
- Gateway privately owns config, execution engine, snapshots, API-key cache
  and tasks, exposed through operations rather than cached-field access.
- Verification owns Monty/workspaces, the Verifier cache, Postgres submission
  writer, runner, scheduler, health and Operator keys. Scribe acknowledgement
  still precedes submission visibility; moving the writer must not change it.
- MCP owns its tracker. Health owns readiness, peer state and the reporter
  actually paired to the served gRPC health service. Lifecycle ownership
  retains the injected telemetry guard and flushes it last; delete unread
  duplicate telemetry/reporter fields rather than adding dummy reads.
- Each engine registers its tasks with the existing supervision owner.
  `BoundServer::run` drives transports and supervision, not cache discovery,
  cache refresh spawning or late construction of the verification runtime.

Remove `AppState::new`, its 14 `with_*` setters, direct mutable assembly,
`StateOverrides`/`apply_overrides`, startup `attach_*` helpers and harness
copies, `test_verification_blocks`, and the disconnected gateway fallback.
Replace callers and obsolete construction assertions with equivalent behavior
proof; do not delete security/failure coverage along with the old APIs.
Consolidate `query_stream_fault` and `mcp_context_probe` in one existing
`cfg(feature = "test-support")` input boundary; production feature unification
must not enable those behaviors implicitly.

### Steven's attribution decision

Keep `StampedAttribution::Rows` and table+schema grouping within each tenant's
slice. Delete `SERVER_WRITE_PRINCIPAL`; introduce no `Sender` type, replacement
identity, transport-authority enum or equivalent machinery. The existing
`PLATFORM_AUDIT_PRINCIPAL` no-verified-caller marker fills the frame's required
principal for every outbox destination. It grants nothing and does not replace
the verified SUB/ACT values on work rows; audit content stays unchanged.

Required deletion/consumer closure:

- `crates/wyrd-spec/src/auth/principal_id.rs:31` constant and
  `auth/mod.rs:52` export.
- `vala-bifrost-redux/src/scribe/ingress.rs:78–81` identity check and
  `:1160` `row_attribution_admits_only_the_server_write_principal` test.
  Preserve tenant checks, system-owner audit restrictions and row validation.
- `crates/shared/wyrd-auth-issue/src/lib.rs:192,213,357,401` identity-specific
  refusals and their tests; `wyrd-auth-verify/src/lib.rs:831,872,978` verifier
  check/call and delegation refusal plus their tests. Retain unrelated token,
  SYSTEM-kind and delegation validation. Delete obsolete test selectors rather
  than pretending a removed check is a security acceptance criterion.
- `S/scribe_outbox.rs:464–479` use the existing marker for every table, keeping
  audit Frame attribution versus non-audit Rows attribution. Update writer
  assertions that currently expect the deleted identity and all imports/docs.
- Remove its mentions from `architecture/wyrd-design.md:187` and
  `architecture/wyrd-security-posture.md:73`; replace them with the two existing
  boundaries below. Drop the former special-principal prose-update closeout.

Verified source basis:

1. **Validated attribution:** Gate's canonical/native paths at
   `crates/vala/vala-bifrost-redux/src/gate/mod.rs:959,1036` derive Frame from
   `auth.attribution()` (the verified-token context). Scribe at
   `scribe/execution_lanes.rs:564–572` rejects supplied SUB/ACT for Frame;
   `:1239–1264` validates principal IDs in server-carried Rows. Internal
   `S/scribe_outbox.rs:464,1035` constructs frames/row pairs;
   `components/gateway/capture.rs:288,466–473` retains the verified pair.
   `S/grpc/capture_peer.rs:102` uses the same frame constructor on the existing
   mTLS-only capture path. Public callers cannot select Rows.
2. **Tenant and physical separation:**
   `crates/shared/wyrd-runtime/src/outbox.rs:90–129,507–518` keeps per-tenant
   lane backlogs; `S/scribe_outbox.rs:1292–1315` writes one tenant slice and passes that tenant into its batches. Scribe's
   `scribe/ingress.rs:72–76` compares authenticated and principal tenant;
   `:403–418` derives tenant table facts, physical binding and shard lane.
   Keep these checks/bindings. Mixing verified pairs within one tenant's frame
   does not mix tenants. The removed identity adds neither protection nor
   throughput; per-row attribution supplies the batching improvement.

### Inspected reuse map (current working-tree line numbers)

Paths below are relative to the repository; line numbers are navigation aids.
`S` = `crates/wyrd/wyrd-server/src`, `T` = `crates/wyrd/wyrd-testing`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Complete startup | `S/state.rs:2312` AppState::new; `S/boot/mod.rs:1171` build_state | `T/src/server.rs:4760–5120` start_with_resources; `S/boot/mod.rs:2483–2559` composition/override assertions | Half-empty state; separate auth/logins/resource/gateway assembly; harness telemetry not installed | One start operation over settings/injected resources, constructing each engine before AppState | StartInputs replaces overrides and duplicated workflow; no second boot framework |
| Gateway ownership | `S/state.rs:2217–2296` AppStateInner; `components/gateway/service.rs:632` snapshot; `snapshots.rs:48` GatewaySnapshots | `app/server.rs:647` listener; `T/tests/gateway/{native,peer,resilience}.rs` | Five gateway fields plus snapshot cache leak into application state; cache is currently unbounded | Existing gateway owner retains cache/config/tasks; bound cache and preserve notification generation/fallback behavior | Cohesive owner retains real state; no replacement cache protocol |
| Verification ownership | `S/verification/submissions.rs:73–97` SubmissionSink::outbox/verifier_cards; `S/state.rs:1582,1838` Bifrost submissions | `S/verification/mod.rs:417` Operator keys; `components/operators/service.rs:81`; hosted blocks/runner | Bifrost owns Postgres submissions; copied-out cache and separate health/blocks/keys | Verification composes existing blocks/cache/writer/runtime once | Existing verification owner absorbs misplaced capabilities; no second runner or writer |
| Supervision and lifecycle | `S/app/supervise.rs` TaskExit/restarting_worker/drain_with_shutdown_hooks; `S/state.rs:2050` Bifrost::shutdown | `S/app/server.rs:620–760,804+`; `S/app/mod.rs:223` source-order assertion; `T/src/server.rs:863` shutdown | Transport runner starts engine caches; no explicit hosted-pool close; ordering crosses writers | Extend existing supervision with engine task registration and ordered drain | Reuse supervisor, task trackers, role shutdown reports and deadline; no parallel lifecycle system |
| Auth/health/telemetry | `S/boot/mod.rs:1453,1499` install_auth/attach_human_logins; `state.rs:2337–2357` defaults | `T/src/server.rs:5048–5109` copied login/authz setup; `S/app/server.rs:553+` health publication | Split authn/authz, fake default telemetry and duplicate unread reporter | Complete auth and health owners; retain actual supplied telemetry in lifecycle | No new auth transport/cache or global recorder |
| Monty worker | `S/monty_worker.rs:36,64,183` dispatch argument/allocator/run; `main.rs:24,98`; `verification/hosted/blocks.rs:52,88`; `sandbox.rs:54,76` | `tests/integration/sandbox_worker.rs:19`; `pg_hosted_artifacts.rs:91`; `T/build.rs:14`; `T/src/server.rs:6122` | Worker exists only in binary; harness pool constructor duplicates boot; pool never explicitly closes | Library `verification::hosted::worker` plus injected executable and prewarm setting; use pinned monty-pool 1.1.0 close/reap facilities | Move existing implementation; allocator installation stays binary-only; no process abstraction/dependency |
| Shared attribution | `vala-bifrost-redux/src/gate/mod.rs:959,1036`; `scribe/execution_lanes.rs:564,1239`; `scribe/ingress.rs:72,413`; `S/scribe_outbox.rs:464,827,1035,1292` | `S/grpc/capture_peer.rs:102`; `T/tests/gateway/resilience.rs:620–803`, `peer.rs:196`; `sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:490–638` | Special identity/check is redundant; capacity journey reads capture counts but lacks concurrent per-client pair/span/location proof; SDK spoof insert fails locally | Delete identity/check/token refusals, use existing no-caller marker; extend existing capacity and SDK attribution journeys with missing public-path assertions | Existing verified Frame path, per-row validation, tenant binding, RawIngest and peer RPC suffice; no Sender or new trust layer |
| Benchmark closeout | `T/src/bin/capacity/gateway/{mod,report,run}.rs`; shared capacity/release_server/profile | `verification/workload.rs:70,93` one Service/key per client; `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs:1455` cross-runtime grouped journey | Remaining release/profile evidence and startup regressions; drift lane lacks TS prerequisites | Existing workloads/reports and lane prerequisites; preserve query consumer | No new harness, parser, resource sampler, profiler, or benchmark-only production path |

Inspected structural references: OpsML `core/state.rs:14` and
`core/app.rs:32` construct state once after service setup; Scouter
`api/state.rs:23,41` and `api/setup.rs:132` retain services and their lifecycle.
Their roots are `~/Documents/GitHub/opsml/crates/opsml_server/src` and
`~/Documents/GitHub/scouter/crates/scouter_server/src`. Borrow the composition
pattern only; their names, raw SQL pools and platform behavior are not Wyrd
precedents.

### Working-tree baseline and exclusions

Planning inspected `git status --short --branch`, `git diff --stat` and the
diff before source exploration. The branch is correct and has substantial
uncommitted changes, including new snapshot/admission SQL files and testing
build script. Preserve them and assess them as the implementation baseline.

Already present: table/schema-only Scribe grouping with per-row attribution;
the redundant SERVER_WRITE_PRINCIPAL check (scheduled for deletion); reserved peer proto fields;
gateway hot-path work; one Service/key per verification client; worker-path
environment/config removal; Cargo server-bin paths in server tests with ignores
removed; same-profile harness build output; dev `build:server`; drift dependency
on it; runtime-limit doc row removal. These are source facts, not compile/test
results. Do not redo them or restore the environment variable.

Exclude the provider-wide `pool_max_idle_per_host = 8` change
(`crates/skald/skald-providers/src/transport.rs:37,92`); it affects all provider
clients and needs separate evidence. Exclude an additional limits-configured
benchmark case; retain the existing real limit/refusal/recovery journey below.
Exclude the CLI `Command::new("kill")` cleanup at
`crates/wyrd/wyrd-cli/src/server/dev.rs:1130`: record it as an existing portability
violation, not an acceptable pattern or a new prerequisite. No new OS-specific
production mechanism, `cfg(target_os)`, `#[allow]`, synthetic load, public
operation, SDK inference API, dependency or Cargo feature. Case 3 memory/loss
and p99 at 64 clients are required by the full overhead matrix, not optional.

## Approach

1. Recheck the dirty baseline and consumer graph. Preserve working changes,
   adopt the approved amendment when available, and characterize current
   success, disabled-role, failure and cancellation behavior before moving it.
2. Compose complete auth, gateway, verification, MCP, health and Bifrost owners
   through shared startup inputs; production and harness call that path with
   their respective resources, signing material, profile, telemetry, worker
   executable and test controls. Preserve every existing boot step and role.
3. Move caches, submissions and task registration to their owners; update all
   handlers, middleware, MCP, peer services, runner, Operator delivery and test
   consumers. Delete partial construction and copied setup only after closure.
4. Move the worker into the library and complete explicit, bounded shutdown
   and failed-start rollback. Preserve Scribe/Forge runtime and data-root
   lifetimes and update source-order proof deliberately.
5. Delete the redundant identity/check/refusals without a Sender replacement;
   verify per-row attribution, local/peer writes and Service/key isolation;
   repair executable gaps through the scenarios below. Correct architecture,
   runtime prose, generated docs and lane-owned build prerequisites.
6. Run focused and owner checks, then the required benchmark sequence; preserve
   artifacts between runs and append a compact obligation/result ledger here.

## Ordered Implementation Scenarios

This is a mixed task. New startup/lifecycle behavior requires TDD; already
implemented optimizations, moves and benchmark evidence use regression/static
proof when correct. A currently passing characterization is not a fabricated
RED. The three proposed startup test names below use the inspected `wyrd-testing`
`server` target's existing `verification_runtime` module; confirm nonzero
selection after adding them. Fixture structure remains implementation-owned.

### Scenario 1 — Production and harness start the same complete graph

**Behavior.** Equivalent settings/inputs produce equivalent authenticated,
audited gateway and direct/queued verification behavior from both entry paths.
Assert injected HTTP limits, profile, resources and real telemetry are honored.
Disabled/non-API capabilities retain their current unavailable response without
spawning workers. Eager bad-worker/key/config input fails before readiness;
zero-prewarm first use remains lazy and reports failure through normal errors.
Failure/cancellation after role acquisition releases acquired tasks, workers,
fences and data-root ownership. Maps proposed AC-010/011 and INV-001/003.

**RED.** Add `verification_runtime::shared_startup_uses_complete_injected_engines`
and run command J1 below. Demonstrate missing shared startup or a behavioral
parity/cleanup assertion failure, separately from incidental compile/setup
failures. Extend the shared-startup test for disabled and rollback branches.

**GREEN.** Make production and `WyrdTestServer` use the same complete start
operation and value injection. Keep federation seeding, rewrap, production
validation, Operator-key checks and storage/resource policy; tests do not skip
these steps to obtain parity. Wire the supplied telemetry guard. Rerun J1 and
the sandbox/hosted and grouped cross-runtime regressions below.

**REFACTOR.** Delete partial constructors, overrides, attach helpers, harness
copies and direct writes, replacing affected tests with the same assertions
through the complete boundary. Keep ordinary private choices local.

### Scenario 2 — Engine caches refresh without transport-owned discovery

**Behavior.** Gateway and Verifier administrative changes invalidate the
correct tenant's cache; listener loss clears cached authority and makes reads
uncached until listening resumes. Bifrost cut refresh remains available on
its owning roles. No stale fill survives a generation change, no tenant sees
another's snapshot, and retained caches stay bounded. Authentication/logins,
readiness and role capabilities are usable before transport admission. Maps
REQ-005, INV-001/003 and proposed REQ-013/AC-010/011.

**RED.** Add `verification_runtime::engine_owned_refresh_preserves_tenant_changes`
and run J2. Exercise shared startup without relying on `BoundServer::run` to
discover cache fields; pin invalidation, reconnect and cross-tenant assertions.
The expected missing behavior is owner-started refresh or bounded retention.

**GREEN.** Each engine registers and owns its existing refresh tasks through
the one supervisor, with the same fallback/invalidation semantics; bound the
gateway cache using existing facilities. Rerun J1/J2 and local/peer gateway
regressions. No polling substitute or second cache/registry is needed.

**REFACTOR.** Remove AppState cache exposure and transport-runner inspection.
Keep J1/J2 green while moving staging, auth, gateway and verification consumers
to the owning operations. Do not introduce utility structs without state.

### Scenario 3 — Shutdown settles accepted work and explicitly reaps Monty

**Behavior.** Stop transports and new background claims, then drain gateway,
MCP, verification runner/hosted pool, Bifrost (outbox first), and telemetry.
In-flight verification/MCP descendants cannot produce gateway work after its
drain is declared complete. Final Scribe ACKs can still stage reserved Postgres
submissions, which commit before the submission writer/dependencies close.
Local and peer capture/results retain exact SUB/ACT, no double-write on retry,
and no silent loss. Active/idle Monty workers settle or are reaped by the shared
deadline; repeated shutdown is safe. Deadline expiry reports incomplete drain
and counted loss, never clean success. Maps AC-003 and proposed AC-011/012.

**RED.** Add `verification_runtime::shared_shutdown_drains_producers_workers_and_acknowledgements`
and run J3. Include accepted gateway/MCP/verification work, an ACK arriving
during shutdown, a busy Monty checkout and a short-deadline negative case.
Expected failure is missing explicit close/order, premature dependency release
or false clean completion. Update the existing source-order test only after
the runtime proof expresses the retained ordering.

**GREEN.** Extend existing supervision and owner drains; quiesce producers
before closing downstream task admission. Runner/pool shutdown happens in the
verification phase; the already-reserved submission handoff stays alive through
Bifrost's initial outbox flush and is then drained before its dependencies
close. Use monty-pool 1.1.0's existing `Pool::close` after checkouts settle;
it closes idle workers, so calling it alone with live checkouts is insufficient.
Retain runtime/data-root owners until their supervised work is done. Rerun
J1–J3, source-order, sandbox, attribution/replay and local/peer regressions.

**REFACTOR.** Remove binary-local worker implementation and harness pool setup.
Keep only allocator installation and early `worker::dispatch()` exit for the
binary's worker-specific responsibility; ordinary CLI behavior stays intact.
Keep one supervisor and explicit ordered lifecycle, with no destructor-only
claim of a successful flush or worker reap.

### Scenario 4 — Real clients retain their own rows and cannot forge attribution

**Behavior.** Proposed REQ-015/AC-013/014 and existing INV-001 remain true after
deleting the special principal. Two tenants call the gateway concurrently with
distinct per-client keys and multiple keys in each tenant. After response
completion and capture/Scribe drain, each tenant's real client reads exactly
its expected call rows and associated spans, every pair matching the verified
key that made that call. Assert disjoint call/trace identities, absence of the
other tenant's records, and tenant-specific physical binding/location evidence
through existing harness inspection alongside public reads; counts alone do
not prove location or attribution. Public ingest carrying valid but forged SUB,
ACT, and both columns is refused, and subsequent client queries contain none
of those attempted rows. Valid control rows still land with verified identity.

**RED.** Extend the existing
`resilience::gateway_capacity_paces_streams_isolates_tenants_and_recovers`
journey (introduced in `b1f83f26b`, current source `T/tests/gateway/resilience.rs:620`)
with only the missing concurrent per-client-key traffic and per-row
capture/span/location assertions. Its current `:790–803` assertions cover
tenant capture counts, not the requested complete outcome. Extend
`query_bifrost::subject_actor_rows_and_reserved_columns`
(`sdks/wyrd-sdk-rust/tests/integration/query_bifrost.rs:490`) with requests that
actually reach public ingest. Its spoofed insert at `:524–530` is refused
locally; the raw server refusal at `:598–622` is table registration, not ingest.
Use existing `wyrd_testing::bifrost::write::RawIngest` (`T/src/bifrost/write.rs:181–249`)
for otherwise-valid forged Arrow frames, not a new client/transport. Run the
two exact journey commands below. Record any real missing-behavior failure;
if the stronger assertions already pass, record regression-only proof rather
than manufacture a RED. Removing the redundant check is static simplification,
not a requirement for previously secure behavior to fail first.

**GREEN.** Remove the constant/export/check/token refusals and their
identity-specific tests, fill the required outbox principal with the existing
no-caller marker, and update affected writer assertions/docs. Preserve the
Frame-vs-Rows distinction, per-row validation, tenant slice/batch separation,
physical binding and mTLS peer boundary. The forged IDs must be valid principal
IDs so rejection proves managed-column protection, not malformed input. Keep
both extended journeys and existing grouping/replay/peer regressions green;
if a journey exposes an actual gap, fix it in its existing owner.

**REFACTOR.** Reuse the existing gateway journey, SDK attribution story and raw
ingest transport. Keep their pacing, limit/recovery, direct/delegated attribution
and reserved-field assertions. Add no Sender, replacement transport identity,
duplicate journey framework, public API or production test bypass.

## Acceptance Criteria

1. **Complete state:** the requested engine-only AppState and shared startup
   path exist; no partial constructor/setter/override, mutable state assembly,
   disconnected gateway, duplicate harness workflow or unread field remains.
   All production steps and injected test values survive. Owner: server/testing;
   proof: J1, static call-site closure and affected-owner checks.
2. **Correct ownership:** all caches, tasks and Postgres submissions reside with
   their specified engines; Bifrost owns the only Scribe outbox; auth combines
   authn/authz/logins; HTTP limits are settings; health uses its actual reporter;
   telemetry stays alive through final flush. Proof: J1/J2 and source inspection.
3. **Portable worker/lifecycle:** library worker, binary-only allocator install,
   explicit injected paths, settings prewarm (zero in harness), no old env var,
   explicit Monty close/reap, failure rollback and required shutdown ordering
   work under the shared deadline, including ACK/submission ordering. Owner:
   verification/supervision; proof: J1/J3, sandbox and source-order regressions.
4. **Attribution and tenant outcomes:** two tenants call concurrently under
   distinct per-client keys, with multiple callers sharing each tenant's slice;
   real client reads after drain/flush return captures and associated spans only
   in their own tenant's physical location. Every row's SUB/ACT equals its
   calling key's verified pair, not the frame's no-caller marker. Opposite-tenant
   call/trace identities are absent, with no omitted or duplicate expected rows.
   An otherwise-valid public ingest request with forged SUB, ACT, or both is
   refused by the server; authorized reads prove nothing forged landed. The
   special constant/check/token refusals/tests are gone, no Sender replacement
   exists, and local/peer attribution, audit content and idempotent replay stay
   correct. Owners: gateway/Scribe/testing; primary proof: the extended capacity
   and SDK attribution client→server→client journeys in Scenario 4; unit/peer
   regressions support those journeys rather than replace them.
5. **Gateway evidence:** case 1 at 4/16/64 clients precedes the full matrix.
   Preserve p50, p99, mean, CPU/call, cores, throughput and all parameters,
   client/server timing, phases, losses, capture counts, memory, traffic/drain
   windows, build/host/feature identity and profile completeness. Target
   ≤500 µs at every applicable point, with strict <1 ms PASS kept independent.
   Case 3 memory/loss and 64-client p99 are mandatory. Missing target is an
   explicitly outstanding measured result with ranked causes, never invented
   success. Owner/proof: existing gateway reports and release commands.
6. **Benchmark closure:** verification uses a separate Service/key for every
   closed-loop client and verifies direct/queued behavior; both gateway capacity
   modes close TASK-001's remaining evidence using revision-3 windows and SLO
   inputs. Preserve raw samples, incomplete-profile reasons and failed rows;
   no benchmark or build shortcut changes production behavior. Owner/proof:
   capacity tests, release commands and compact appended evidence ledger.
7. **Journey/docs/check closure:** drift lane passes its current 51 tests plus
   any additions with nonzero selection and genuine TS/Python prerequisites;
   cross-runtime grouped support passes. Replace removed-identity prose with
   the verified-attribution and tenant/physical boundaries, correct stale per-frame Bifrost attribution prose,
   and runtime setup wording. Generate docs/contracts from source. Required
   focused, owner, lint, boundary, codegen and docs checks pass, or report the
   exact environment/dirty-tree limitation without claiming a green gate.

## Expected Write Set and Consumer Closure

Paths guide ownership, not private implementation structure or a file allowlist.

- Server: `src/{state,config,main,lib}.rs`, `src/boot/`, `src/app/`,
  `src/components/{auth,gateway,health,operators}/`, `src/verification/`
  (including hosted worker, cache, submissions, runner/scheduler/health),
  `src/scribe_outbox.rs`, and affected HTTP/router, gRPC peer and MCP consumers.
  Move `src/monty_worker.rs`; do not copy it. Update every AppState constructor,
  setter, direct-field and staging consumer, including test-support helpers.
- Testing: `crates/wyrd/wyrd-testing/{build.rs,src/server.rs}` and its capacity,
  gateway and Bifrost server tests; existing server integration/unit tests;
  shared SDK Rust journeys and Python/TypeScript harness consumers affected by
  changed startup inputs. Preserve interpreter-owned tests in their runtimes.
- Existing attribution/optimization closure: `vala-bifrost-redux` contracts,
  Scribe ingress/execution/projections/tests; delete the `wyrd-spec` constant/export,
  retain gateway-record changes, remove identity-specific auth issue/verify tests; `wyrd-tonic` proto; `wyrd-sql` gateway
  queries and the two new migrations; tenant-isolation sanctioned owner entry.
  Inspect existing changes; do not expand SQL exemptions to hide violations.
- `mise.toml`: shared worker-bin build and drift prerequisite ownership.
  `architecture/{agent-rules.md,wyrd-design.md,bifrost-design.md,wyrd-security-posture.md}`
  for lasting owner/attribution statements, including the old AppState outbox
  reference. `docs/src/content/docs/operate/configure-verification-runtime.md:14`
  must say the worker always uses the shipped binary, not “the default needs
  no setting”; update its stale install-worker description too. Keep the
  removed runtime-limit row removed. Regenerate `docs/public/llms*.txt` through
  `docs:generate`, separately from contract codegen.
- Append results/evidence links here and reconcile TASK-001's outstanding
  evidence status when proven; do not rewrite its historical measurements.

## Verification and Evidence

All commands run from the repository root through mise. These are instructions
for implementation; planning has not run them. Rerun any failing test with
`WYRD_LOG=info,wyrd_server=debug,vala_bifrost_redux=debug` and read its trace
before diagnosis. Fix absent tracing in the harness rather than debugging blind.
For every exact selector, confirm it selects the intended test, including
planned tests after addition. Never accept a zero-test run.

Apply AGENTS.md's struct-centered Rust style and rustdoc to every new or
materially changed item, including private items and tests: `# Errors` for
fallible functions, `# Panics` when applicable, and cancellation/partial-progress
behavior for async lifecycle operations. Keep pure work synchronous. Do not add
`#[allow]`, disable a test or weaken a boundary/gate to clear verification.

### Focused scenario and regression commands

J1, J2 and J3 respectively (new names in the inspected existing target):
build the same-profile worker first with `mise run build:server`. Ensure every
environment-owning harness lane has its own correct worker-build prerequisite,
including custom target directories/profiles; a developer's existing binary is
not a dependency declaration.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::shared_startup_uses_complete_injected_engines)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::engine_owned_refresh_preserves_tenant_changes)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=verification_runtime::shared_shutdown_drains_producers_workers_and_acknowledgements)'
```

Existing source-order, sandbox, hosted and shared-writer regressions:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::tests::forge_runtime_lives_until_worker_supervision_drains)'
mise exec -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::sandbox_runs_entrypoints_without_shared_state)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::artifact_evidence_and_code_reach_every_consumer)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::writes_group_by_destination_whatever_their_attribution)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=scribe_outbox::tests::a_retry_resubmits_identical_frames)'
```

Local/peer, post-response capture, limits/recovery and mixed-language regressions:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=peer::oracle_only_gateway_captures_through_the_peer_scribe)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::capacity_capture_accounting_waits_for_post_response_work)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::a_retryable_refusal_fails_over_and_a_limit_refuses_before_dispatch)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E 'test(=resilience::gateway_capacity_paces_streams_isolates_tenants_and_recovers)'
mise run build:server
mise run py:setup
mise run ts:build
mise run ts:build:testing
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verify_in_real_time::grouped_support_resumes)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_bifrost::subject_actor_rows_and_reserved_columns)'
```

Scenario 4's primary proof is the exact capacity journey command above plus
the exact SDK attribution journey command; no new test target is needed.
Identity-check tests scheduled for deletion are removal-only obligations, not
tests to run or replace with an assertion about another transport identity.

The drift lane itself must own its prerequisites; running `ts:build` manually
once is diagnostic proof, not the fix for a clean checkout. Its grouped journey
launches public Python and Node SDK processes; preserve that real-runtime proof.
The exact old source-order selector remains the iteration command while its
assertions are deliberately updated for the new composition shape. If symbols
move, record the new source-validated selector rather than dropping coverage.

Affected owner/check commands, plus the explicitly requested drift lane:

```sh
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^verification::/)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(/^gateway::/)'
WYRD_TEST_PACKAGES='wyrd-server wyrd-testing wyrd-gateway wyrd-sql' mise run test:wyrd
WYRD_TEST_PACKAGES='vala-bifrost-redux' mise run test:vala
WYRD_TEST_PACKAGES='wyrd-auth-issue wyrd-auth-verify' mise run test:shared
mise run test:verification:sandbox
mise run test:bifrost:journey:drift
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:regen
mise run codegen:check
mise run docs:generate
mise run docs:check
git diff --check
```

Add Python format/lint/type/unit checks only if their source/typing changes;
add affected runtime-specific focused tests when harness bindings change. This
task adds no client API, so do not manufacture three duplicate startup fixtures.
Full journey families and the broad gate belong to integrated change review;
the explicitly requested capability-specific drift lane runs here.

`docs:check` currently compares generated docs with Git HEAD and the baseline
already has a modified `docs/public/llms-full.txt`. Under the no-commit
constraint it may fail despite deterministic regeneration. Run it unchanged,
retain the exact diff/failure and prove a second generation adds no changes;
do not stage, commit, reset generated docs, weaken the checker, or report that
alternative as a passed gate. Distinguish this from actual generated drift.

Planning-only proof: the packet adds exactly one TASK-002, all four scenarios
have Behavior/RED/GREEN/REFACTOR blocks, all revision-3 obligation IDs are mapped,
local authority links resolve, the approved spec remains revision 3, and the
amendment is revision 4 draft. `git diff --check` passes. No code, tests,
benchmarks, branch, commit, staging or git configuration changed in planning.

### Required release sequence

Preserve each command's output directory before the next command replaces it,
and retain logs on nonzero exit. Run on the same host/build and state profiling
mode. Do not generate synthetic host load or reuse stale binaries.

```sh
mise run bench:gateway:overhead -- --case 1 --clients 4 --no-profile
mise run bench:gateway:overhead -- --case 1 --clients 16 --no-profile
mise run bench:gateway:overhead -- --case 1 --clients 64 --no-profile
mise run bench:gateway:overhead -- --no-profile
mise run bench:gateway:overhead
mise run bench:verification:capacity
mise run bench:gateway:capacity -- --max-answer-ms 200 --max-first-token-ms 350 --max-chunk-lag-ms 50 --max-stream-ms 1100 --no-profile
mise run bench:gateway:capacity -- --max-answer-ms 200 --max-first-token-ms 350 --max-chunk-lag-ms 50 --max-stream-ms 1100
```

The capacity SLO numbers are declared deterministic-workload inputs inherited
from TASK-001, not production promises. Use revision-3 2 s warmup/15 s windows,
3 s backlog edges, 10 s overload/20 s recovery and the specified sample minima.
Do not skip a selected rate silently or present untested ceilings as capacity.

Every report records parameters plus server/client p50, p99 and mean, server
CPU per completed call (CPU seconds divided by the matching completed-call
population, with units), mean cores, throughput, and separate drain CPU/time.
Record missing values as unavailable, never zero. Report raw populations,
profile files/completeness, retained rows, final loss counters and clean shutdown.
Use existing reporting/measurement owners to fill any output gap.

Prior measurements supplied by Steven are comparison context only: 4 clients
14,676/s with p99 ≤0.5 ms; 16 clients 32,839/s but 78,653 lost events before
per-row attribution, approximately 33 frames/write. They are not evidence of
the present code. Compare new loss/frame/throughput figures on matching hosts,
including 64 clients and the payload case; retain every unmet target and its
ranked phase/CPU cause. A correct performance FAIL is valid evidence of an
unmet target, while functional failure or invalid integrity evidence is not
benchmark completion.

For already-correct code, extraction/moves, docs, generated output and task
wiring, use regression, static caller inspection, generation and measured
evidence; no manufactured RED. If tests expose a missing executable behavior,
record the real RED/GREEN/REFACTOR cycle before changing that behavior. Append
results for all seven acceptance criteria, commands, test counts, artifact
locations, residual risks and excluded items here at implementation handoff.

## Material Stop Conditions

- The draft amendment is not approved: do not implement the behavior that
  conflicts with revision 3, and do not claim this proposed task is ready.
- A required solution changes tenant/auth/audit/loss semantics, public SDK/API
  behavior, accounting/refusal/reservation policy, or migrations beyond the
  precise amendment: state the source-backed conflict for spec authority.
- A new dependency/feature, OS-specific mechanism, second outbox/boot framework,
  relaxed gate, fake worker path, silently omitted workload or suppressed loss
  appears necessary: reject that shortcut and expose the actual gap.
- Available profiler/environment resources cannot prove a required class of
  evidence: continue independent proof and retain partial artifacts, but do not
  declare the affected criterion complete. Do not turn failed performance into
  a policy or threshold change.

Normal compilation/test errors, fixture setup, equivalent exact-selector
corrections and private implementation decisions remain implementation work.

## Authority Links

- [Approved spec](../spec.md) and [revision-4 amendment](../spec-revision-4.md)
- [AGENTS.md](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)

## Implementation Status

Criteria 1–4 and 7 are implemented on this branch. Their proof is the shared
startup, engine-owned refresh, ordered shutdown and two-tenant attribution
journeys committed with them. Criteria 5–6 are release benchmark evidence:
- the revision-2 evidence is recorded;
- the revision-4 matrix found capture shedding under closed-loop saturation,
  which led to [revision 5](../spec-revision-5.md);
- the release matrix rerun on the revision-5 code is deferred at the
  maintainer's request (2026-10-21).

The task stays `in-progress` until that rerun closes criteria 5–6.

## Revision 5 Evidence

Implementation of [revision 5](../spec-revision-5.md) (REQ-016 to REQ-019,
AC-015 to AC-019) on this task, committed in `58baeee97`.

### Implementation

- `wyrd-runtime::outbox`
  - `LossReason` (`shed|rejected|unreachable|shutdown`) labels every
    `outbox_events_lost_total` increment.
  - `OutboxLimits { items, bytes }` with `OutboxSink::weight`. The ¾/¼
    reserved split applies to bytes. Unreserved staging sheds on either
    limit. A spent reserved slot always keeps its item, and `reserve` refuses
    once the reserved items' bytes reach their share. `OutboxFull.unit`
    names which limit refused.
  - Admission happens under the queue read lock, so shutdown settles byte
    shares exactly.
  - Shed records are aggregated to at most one per outbox and tenant per
    `SHED_LOG_INTERVAL` (1 s), each carrying the count; the remainder is
    flushed when shutdown begins.
  - New counters: `outbox_items_written_total` and
    `outbox_bytes_written_total`.
- `ScribeSink`
  - `LANES = 4`. A capture and every audit decision get their lane from a
    hash of the request ID, so one call shares a lane. Verifier inputs and
    results, server spans, and unprojected audits take lane 0.
  - `weight` is a capture's payload texts plus its objects, a span's encoded
    length, or a row batch's Arrow memory.
  - Loss reasons: span-permit exhaustion is `shed`, no route is
    `unreachable`, and terminal refusal or projection failure is `rejected`.
  - `SCRIBE_WRITERS` is unchanged at 4.
- Config: `scribe.outbox_bytes` (default `SCRIBE_BYTES` = 1 GiB, at least 1)
  is wired at boot beside `scribe.outbox_capacity`. Submission outbox bytes
  are unlimited, because its items are bounded by verification input limits.
- Capacity report
  - `Integrity` gains `shed` and `written_items`; `Step.durable` holds the
    items and bytes written during the traffic window.
  - An overhead row that passes except for shedding is `CAPTURE-SATURATED`.
    It is not a failure, never `TARGET`, and is listed in the summary line
    of `report.md`. Its evidence gives the shed count, the shed fraction, and
    durable items/s and MB/s.
  - The drain row and the shutdown quiescence check forgive `shed`. Overload
    forgives `shed`, including that many missing durable rows. Rate,
    recovery and integrity rows still fail on any loss.
- Docs: security posture, Bifrost design, reliability and recovery,
  runbooks, and the docs-site verification limits row.

### Commit-order reader check (REQ-019)

An independent read-only survey of every reader of the six Scribe outbox
tables found one reader that relies on order: the Verifier group read
(`verification/groups.rs`). It orders executions and picks results by
`wyrd_ingested_at`, which follows commit order. Pinning all Verifier inputs
and results to lane 0 keeps today's arrival order for them; the
`pg_verification_groups` and MCP group journeys that assert call order are
therefore unaffected.

Every other reader is safe:
- spans, read by event time;
- results, by point lookup;
- trace timeline, unordered;
- MCP guide queries, by producer time;
- audit log readers, by `created_at` or counts;
- gateway harnesses, as sets, counts, or by `started_at`;
- capacity bins, counts.

No reader uses a cursor over commit order.

### Acceptance evidence

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| AC-015 reasons per path | `outbox.rs` `count_lost`, `lose_late`, `shed`, `shutdown`; `scribe_outbox.rs` reasons | `outbox::tests::bounded_audit_staging_remains_nonblocking` (shed), `shutdown_counts_items_unwritten_at_the_deadline_as_lost`, `concurrent_shutdown_releases_reserved_losses_once`, `shutdown_refuses_items_staged_after_it_begins` (shutdown); `scribe_outbox::tests::an_unreachable_route_drops`, `a_terminal_rejection_never_blocks_later_writes`; `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` | PASS |
| AC-016 bytes before items | `Outbox::stage`, `reserve`, `send_reserved`, `Admission` bytes | `outbox::tests::large_items_shed_on_bytes_before_items` | PASS (release memory evidence deferred) |
| AC-017 report classification | `report.rs` `Verdict::CaptureSaturated`, `Report::saturated`; `envelope.rs` `capture_misses(.., allow_shed)` | `report::tests::shed_only_rows_are_capture_saturated_and_other_loss_fails`, `every_row_requires_latency_correctness_and_zero_loss`; whole capacity bin (20 tests) | PASS (release matrix deferred) |
| AC-018 lanes keep order | `ScribeSink::{LANES, lane}` | `scribe_outbox::tests::a_calls_writes_share_a_lane_and_calls_spread`; `outbox::tests::a_tenants_lanes_write_concurrently_and_each_keeps_order`; commit-order reader check above; `test:wyrd` | PASS (new ceiling deferred) |
| AC-019 aggregated shed log | `Outbox::shed`, `log_shed`, shutdown flush | `outbox::tests::shed_records_are_aggregated_per_tenant_and_interval` | PASS (release log deferred) |

### Verification

| Command | Result |
|---|---|
| `mise run fmt` (`cargo fmt --check`) | PASS |
| `mise run lints` | PASS |
| `mise exec -- cargo nextest run --locked -p wyrd-runtime --lib -E 'test(/outbox::tests::/)'` | PASS (15) |
| `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/scribe_outbox::tests::\|config::tests::/)'` | PASS (95) |
| capacity bin tests (`wyrd-testing --bin capacity`) | PASS (20) |
| `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans` (repository Postgres wrapper) | PASS |
| `mise run test:shared` | PASS (806) |
| `mise run test:wyrd` | PASS (2360) after the fix below; first run 2359/2360 |
| `pg_administration_tests::{gateway_administration_is_authorized_audited_and_tenant_isolated, gateway_apply_is_atomic_and_tenant_scoped}` exact selectors, three runs | PASS |
| `mise run check:tenant-isolation`, `check:deps`, `codegen:check` | PASS |
| `mise run docs:check` | PASS after committing regenerated `docs/public/llms-full.txt` |

#### Diagnosis: cross-call audit order

- **Symptom:** `test:wyrd` failed
  `gateway_administration_is_authorized_audited_and_tenant_isolated`
  (`pg_administration_tests.rs:892`). The expected decisions arrived with
  `put/denied` ahead of two other callers' `list/denied`.
- **Evidence:** `ScribeSink::lane` hashes an audit decision's `request_id`, and
  `SCRIBE_WRITERS = 4` writes those lanes concurrently. The test's `caller()`
  gives every caller a fresh `RequestId`, and nothing settles until the
  assertion.
- **Cause:** the test asserted cross-call commit order, which REQ-019 no
  longer promises. Each caller's own decisions stayed ordered.
- **Fix site:** the test assertions, not production lane selection, which
  would contradict REQ-019. An independent diagnostician confirmed the cause,
  and found one other test with the same exposure:
  `gateway_apply_is_atomic_and_tenant_scoped` (`:1739`). Both now compare the
  decisions sorted. Every other ordered audit or row assertion either uses
  one request or settles between requests.

### Open evidence

The release benchmark matrix on the reference host was not run, by maintainer
instruction. It still has to show:
- only `shed` loss under saturation;
- chat and chat-stream at 64 clients with zero `shed`;
- the 64 KiB capture case keeping memory headroom at 16 and 64 clients;
- the shed log rate;
- the new ceiling against the revision-5 baseline.

Rerun it with the gated serial `bench.sh` before closing the change.
