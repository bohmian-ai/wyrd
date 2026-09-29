---
id: TASK-006-R2
kind: remediation
status: implemented
spec: SPEC-verified-change-contract
spec_revision: 40
requirements: [REQ-153, REQ-154, REQ-155, REQ-156, REQ-157, REQ-158, REQ-159, REQ-160, REQ-161, REQ-162, REQ-163, REQ-164, REQ-165, REQ-166, INV-016, INV-017, INV-018, AC-027, AC-034, AC-035, AC-036, AC-037, AC-038]
depends_on: [TASK-006-R1]
parent_task: TASK-006
remediates: [FIND-TASK-006-11, FIND-TASK-006-12]
---

# TASK-006 R2 — Complete Eval and remediate server startup

## Authority and current state

- Approved authority: [spec revision 40](../../spec.md), corrected 2026-09-24 at the user's direction to require live Kubernetes autoscaling proof.
- Original implementation: [TASK-006](../../tasks/TASK-006-continuous-eval-verifier.md).
- Prior review: [R2 verdict](verdict.md); its two validated findings remain in scope.
- The R2 implementation already removed the redundant `WyrdTestServer::superuser_pool` forwarder and added the three negative media records. Its exact terminal-matrix test passes alone, but the full server journey fails when a global 256-entry Scribe tail-ticket nonce map fills. It rejects fresh tickets and audits them as replay. This is the root failure to remove, not a reason to slow or shrink the test.
- Earlier TASK-006 and R1 closures remain required. Do not redo completed Eval engine work.

This is **one remediation task**. The separate server-startup draft was planning context; this packet covers the approved expanded scope. A later readiness review may correct this packet, but must not split it into new tasks without user direction.

## Outcome and value

A developer can pull the published application image, start it with a separate durable Postgres service and artifact storage, run migration and one first-use setup command, then use a real client. A production operator uses the same Wyrd boot contract, with separately supplied credentials and storage. One process needs no peer credentials or loopback private RPC. A peer-enabled deployment starts at one replica and can add a second using runtime-supplied addresses, existing Postgres membership, one shared cluster TLS bundle, and shared object storage. The full TASK-006 Eval journey passes without tail-ticket capacity failure or false replay audit.

## Decisions and boundaries

- Keep one application image containing Rust server, Node BFF, and nginx. Nginx sends actual Rust paths (`/v1`, `/auth`, `/platform`, `/mcp`, `/openapi.json`, `/healthz`, `/readyz`) to Rust without rewriting; UI goes to the live Node BFF. Nginx also exposes public gRPC on port `50051` and forwards it to a distinct internal Rust gRPC listener. Rust, Python, and TypeScript `WyrdClient` surfaces derive the gRPC host and scheme from their effective `server_url` and default to that port; the existing `grpc_url`/`WYRD_GRPC_URL` override remains optional.
- Delete embedded Postgres, its binary download and boot path. The separate `wyrd-server migrate` invocation receives an existing database-owner URL through `WYRD_DATABASE_URL`; serving never receives that owner secret. Normal boot checks migration versions/checksums and RLS/grants and does not migrate. Local Compose may run migration automatically before Wyrd.
- Only two database logins are available to serving Wyrd: RLS-bound `wyrd_app` through `WYRD_DATABASE_URL`, and explicitly privileged `wyrd_platform_admin` through `WYRD_PLATFORM_DATABASE_URL`. The latter owns platform and Iceberg catalog work, never ordinary tenant requests. Nothing has shipped: edit the unshipped migrations and bootstrap directly, deleting `wyrd_migrator`, `wyrd_catalog_app`, and `wyrd_catalog` plus their passwords, grants, and role switching. Reset repository-managed databases with obsolete checksums; do not add compatibility code.
- Keep `WYRD_STORAGE_URL` for Card, artifact, and Bifrost/Iceberg objects; keep `WYRD_BIFROST_DATA_DIR` for each Scribe's private durable WAL, staging, and stable identity. Standalone `file://` storage must be durable. Peer mode requires one shared object store and rejects process-local `file://` before ready membership. Persist one Wyrd signing key; the operator supplies it in production.
- Delete `WYRD_PUBLIC_BASE_URL`. Local artifact upload/download return root-relative authenticated routes resolved against the configured Wyrd endpoint; cloud presigned URLs stay absolute. Prove Rust, Python, and TypeScript clients.
- The default `all` target calls co-located Scribe, Oracle, and Forge directly and needs no private listener, peer credential, or calibration profile. Peer mode is explicit even with one initial replica: it binds the private listener and takes `WYRD_PEER_ADDRESS` and `WYRD_PEER_TLS_DIR`. The runtime supplies a distinct reachable address per instance; Wyrd validates and publishes it through existing `vala.cluster_nodes` role/fence/heartbeat registration. Do not add a registry, scheduler, peer list, or bind-address inference.
- Remote peers use mTLS only. `WYRD_PEER_TLS_DIR` names a directory with `ca.crt`, `tls.crt`, and `tls.key`: one dedicated cluster CA and one shared leaf/key with fixed `wyrd-peer` DNS identity. The CA private key stays outside replicas. Remove peer API keys/JWTs, purpose tickets, ticket keyrings, nonce maps, and `ScribeTailAuthority`. A shared key admits a trusted cluster process, not a tenant or individual replica. Each receiver validates applicable tenant/resource ownership, query/assignment, target fence, deadline, and bounds from trusted state before plan decode or storage IO; preserve public auth, RLS, audit, and fail-closed behavior. Limit the private port to the workload network.
- No prior version has shipped, so do not build a ticket-protocol compatibility shim or old/new cutover. Future deployed upgrades use mutually declared peer/schema/object compatibility or drain before replacement. For failed migrations, stop rollout, repair and retry safely or restore a verified backup; no generic down migration or automatic restore.
- Delete `/v1/authz/check` and `PolicyHook` rather than keep a disabled route, allow stub, or replacement seam. Remove the hook call, policy-only branches, and fake `invoke` policy attribution from delegated token exchange; leave its other behavior intact. Delete `crates/wyrd/wyrd-server/tests/auth_e2e.rs`: every test in that file ends at the deleted route. Remove other obsolete route, hook, OpenAPI, client, test, and documentation references. Keep the existing Python and TypeScript Bifrost journeys that perform real delegated reads and reject writes, and the Rust exchange tests for `sub`/`act` and permission intersection. Do not add a replacement policy gate or a replacement `auth_e2e` suite. Synchronize the security and protocol architecture documents, which currently prescribe the deleted route.
- This remediation must ship and run a local kind autoscaling journey from a tracked script/manifests through `mise.local.toml`. It is a completion gate for this remediation, not a default CI gate. Use a Kubernetes HorizontalPodAutoscaler driven by measured successful analytical-read requests per second, with a threshold around 10, to grow a one-replica deployment to two. CPU or memory utilization is not the trigger; `kubectl scale`, patching `spec.replicas`, or starting a second pod directly does not prove autoscaling. Keep the separate two-process peer journey runnable without kind.

## Projected startup environment

| Process | Inputs supplied by its deployment |
|---|---|
| One-off `wyrd-server migrate` | `WYRD_DATABASE_URL` with the existing database-owner login. No serving database URL or peer key. |
| One-process Wyrd, local or production | `WYRD_DATABASE_URL` (`wyrd_app`), `WYRD_PLATFORM_DATABASE_URL` (`wyrd_platform_admin`), `WYRD_STORAGE_URL`, and a stable `WYRD_SIGNING_KEY_FILE`. `WYRD_BIFROST_DATA_DIR` defaults to `.wyrd/bifrost`, but its path must be mounted persistently wherever Scribe runs. Local Compose supplies the values; production supplies its own. Clients use `WYRD_SERVER_URL` or an explicit `server_url` for both HTTP and public gRPC. |
| Production hardening | Set `APP_ENV=production` (or its equivalent config value). The published image must boot with canonical audit using the same inputs as a local one-process deployment. No external authorization policy hook exists. Public TLS files are needed only if Wyrd terminates the public transport itself. |
| Peer-enabled replica, including the first one | Add `WYRD_PEER_ADDRESS` from per-instance runtime metadata and `WYRD_PEER_TLS_DIR` pointing to `ca.crt`, `tls.crt`, and `tls.key`. In Kubernetes, each pod receives `POD_IP` from Downward API `status.podIP`; the shared pod template sets `WYRD_PEER_ADDRESS` to `$(POD_IP):<peer-port>`. The pod UID is not the network address. `WYRD_STORAGE_URL` must point to one shared object store. `WYRD_TARGET` is needed only for split-role workloads; its default is `all`. |
| Selected optional feature | Provider SDK credentials for the chosen cloud store; `WYRD_SEALING_KEY_FILE` when OIDC client secrets are configured; `WYRD_SERVER_TENANT_SLUG` for configured trusted issuers or workload bindings; `WYRD_GRPC_URL` or the client's `grpc_url` only when public gRPC uses a different address; public gRPC certificate/key files only for direct TLS termination. |

The application still accepts optional bind, logging, metrics, tracing,
resource, pool, request-limit, storage-tuning, and verification overrides.
They have defaults and are not required in the basic journey. The obsolete
database-password, peer-ticket/API-key/certificate-path, and
`WYRD_PUBLIC_BASE_URL` inputs must be removed from parsing and examples. Do
not claim that every optional tuning variable is being deleted.

## Execution contracts for the new boundaries

Reuse these exact owners instead of adding a migration service or a second
membership registry:

| Boundary | Existing interface or required signature shape |
|---|---|
| One-off migration | `wyrd_sql::migrate(&PgPool) -> Result<(), SqlError>` followed by `vala_sql::migrate(&PgPool) -> Result<(), SqlError>`. Both already take their own advisory lock. The only caller with an owner pool is the `wyrd-server migrate` command; remove migration calls from both runtime `connect_from_dsns` paths. |
| Runtime database readiness | `WyrdPostgres::connect_from_dsns(&ResolvedDsns)` and `ValaPostgres::connect_from_dsns(&ResolvedDsns)` become serving-role-only constructors. Their owning handles gain `validate_schema(&self) -> Result<(), SqlError>` checks for their own applied migration versions/checksums and effective RLS/grants. `ResolvedDsns` carries only the app and platform serving URLs in serve mode. |
| Peer membership | Keep `ClusterRegistry::{reserve_scribe, reserve_oracle, activate, deactivate, start_readiness_heartbeat, refresh_snapshot, shutdown_role}` and its `RegisteredRole` fence; reserve creates `ready=false`, activate publishes ready. Use `ExecuteFragmentRequest.target_fence`, `TenantTableBinding`, and `TailReadFence` as the existing typed request precedents; add only missing typed context fields to the affected private messages. |

The required order is:

```text
migrate: parse owner WYRD_DATABASE_URL -> open owner pool -> wyrd_sql::migrate
         -> vala_sql::migrate -> validate migrated versions/grants -> close pool -> exit
serve:   parse app/platform URLs only -> connect serving pools -> validate both
         schemas and effective grants/RLS -> build state -> bind listeners -> ready
         (any validation error exits before a ready listener; no DDL is attempted)
peer:    validate advertised address and TLS files -> reserve fenced roles
         as not-ready -> recover local Scribe/Oracle state -> bind mTLS peer
         listener -> activate roles -> start heartbeat/snapshot poller -> ready
stop:    deactivate exact roles -> stop new work -> drain -> stop heartbeat/poller
         -> shutdown_role for exact fences -> close listener
receive: TLS authenticates cluster process -> validate bounded typed context
         against local tenant/resource, query/assignment, target fence,
         deadline and limits -> decode executable plan or touch tenant storage
```

Existing per-crate migration advisory locks serialize their own migration
tables; do not add a second lock layer. If Wyrd migration succeeds and Vala
migration fails, serve remains unready and rerunning `migrate` resumes the
idempotent Wyrd step before retrying Vala. During peer startup, reserved roles
are invisible to live snapshots; a bound listener rejects work until its
exact role is active. A peer certificate establishes cluster-process identity
only. A missing or mismatched context fails closed before plan decode or
storage IO, with a truthful security refusal rather than a replay allegation.

## Ordered implementation scenarios

### 1. Fresh database and separate migration

**Behavior.** A fresh external Postgres database is provisioned with only the two serving roles. `wyrd-server migrate` uses the owner URL, applies Wyrd/Vala migrations, validates grants and checksums, and exits without starting the application. Serving with app/platform URLs then becomes ready. Missing, wrong-checksum, or RLS/grant-deficient schema stays unready. No owner credential enters serving environment or mounts.

**RED.** Add the fresh-database and refusal assertions to the planned image journey `mise run test:server:startup`; they fail on embedded boot, role-dependent old SQL, startup migration, or missing schema check. Also run the existing `mise run test:sql` after changing SQL.

**GREEN.** Remove embedded boot/dependency paths; revise unshipped SQL/bootstrap for owner-run migration and two serving roles; add the migrate command and normal-boot validation. Keep the owner credential confined to the one-off process.

**REFACTOR.** Reuse the existing ordered Wyrd/Vala migration functions and SQL ownership checks; remove old DSN branches and passwords rather than wrap them.

### 2. Image, first-use setup, nginx, and persistence

**Behavior.** A pinned application image plus separate durable Postgres starts with the same Wyrd inputs as a production one-process deployment. The published image also starts with `APP_ENV=production` and canonical audit without `PolicyHook`. Wyrd still authorizes its own API requests. `/v1/authz/check` is absent from the router and served OpenAPI, and a request to it receives the normal unknown-route response. Existing delegated token exchange still works without the no-op hook or fake `invoke` audit attribution. One idempotent setup command creates a platform administrator, first tenant, and usable client credential. A real client writes and reads through nginx. The public Rust SDK `WyrdClient`, given only `server_url` or `WYRD_SERVER_URL`, completes a real public gRPC call through the published image's nginx port `50051`; an explicit gRPC URL override still works. Restart preserves credential validity, object bytes, and acknowledged Bifrost data. The Node BFF serves a real server-backed request; nginx health and MCP streaming use actual Rust routes.

**RED.** The planned `mise run test:server:startup` builds and starts the official image in both development and production profiles and drives setup/client/restart/BFF/router requests; today its documented path cannot complete and production fails the stub-policy gate. The image exposes no public gRPC proxy and `WyrdClient` does not derive gRPC from `server_url`. The image journey uses the public Rust SDK for a real gRPC call; public Python and TypeScript constructor tests cover the same URL resolution and override. Existing exchange tests remain green; the obsolete route and fake `invoke` attribution disappear, while Wyrd API permission denial still works.

**GREEN.** Keep Rust, Node, and nginx in one image; correct the route template and live BFF server client; add an nginx HTTP/2 gRPC listener on public port `50051` with `grpc_pass` to a distinct internal Rust listener; derive the default gRPC address from the effective client `server_url` in the shared Rust client and keep Python/TypeScript public constructors, types, docs, and tests aligned while preserving the optional override; remove direct-public-TLS requirements when TLS terminates at the edge; delete the policy hook, obsolete route and its production gate while preserving Wyrd API authorization, delegated-token restrictions, and canonical audit; expose first-tenant setup through a binary already in the image; persist the local signing key and distinct storage/Bifrost paths.

**REFACTOR.** Reuse existing platform initialization and tenant provisioning; delete misleading nginx routes and `WYRD_SERVER_PORT` rather than introduce a second router.

### 3. Artifact URLs and all first-class clients

**Behavior.** A standalone `file://` backend returns root-relative authenticated upload/download URLs without `WYRD_PUBLIC_BASE_URL`. Rust, Python, and TypeScript clients use them through nginx and do not send credentials to a foreign origin. Cloud presigned URLs remain absolute.

**RED.** Extend `mise run test:server:startup` and the owning `mise run test:storage:matrix` with local and cloud URL assertions; add public-client journey assertions under `mise run test:bifrost:journey:python` and `mise run test:bifrost:journey:typescript`.

**GREEN.** Remove the public-base input and emit relative local routes. Use the existing shared Rust client's authenticated relative-URL resolution and fix only client projections that fail the journey.

**REFACTOR.** Keep one URL-resolution path in the shared client, not language-specific copies.

### 4. Local engine calls and TASK-006 terminal matrix

**Behavior.** A one-process `all` server starts without peer TLS, bearer keys, tickets, or calibration profile. Oracle uses local Scribe transport and local workers. The existing terminal-matrix journey covers valid media plus unbound, unsupported MIME, oversized, and cross-tenant negative records; refused records settle errored without result, dispatch, or provider request. The full server journey no longer fails from nonce-map exhaustion or emits false replay audit.

**RED.** Keep the existing focused test and full lane as the regression proof. The focused test is:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_runs_the_terminal_matrix)"'
mise run test:bifrost:journey:server
```
The focused test has passed alone; the full lane fails on fresh tail-ticket capacity, which is the expected RED for this remediation.

**GREEN.** Wire existing local Scribe transport, remove local peer dialing and the ticket/nonce authority, and retain the already implemented R2 negative-media and raw-pool fixes. Do not weaken assertions or slow the test to avoid capacity.

**REFACTOR.** Delete unused ticket paths and config after all private RPC callers are converted; keep local calls direct.

### 5. Peer discovery and actual distributed execution

**Behavior.** Start one peer-enabled replica using shared object storage, its own persistent Bifrost path, an address injected from runtime metadata, and the shared TLS bundle. Start a second from the same template with a different address. The first discovers it through existing membership without restart, and a real analytical query dispatches remote Oracle work over mTLS. Cross-pod Scribe tail reads work. Stopping the second removes it from selection after the existing liveness cutoff; a broken RPC fails the query without partial results. A local-only `file://` backend, missing TLS files, malformed address, or unreachable advertised address refuses readiness or remote work as appropriate.

**RED.** The planned `mise run test:server:peer` starts two real server processes against repository-managed Postgres and shared object storage and asserts a worker-ID-bearing remote RPC. Today the first process cannot self-publish a reliable runtime address without manual peer setup.

**GREEN.** Reuse `vala.cluster_nodes`, existing role fences/heartbeats/poller, and remote Oracle dispatch. Form the URI from `WYRD_PEER_ADDRESS`, bind the listener before ready registration, validate peer TLS, and retain a private durable path for each Scribe.

**REFACTOR.** Remove old advertise-address fallback, manual peer name, duplicate membership configuration, and obsolete test fixtures.

**Required local kind proof.** Deploy the published image with one ready peer-enabled pod, external Postgres, shared object storage, a private durable Bifrost path per Scribe, and the shared mTLS files. The pod template injects `POD_IP` from `status.podIP` before expanding `WYRD_PEER_ADDRESS=$(POD_IP):<peer-port>`. Reuse Wyrd's existing `wyrd_http_requests_total` counter for successful `POST /v1/query` reads; a test-local Prometheus scrape and custom-metrics adapter expose its per-second rate to a Kubernetes HorizontalPodAutoscaler. Set the HPA target near 10 reads/second and `maxReplicas: 2`. Start below threshold, then send roughly 12–15 small successful reads/second with bounded concurrency and a hard duration limit until the HPA changes desired replicas from one to two. This is an illustrative local load target, not a production scaling default. Do not set the replica count in the test. Wait for the second pod to become ready; assert distinct registered peer addresses and a query whose execution evidence identifies remote Oracle work over mTLS. Fail on missing metrics, scale timeout, missing membership, local-only execution, or a failed peer handshake. Give all test-local components resource limits and clean them up on success or failure. Put the script and manifests in the repository and expose one example `mise.local.toml` task. Run it once for this remediation and record the command and result; it need not run on every CI task.

### 6. mTLS receiver boundary without tickets

**Behavior.** A peer with the dedicated CA and fixed identity may call the private plane. Unrelated CA, expired/wrong-identity certificates, and forged tenant/resource/query/assignment/fence/deadline contexts fail before plan decode or tenant IO. Every private RPC family is covered: query forwarding, reservations, stage/fragments, shuffle reads, and Scribe tail list/acquire/read. Normal public principal admission, tenant RLS, and audit remain intact. No signed ticket, peer API key, or replay-capacity path remains.

**RED.** Add negative cases to `mise run test:server:peer` and the owning `mise run test:bifrost:journey:oracle`; each currently depends on tickets or lacks the required receiver check. Record the inventory of private RPCs and their trusted fields in task evidence before deleting the old authority.

**GREEN.** Authenticate cluster members at TLS, carry only necessary typed request context, and verify each receiver's tenant/resource ownership, local role fence, query/assignment, deadline, and resource bounds before decode/IO. Remove ticket code and fields after consumers use those checks.

**REFACTOR.** Keep one trust model across private RPCs; delete ticket signing, verification, keyrings, and nonce cache instead of preserving unused compatibility.

### 7. Migration failure and future upgrade rules

**Behavior.** A failed one-off migration exits nonzero and stops serving rollout. Normal boot rejects unmigrated, checksum-mismatched, or security-deficient schemas. Retrying a recoverable migration preserves tenant and audit data. No owner secret appears in serving pods. Future incompatible peer releases refuse overlap before payload decoding; no test pretends the unshipped ticket protocol is a deployed version.

**RED.** Add forced failure/retry and schema rejection to `mise run test:server:startup`; add incompatible-protocol refusal to `mise run test:server:peer`.

**GREEN.** Reuse the existing Wyrd and Vala migration advisory locks, check the resulting schema before readiness, and document verified backup plus repair/retry/restore procedure.

**REFACTOR.** Use existing migration registries and release compatibility metadata; do not add a rollback engine.

## Required test recipes

The two new lanes are tracked `mise.toml` tasks. Their implementations may be
shell-driven journeys or Rust integration targets, but each named case below
must run through the lane and fail if it is skipped. Record the exact focused
`mise exec -- cargo nextest run ... -E 'test(=...)'` command for any Rust test
added to a lane after confirming its name with `cargo nextest list`.

| Case and command | Setup and action | Critical assertions |
|---|---|---|
| `startup_image_journey`, `mise run test:server:startup` | Build the pinned official image; start fresh repository-managed Postgres and durable local storage; run image `migrate` with owner URL; start image with serving URLs; run first setup; use the public Rust SDK for authenticated HTTP write/read and a real Bifrost gRPC operation through nginx using only `server_url`; restart; repeat reads. Run the boot portion under development and `APP_ENV=production`. | Owner URL absent from serve; one setup is idempotent; credentials, objects and acknowledged data survive; nginx HTTP/BFF/MCP and port `50051` work; explicit `grpc_url` still works; `/v1/authz/check` is absent from router/OpenAPI. |
| `migration_refusal_and_retry`, `mise run test:server:startup` | Start serve before migration, then against a wrong-checksum or RLS/grant-deficient schema; inject a recoverable Vala migration failure after Wyrd migration; retry `migrate` and serve. | No serve readiness or tenant data loss before repair; no serving DDL or owner secret; retry completes without a down migration; both schema checks pass before ready. |
| `peer_join_and_remote_query`, `mise run test:server:peer` | With repository-managed Postgres and shared object storage, start one peer-mode process, then a second with a distinct runtime-supplied address and the same cluster TLS bundle; issue an analytical query and Scribe tail read; stop the second. | First process discovers the second without restart; a worker-ID-bearing remote Oracle RPC and cross-process tail succeed over mTLS; stale member disappears and a broken remote call gives no partial result. |
| `peer_context_refusals`, `mise run test:server:peer` | Repeat peer calls with missing/wrong/expired TLS and forged tenant, table, query/assignment, fence, deadline, or bounds; try process-local `file://` in peer mode. | Untrusted TLS never reaches a handler; bad typed context is refused before plan decode or tenant storage IO; no cross-tenant bytes or false replay audit; invalid storage prevents ready membership. |
| SDK URL contract, existing Rust/Python/TypeScript Bifrost journey lanes | Use `WyrdTestServer` and its dynamic gRPC port with the explicit override for real RPC journeys; separately construct each public SDK client with only `server_url` or `WYRD_SERVER_URL`, then with `grpc_url`. | All three constructors derive the same default `host:50051`; explicit override wins. The image lane above is the proof that the derived default actually reaches nginx. |

## Acceptance and evidence

| Obligation | Required proof |
|---|---|
| Prior R2 findings and AC-027 | The raw-pool forwarder remains deleted and the three negative-media records remain; exact terminal-matrix test and full server lane pass. |
| AC-034 | Pinned official image in development and production profiles without `PolicyHook`, with Wyrd API authorization and canonical audit; separate Postgres, migrate, one setup command, public Rust SDK client, restart persistence, nginx actual routes and gRPC port `50051`, real gRPC through the Rust SDK with only `server_url`, optional override, Rust/Python/TypeScript public constructor contract checks, live BFF, MCP streaming, relative local artifact URLs. |
| AC-035 | Fresh database with only app/platform serving roles; owner URL absent from serving; app RLS/catalog denial; platform catalog success; missing/checksum/grant failure and recoverable retry. |
| AC-036 | One-to-two replica discovery with distinct automatic addresses, actual remote Oracle execution and Scribe tail, shared object store, dedicated-CA mTLS, receiver tenancy/fence refusals, no partial results. |
| AC-037 | Future upgrade failure/compatibility behavior and documented recovery point; no unshipped-version compatibility shim. Required local kind run shows the successful-read rate crossing the configured threshold, HorizontalPodAutoscaler-driven one-to-two scale, automatic address injection and registration, and remote Oracle execution over mTLS; no direct replica-count change or CPU/memory scaling trigger. |
| AC-038 | `/v1/authz/check` absent from the router and served OpenAPI, with its normal unknown-route response; `auth_e2e.rs`, `PolicyHook`, and policy-only branches removed; fake `invoke` attribution absent; existing Python/TypeScript delegated Bifrost operations, Rust exchange assertions, and Wyrd API permission checks still work. |

## Expected write set and consumer closure

Likely owners: `wyrd-sql`/SQL migrations and bootstrap, `vala-sql` catalog migrations, `wyrd-server` config/boot/CLI/HTTP/Oracle/gRPC, `wyrd-auth` delegated exchange, `wyrd-auth-check` deletion or reduction to genuinely used code, `vala-bifrost-redux` peer transports and protocol, `wyrd-storage` local URLs, shared `wyrd-client` and first-class SDK projections, the official image entrypoint/nginx/Node BFF, Compose examples, repository-managed journey tests and `mise` lanes, and the applicable security/protocol/deployment architecture documents. The file list is guidance, not an allowlist. Update contract generation for the removed route and its wire types. Remove obsolete docs and variables in the same remediation.

## Verification commands

The new `test:server:startup` and `test:server:peer` mise tasks are part of this remediation; their scripts must build/use the pinned official image or real server processes and fail if no assertions run. Confirm exact selectors for any newly named Rust test with `mise exec -- cargo nextest list`; record and run its exact `test(=...)` command in implementation evidence. Required commands after implementation:

```bash
mise run test:server:startup
mise run test:server:peer
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_runs_the_terminal_matrix)"'
mise run test:bifrost:journey:server
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:python
mise run test:bifrost:journey:typescript
mise run test:sql
mise run test:storage:matrix
mise run check:from-pools-allowlist
mise run codegen:check
mise run fmt
mise run lints
git diff --check
```

Run narrower owner tests during each RED/GREEN cycle. Run `mise run gate` only if the final change has no complete capability gate under the repository rule. The optional local kind task is outside this required gate and proves one-to-two automatic scaling only when invoked.

## Stop conditions

Stop for a spec revision only if implementation reveals a materially different public startup contract, database privilege boundary, tenant trust model, persisted format, or peer compatibility policy. Resolve ordinary code structure, test fixtures, and Compose details inside this task. A red lane requires diagnosis and correction; do not label it pre-existing or weaken a gate.

## Prior implementation evidence retained

R2 previously deleted `WyrdTestServer::superuser_pool` and changed its three callers to `server.pg_fixture().superuser_pool()`; the exact terminal-matrix, replay, and stable-error tests and `check:from-pools-allowlist` passed. It added `unbound`, unsupported `image/tiff`, and `MEDIA_LIMIT_BYTES + 1` records; each uses the existing `assert_unresulted` path and passed in the focused terminal-matrix run. The full server lane failed with `QueryVisibilityUnavailable` after trace `tail ticket replay detected`; `ScribeTailAuthority` had filled its server-wide nonce map and classified capacity as replay. These are historical results, not acceptance of the expanded remediation. Re-run the complete evidence after implementation.

## R2 continuation evidence (status IMPLEMENTED)

The earlier BLOCKED entry is superseded. The owner explicitly authorized
deleting `PolicyHook`/`/v1/authz/check` and the embedded Postgres boot.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Prior R2 findings, AC-027, REQ-159 local tail | `201d9533`, `810e0d8f`; raw-pool forwarder deleted; three negative-media records retained | exact terminal-matrix command (1/1); `mise run test:bifrost:journey:server` 22/22, no replay audit | PASS |
| AC-034 image, setup, nginx, gRPC 50051, BFF, MCP, restart, relative artifact URLs | `578c13f7`, `858ac8f9`, `555b4c88`, `18af9b2d`, `c95a2c38`, `e496133b`, `d05314dd`; Forge local-storage listing `e310c496`; restart fence reclaim `bcefc3a0` | `mise run test:server:startup` PASS (write, migration refusal/retry, production-profile restart verify); `test:bifrost:journey:python` 38 passed; `test:bifrost:journey:typescript` 18/18; `test:storage:matrix` PASS | PASS |
| AC-035 two serving roles, owner-run migrate, schema refusal and retry | `f2d9426c`, `643730ac`, `90a4be84`, `4bdec938` | `test:server:startup` (`WYRD_SQL_503_SCHEMA_NOT_READY` before and after injected failure, retry succeeds); `mise run test:sql` 163+5+113+2 passed | PASS |
| AC-036 one-to-two replica discovery, remote Oracle, Scribe tail, mTLS, receiver refusals | `8b72a070`, `23ca5817`, `2e06f954`, `8fa3caf0`, `3fbdb365`, `19c354e2`, `47f984d9` | `mise run test:server:peer` 9/9 (`peer_join_and_remote_query`, `peer_context_refusals`, listener isolation, transport fences); `test:bifrost:journey:oracle` 28/28 | PASS |
| AC-037 failure and compatibility behavior, documented recovery point | incompatible-protocol refusal in `peer_network::security`; migration repair/retry/restore procedure in `docs/src/content/docs/self-hosting/database.svx` | `test:server:peer` (`peer_context_refusals`); `test:server:startup` migration phase; `mise run docs:check`; `mise run test:server:kind` PASS (see kind journey evidence below) | PASS |
| AC-038 route, hook, policy branch removed | `76a0e057`, `cab57f67`, `b4330b96` | `test:server:startup` (authenticated `POST /v1/authz/check` → 404, absent from served OpenAPI); `pg_router_smoke` unauthenticated 401; delegated Python/TypeScript journeys green | PASS |
| Repository gates | `2b013b28` | `check:from-pools-allowlist`, `codegen:check`, `docs:check`, `py:typecheck`, `fmt`, `lints`, `git diff --check`: all exit 0 | PASS |

Focused commands run for named Rust tests:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=eval_verification::continuous_eval_runs_the_terminal_matrix)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=boot::tests::open_dal_forge_listing_pages_backend_without_start_after) | test(=boot::tests::open_dal_forge_listing_filters_directories_before_page_boundary)'
```

Diagnoses for the failures fixed in this continuation. Each timing or
ordering cause was confirmed by an independent read-only diagnostician.

- **Peer join, second `all` replica never ready.** Symptom: `ForgeCoordinatorUnavailable`. Cause: a second coordinator is a standby by design, because readiness means holding the scheduler fence (`forge/scheduler.rs`). Fix site: the join journey adds an `oracle` replica, not an `all` one.
- **Peer join, graph never settled.** Symptom: the query ran Interactive. Cause: with one remote participant, the planner elides the single-task shuffle boundary. Fix site: the journey runs three Oracles, so the joiner's graph distributes.
- **Listener refusal.** Symptom: `InvalidArgument` instead of an authentication refusal. Cause: a default request fails conversion before the context check, and the Scribe pod also mounts `OraclePeerService`. Fix site: the test sends a well-formed, context-free reserve and expects `Unauthenticated` from Oracle pods and `FailedPrecondition` from the rest.
- **Startup, schema refusal carried no stable code.** Cause: `main` printed the error without its catalog code. Fix site: `wyrd-server` `main` prints `[WYRD_SQL_503_SCHEMA_NOT_READY]`.
- **Terminal matrix, "peer bind is reserved before composition" panic.** Cause: the non-peer test server assumed a peer bind. Fix site: the harness advertises `local://` and uses the process-local registry, like production boot.
- **Startup, Forge worker exited on `file://`.** Cause: the worker refused any backend without native `list_with_start_after`, which includes local fs and Azure. The documented `docker.svx` deployment and this task's durable-local-storage requirement both hit it. Fix site: `OpenDalForgeObjectStore::list_pages` pages on such backends. Each page is one bounded walk that keeps the smallest `FORGE_OBJECT_LIST_PAGE_ENTRIES` keys after the cursor. The refusal, its config flag, and every test capability override were deleted.
- **Startup, UI process exited.** Cause: Node 26 needs `libatomic.so.1`. Fix site: the image installs `libatomic1`.
- **Startup, `/v1/authz/check` probe got 401.** Cause: `/v1` authenticates before routing. Fix site: the lane probes with a token and expects 404.
- **Startup, MCP probe got -32022.** Cause: the server serves only the `2026-07-28` lifecycle. Fix site: the lane probes with `server/discover` plus the required headers and `_meta`.
- **Startup, `RegistryManifestHashMismatch`.** Cause: the journey wrote a hex digest, but the client contract is standard base64 SHA-256. Fix site: the journey test.
- **Startup, rows never visible.** Cause: `Bifrost::sql` reads published data only. Acknowledged rows publish when a generation seals, which takes 10 minutes by default; before that they are guaranteed readable only through the tail (`bifrost-design.md`). Fix site: the journey reads with `Fused` visibility and `Strict` freshness.
- **Startup, `/readyz` stuck after restart.** Symptom: `forge_coordinator_unavailable`. Cause: each process minted a new scheduler owner (`Uuid::now_v7()`), so a restarted sole replica waited out the dead owner's 15-minute lease. Fix site: `ForgeBuildConfig::scheduler_owner` is the durable node id that boot already uses as the worker claim owner. Test fixtures keep a fresh owner per Forge.

Material limits:

- The BFF proof is the live server-backed readiness call only. The session lifecycle is owned by the oidc-production-readiness change.
- In `peer_join_and_remote_query`, the rows are published before the fused read. The read reaches the leader's tail listener, but cross-process tail rows are not proven strictly unpublished.
- Replaying a reservation context is accepted by design. The digests prove consistency and the mTLS identity proves origin.
- A standby coordinator replica is unready by design, so the join adds an `oracle` replica.
- The scheduler-owner fix assumes one live process per durable node id, the same invariant the worker claim owner relies on. A coordinator-only target still mints a fresh node id, and after an ungraceful restart it waits out the lease TTL.
- No default CI kind gate is required; the local kind proof ran once and passed (below). No ticket compatibility shim or replacement policy gate is required.

## Kind autoscaling journey evidence (revision 40)

Command: copy `deploy/kubernetes/kind/mise.local.toml.example` to `mise.local.toml`, then `mise run test:server:kind` (`scripts/server/test-kind-autoscale.sh`, manifests in `deploy/kubernetes/kind/`). Commits `d528caee8`, `5229e1b3d`, `8f566b34f`, `13168cb4e`, `0c9e22ed5`, `d6210b21a`, `827d36474`. Result: exit 0, cluster deleted afterward.

| Requirement | Evidence from the run | Result |
|---|---|---|
| Published image, external Postgres, shared object store, private durable Bifrost path per Scribe, shared mTLS bundle | `docker/official/Dockerfile` image `wyrd:kind`; in-cluster `postgres:16` with only the two serving roles plus an owner-run migrate Job; one RustFS S3 bucket as `WYRD_STORAGE_URL`; StatefulSets with one PVC per pod; `wyrd-peer-tls` Secret from a dedicated CA whose key stays in the script's work directory | PASS |
| Downward API address injection | both templates set `POD_IP` from `status.podIP` and `WYRD_PEER_ADDRESS=$(POD_IP):50052` | PASS |
| One ready replica, below threshold | autoscaled StatefulSet `readyReplicas=1`; custom metric present at `0.0/s`; HPA desired and current 1 | PASS |
| Read-rate HPA, not CPU/memory, no direct replica change | `wyrd_http_requests_total{method="POST",path="/v1/query",status="200"}` → Prometheus → prometheus-adapter → `wyrd_query_reads_per_second` (Pods metric, `averageValue: 10`, `maxReplicas: 2`); bounded Job at ~14 reads/s, ≤8 in flight, ≤240 s, `activeDeadlineSeconds: 300`; HPA desired 2 at measured `13625m` reads/s with a `SuccessfulRescale` event; the script never scales or patches replicas | PASS |
| New replica joins and registers its address | live ready `vala.cluster_nodes`: `oracle https://10.244.0.14:50052`, `scribe https://10.244.0.14:50052`, `scribe https://10.244.0.15:50052`, `scribe https://10.244.0.18:50052` (new pod); no Oracle row for the new pod | PASS |
| mTLS only on the peer port | from the probe pod, a CA-trusting client without a certificate is refused; the cluster leaf is admitted | PASS |
| Remote Oracle execution over mTLS and cross-pod Scribe reads | `kind_join_read` writes rows 5–8 through the new replica's Scribe and a fused read through it returns rows 1–8 (rows 1–4 live on the first replica's tail); anchor `bifrost_query_duration_seconds_count` 502 → 551 while the new replica's stays 0 | PASS |

Topology decision: the autoscaled template is `WYRD_TARGET=scribe` beside one fixed `all` anchor, mirroring `peer_join_and_remote_query`. In production code, public reads execute on the local Oracle when one is ready (`ReadyOracleForwarder::forward`), and the distributed Analytical path is test-only (`query_sql_inactive_analytical`). So remote Oracle execution requires an ingress replica without an Oracle. A second `all` replica is an unready standby Forge coordinator, so it cannot be the autoscaled template.

Diagnoses from the kind runs:

- **Peer listener collided with the image's internal gRPC.** Cause: the image bound gRPC to `127.0.0.1:50052` and the default peer bind is `0.0.0.0:50052`; the config check compared whole addresses. Fix site: `binds_overlap` in `config.rs` refuses equal ports when either address is a wildcard, and the image's loopback gRPC moved to `50053`. Covered by `config::tests::peer_mode_is_explicit_and_validated`.
- **`all` anchor refused to boot at 3 GiB.** Cause: the default Forge compaction budget is four fifths of memory and must fit beside 768 MiB of Scribe, Oracle, and unmanaged floors (`resources.rs::forge_compaction_budget`, no clamp by design), so an `all` pod needs at least 3.75 GiB. Fix site: the kind anchor's limit is 4 GiB (owner's choice); no product change.
- **Second port-forward could not bind 50051.** Cause: the script backgrounded the `k` shell function, so `$!` was a subshell and killing it orphaned `kubectl`. Fix site: the script backgrounds `kubectl` itself and reaps it.

Also updated: `deploy/kubernetes/bifrost/*.yaml` now run in peer mode with Downward-API addresses, the peer TLS mount, and peer port `50052` on the private Service and NetworkPolicy; `OPERATIONS.md` and `docs/.../configuration.svx` document the injection.

Verification after these changes: `mise run fmt`, `mise run lints`, `mise run docs:check`, `mise run test:server:peer` 9/9, `mise run test:server:startup` PASS, and the focused config test:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::peer_mode_is_explicit_and_validated)'
```

Kind material limits:

- 4 GiB boots the `all` anchor but leaves ~50 MiB elastic memory, and the in-container BFF and nginx are outside Wyrd's accounting. Fine for this journey's tiny data; production `all` pods need more or a lower `WYRD_BIFROST_MEMORY_LIMIT_BYTES`. The 3.75 GiB `all` minimum is not yet in the operator docs.
- `deploy/kubernetes/bifrost/deployment-mixed.yaml` keeps `replicas: 3` of `all`; two of those are unready standby coordinators by design. Not changed here.
- The HPA target of 10 reads/s is an illustrative local value, not a production default. The kind node container itself is not resource-limited; every pod in it is.

## Private RPC inventory and trusted receiver checks (scenario 6, recorded before deletion)

Trust model after remediation: mTLS with the dedicated cluster CA and the fixed
`wyrd-peer` DNS identity admits a trusted cluster process only. No bearer,
signature, nonce, or replay cache remains. Each receiver checks the typed
context below against its own trusted state before plan decode or tenant IO.
The unsigned typed context bytes (`PeerContext.claims`) replace
`SignedPeerTicket`; they carry the same typed claims minus key id, signature,
and nonce.

| Service / RPC | Typed request context | Receiver's trusted state and check (before decode/IO) |
|---|---|---|
| `ScribeTailService.ListActiveStreams` | `binding` (tenant, namespace, table), `query_id` | Binding parses to a canonical tenant/table; only that tenant's local seal keys are listed (metadata only, no row bytes). |
| `ScribeTailService.AcquireFence` | `binding`, `time_partition`, `exclusive_sealed` (writer epoch), `deadline`, `schema_fingerprint`, `tail_protocol_version`, `query_id` | Protocol version equals local; `deadline` in the future and TTL bounded by local config; writer epoch equals the local Scribe stream epoch; schema fingerprint equals retained batches; fence capacity bounded; the returned fence retains `query_id` and binding. |
| `ScribeTailService.ReadFencePage` | `fence_id`, `query_id`, `after`, `max_rows`, `max_encoded_bytes` | Fence must be retained locally and unexpired; `query_id` must equal the query that acquired it; tenant read from the retained fence (never the wire); page bounds clamped to local config. |
| `ScribeTailService.ReleaseFence` | `fence_id`, `query_id` | Same retained-fence `query_id` match before removal; idempotent. |
| `OraclePeerService.ReserveSlots` | `query_id`, leader node + fence, class, slot units, expiry, optional graph; context `{audience, worker_fence, body digest, deadline}` | Audience equals local node and `worker_fence` equals the local Oracle role fence; context body digest equals the request with context cleared; expiry bounded; slot units bounded by the local slot manager. |
| `OraclePeerService.ReleaseSlots` | `reservation_id`, `query_id`, leader node + fence; context as above | Same audience/fence/digest checks; release only when the local reservation's query and leader match. |
| `OraclePeerService.ExecuteFragment` | `reservation_id`, `leader_fence`, `target_fence`, `assignments` (`TenantTableBinding`, reader cut), `plan_fingerprint`; context (tenant, query, deadline, assignment-authority digest) | `target_fence` equals local role fence; local reservation exists for the context query and leader; every assignment's tenant equals the context tenant; recomputed assignment digest equals the context; execution deadline in the future; plan bytes bounded — all before plan decode or object IO. |
| `OraclePeerService.ForwardQuery` | context JSON `{audience, worker_fence, expiry, authorized query context, request, deadline}` | Audience/fence equal the local Oracle role; expiry and absolute deadline in the future; size bounded; the authorized context's tenant equals the request tenant before admission. |
| `OracleLifecycleService.List/Get/Cancel` | `tenant_id`, `request_id` | Only locally owned lifecycle entries under that tenant are visible; others return `NOT_FOUND`. |
| DataFusion worker `CoordinatorChannel` (first `SetPlan`) and `ExecuteTask` | `x-wyrd-stage-context` metadata carrying `StageTicketClaims` without nonce | Operation matches the gRPC path; destination node/fence equal local; body digest equals the framed bytes; reservation and graph lease exist locally for the public/datafusion query ids; deadline/expiry in the future; participant cut bounded — before tonic decode, task-cache access, provider construction, or storage IO. |

Known ceiling: without a signature, the context digests prove consistency, not
origin. Origin is the mTLS cluster identity, which is the approved trust model.
