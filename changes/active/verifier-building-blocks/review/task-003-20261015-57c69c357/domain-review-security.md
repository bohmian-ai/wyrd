# TASK-003 independent security / RBAC / tenancy review

Result: **FAIL**.

Subject: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0..57c69c3579e4c331e8d3258e00b22ee7aa0155cb`. Source and authority remain unchanged. This report is limited to accepted authority, protected tool operations, evidence tenancy, workspace confinement, and mandatory worker isolation/resource boundaries. No other discovery report was read.

## Authority and source coverage

Read the task-review skill's domain role, repository agent rules and reference router, approved spec/task snapshots, security posture, relevant Wyrd doctrine/design, and cumulative changed surfaces. Used CodeGraph before navigating owners and then expanded through real consumers.

| Boundary | Source/caller coverage | Assessment |
|---|---|---|
| Tool invoke authorization | `hosted/tools.rs::Invocation::call`, `Builtin::invoke`; shared `audit::authorize`, permission checker and exact/global tool permission representation | PASS: invoke permission is checked and its allow/deny decision staged before argument parsing/domain work; registration does not grant authority. |
| Card domain authorization and tenancy | Hosted dispatch → `components/cards/routes.rs::{list_cards_for,get_card_for,get_card_by_ref_for}` → Card service tenant connections; sibling HTTP/MCP callers | PASS: existing domain owners remain authoritative; no loopback MCP or replacement registry. Tenant comes from retained Caller, not tool arguments. |
| Bifrost domain authorization | Hosted dispatch → `bifrost/service.rs::{list_tables,describe_table}` and `query/collect.rs::BoundedQuery::run` → `query/service.rs::{stream_query,oracle_context}` → Oracle `authorize_resolved_tables`; sibling MCP/HTTP paths | PASS in source: tool permission adds no table permission; query context retains principal/tenant/delegation, and Oracle checks every resolved table under existing scope coverage before data access. |
| Original subject, delegated authority, attribution | `HostedTools::new/open`, invocation fields, `Caller::{accepted,attribution}`, shared audit builder; `AgentExecutor::invoke`, per-call ToolSession and RecordedTool | PASS in supplied component: accepted Caller is cloned intact per invocation, with no model-supplied replacement, global mutable caller, SYSTEM grant or permission re-resolution. Queued run acceptance/restart wiring is TASK-005, not implemented here. |
| Evidence namespace/integrity | `TenantEvidence::{read,media,cited,failed}`, `tenant_path::evidence`, existing TenantMedia and tenant registry consumers | PASS: tenant-qualified digest key, bounded length/read, digest and format checks, redacted storage errors, reachable/pinned-slot citation validation. No host or remote URL is accepted as a file locator. |
| Paths, archives and mounts | `Workspaces::open`, all Workspace materialization/read/search/path/archive helpers, related tests; `Invocation::run`; installed monty-pool/monty-fs 1.1.0 mount construction and path-security owners | PASS for confinement: fresh TempDir, archive link/special-entry refusal, normal path segments, descriptor-confined parent-serviced mounts, read-only inputs. FAIL for per-invocation write/scratch budgets (SEC-003-02). |
| Worker isolation | Entire hardened worker, Sandbox execution/process lifecycle, actual worker tests; installed monty-pool worker spawn/checkout, mount dispatch | Empty environment, new network namespace, Landlock and no-new-privs exist. One worker checkout per process avoids interpreter reuse. FAIL for required separate OS identity (SEC-003-01). |
| Proof limits | Hosted tool/evidence/workspace tests, blocks_agent tests, actual-worker test source and supplied implementation evidence | Existing focused tests cover basic deny/allow, forged arguments and isolation; they do not establish either finding's invariant. Full protected-owner/delegation/restart journeys are deferred by task authority to integrated review. |

## Material findings

### SEC-003-01 — VIOLATION: worker retains the server's host UID/GID

**Obligation.** REQ-008 and the Deployment security paragraph explicitly require a separate unprivileged worker identity. TASK-003 scenario 4/5 requires actual host enforcement and forbids shipping known unmet isolation.

**Location.** `crates/wyrd/wyrd-monty-worker/src/main.rs:87` (`confine`) and the unchanged credential inheritance at installed monty-pool 1.1.0 `src/worker.rs::Worker::subprocess`.

**Producer → consumer evidence.** The pool uses `Command::new(worker)` with an empty environment but no credential change. The worker then unshares user/network/IPC namespaces, applies rlimits, no-new-privs and Landlock, and serves. Neither path changes UID, GID or supplementary credentials. Creating a user namespace with no ID mappings does not replace the process's host kernel identity; an overflow UID as seen from inside the namespace is not a dedicated host credential.

I independently launched the existing candidate worker with an empty environment, sent the malformed frame used by the isolation test, waited for its four-byte protocol reply (therefore confinement had completed), and inspected `/proc/<pid>` from the parent namespace. The read-only check ran through `mise exec -- python3` and returned:

```text
Confinement protocol reply: 4
Parent effective UID/GID: 1000 1000
Uid: 1000 1000 1000 1000
Gid: 1000 1000 1000 1000
NoNewPrivs: 1
uid_map: ''
gid_map: ''
```

The existing `worker_enters_isolation_before_serving` test asserts namespace inequality and no-new-privs, but never asserts distinct host credentials. The crate documentation's claim that this runs as `nobody` does not establish that boundary.

**Consequence.** Worker code runs under the server's host account instead of the separately required unprivileged identity. Landlock and namespace controls still provide their own restrictions; this is not a demonstrated Python escape. It nevertheless omits the explicitly approved OS credential separation between potentially hostile native interpreter execution and the credential-bearing server. Native operations not mediated by those controls retain the server account's identity.

**Smallest correction boundary.** Establish distinct unprivileged OS credentials at the real worker launch/confinement boundary before serving, preserve the existing environment, filesystem/network, process and interpreter controls, and refuse execution if that identity cannot be established. An unmapped display UID must not stand in for changing host credentials. This is fulfillment of an already approved mandatory boundary, not a request for another sandbox framework.

**Closure proof.** Extend the actual-worker isolation proof to observe the worker from the parent's namespace after confinement, assert effective host UID/GID differ from the server and that server supplementary groups are not retained, then retain the existing mount/network/state-isolation cases. Demonstrate refusal when the required identity cannot be established. Do not weaken the approved boundary to match the present implementation.

### SEC-003-02 — INCORRECT: `/work` quota resets each call and total scratch excludes code writes

**Obligation.** Spec Deployment rows require a cumulative writable `/work` budget per invocation (50 MiB default) and maximum scratch bytes per invocation (250 MiB default); REQ-008 retains `/work` across tools and INV-007 requires bounded scratch.

**Location.** `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:263`–`264` (`Invocation::run`); related scratch accounting at `hosted/workspace.rs:192`–`197`.

**Producer → consumer evidence.** Every `code.run` creates a new writable MountSpec with the entire `work_write_max_bytes` limit and feeds it to a fresh worker. In installed monty-pool 1.1.0, `build_mount_table` constructs a fresh monty-fs Mount; `Mount::with_root` initializes `write_bytes_used` to zero. `common::{check_write_limit,commit_write_bytes}` account only writes through that mount instance. The host `/work` directory persists in Invocation's Workspace, so each subsequent tool call receives the full quota while retaining earlier files. Workspace's `scratch` accumulator covers only initial `place` calls and is discarded after materialization; parent-serviced code writes do not charge it. Skald's actual loop also permits concurrent tool invocation (`buffer_unordered`, default cap 8), so per-call mount counters do not serialize or share an invocation budget.

A permitted Agent can therefore write two distinct files in two `code.run` calls, each smaller than 50 MiB but together larger than 50 MiB. More calls can exceed the 250 MiB total scratch bound; a workspace materialized near that bound can exceed it with the first legal code write. No escape, extra permission or unsupported Python feature is required: `Path.write_text` is already exercised by the real-worker test. Existing tests prove only a single mount's quota, not retained invocation state.

**Consequence.** A tenant with granted `code.run` can exceed its approved invocation disk budget and consume shared server scratch beyond the process workspace admission assumptions. Tool result limits and worker-memory limits do not bound retained host files.

**Smallest correction boundary.** Keep cumulative writable and total materialized scratch accounting with the invocation's existing Workspace/resource owner, and enforce the remaining budgets before parent-serviced writes. Reuse the current descriptor-confined mount dispatch; ensure repeated and concurrent code calls share the same accounting rather than replenishing quota on checkout. Preserve read-only inputs, cleanup and interpreter freshness.

**Closure proof.** A focused real-worker/hosted-tool test with small limits must perform two individually permitted writes across separate code.run calls and reject the write crossing their combined `/work` limit, without retaining bytes beyond the cap. Also prove the same invariant under concurrent calls and when materialized inputs leave less remaining total scratch than the `/work` allowance. No full service suite is necessary for this seam.

## Verification and exclusions

This review did not rerun service suites or modify implementation/tests. It independently performed the worker credential observation above and traced the quota failure through pinned installed dependency source. The first attempt used `mise exec -- python` and failed because that executable is absent; the identical `python3` check passed. Root-provided green focused lane outcomes do not cover these missing assertions.

No claim is made that ordinary Python can escape the current mount or network restrictions, or that a tenant/caller authority leak was found. TASK-005 acceptance, persisted authority/restart wiring, SDK journeys, cloud storage and deployment/container wiring were not independently executed here. Findings are limited to mandatory implemented security boundaries; optional hardening is excluded.
