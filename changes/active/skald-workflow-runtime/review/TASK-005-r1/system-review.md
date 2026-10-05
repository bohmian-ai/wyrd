# System-resilience review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`

The candidate was still checked out at the stated commit when this review
started. This review was source-only as directed: no build, test, or executable
verification command was run.

## Deployed-path coverage

| Changed path | Process and dependency path | Failure and recovery behavior established from source |
|---|---|---|
| Authored local CLI run | `wyrd workflow run --file` -> `wyrd_client::Workflow::from_path` -> shared loader/Card hydrator when refs exist -> Skald -> Native, public Wyrd gateway, or direct external gateway | Loading and selected-secret reads remain outside the CLI owner. Dropping the local run future drops Skald-owned execution; an already-issued provider call is not rolled back or resent. The candidate adds no scheduler, durable owner, or recovery loop. |
| Registered local CLI run | CLI selector -> `crate::client::from_global` -> `Cards::workflow().load` -> pinned graph -> Skald | Registry or dependency outage returns the shared stable error before dispatch. The exact loaded graph is retained for the run. A dropped CLI process ends local execution rather than moving ownership to the server. |
| Server submission and wait | CLI -> `crate::client::from_global` -> `wyrd_client::Workflows::create` -> `/v1/workflow-runs` -> process-owned preparation/execution; then `Workflows::wait` polls GET once per second | Create uses the shared idempotent submission transport. After acceptance, the server's tracked owner, not the HTTP connection, owns preparation/execution. A poll error stops only the CLI wait; the already printed run ID remains usable. Restart loses the process-local run by design. |
| Detach, status, and cancel | CLI -> shared `Workflows::{get,cancel}` -> owning server replica | Detach returns the accepted snapshot. Status observes complete snapshots. Cancel records the signal before awaiting drain, so a cancel-request disconnect does not revoke the signal; a retry is safe. Missing, foreign, evicted, expired, process-lost, and non-owning-replica identities remain the common not-found response. |
| CLI interrupt while waiting | `tokio::select!` races `Workflows::wait` with `tokio::signal::ctrl_c` | The CLI reports the accepted ID and exits 130. Dropping `wait` stops polling only; it neither calls cancel nor calls create again. The server run remains owned and active. |
| Server shutdown | server shutdown token -> `WorkflowRuns` admission closure -> preparation/run cancellation -> tracked-task drain; gateway work drains under the gateway owner | No candidate production change weakens this path. The updated tool test now uses the repository test-support state rather than creating an unowned lazy pool. |
| Multi-replica and restart topology | gateway affinity -> one `wyrd-server` process-local run table | The architecture updates correctly state that create/replay/get/cancel require affinity, a non-owner returns not found, and restart performs no recovery or provider replay. No cross-replica or persistent machinery entered the diff. |
| SDK local journeys | Rust, Python, and TypeScript public Workflow/Card facades -> shared client -> one Skald runtime | These remain local-process capabilities. Test-only provider/gateway outages terminate or fail the local run through existing typed results; the candidate adds no language-specific execution owner. The approved Cards/CLI-apply split is preserved. |

## Failure-path assessment

| Failure or interruption | What stops | What remains available | Recovery and proof assessment |
|---|---|---|---|
| CLI wait receives Ctrl-C | Only client polling | Accepted server run and its run ID | Source and the recorded compiled-CLI journey prove exit 130, continued running state, eventual success, and no second provider execution. |
| CLI process exits after detach | CLI process | Server-owned run, GET, and cancel on the owning replica | Direct consequence of the shared client/server ownership split; the recorded journey exercises detach, status, cancellation, and idempotent repeat cancellation. |
| GET/poll dependency failure | Current `wait` future | Server run | `Workflows::wait` returns the first GET error without cancellation or resubmission. The printed ID permits a later status call. This matches the approved client contract. |
| Submission response loss | Current transport attempt | Server preparation/run once accepted | Shared idempotent submission reuses one key across transport retries. Existing server/client evidence covers replay; the CLI adds no retry owner. |
| Provider or gateway outage | Affected Workflow step/run according to Skald terminal semantics | Server and unrelated capabilities | Candidate route journeys record protocol refusal before dispatch and terminal failure as a snapshot. The broader shutdown, settlement, and retry invariants remain in the cumulative server/Skald source and recorded gate evidence. |
| Server graceful shutdown | New Workflow admission and active/preparing runs | Unrelated server shutdown owners continue through their own drain | `WorkflowRuns::drain` closes admission, cancels, and drains tracked work. No candidate code changes the boundary. |
| Server crash or rolling replacement | Every process-local queued, running, and retained Workflow run on that replica | Durable registry data and other replicas | Deliberate V1 loss boundary; docs now state affinity and restart loss rather than implying recovery. Callers submit a new logical run if they still need an outcome. |
| Alternate endpoint or failover for an authored file | The CLI invocation is rejected by Clap before loading | Ambient configuration remains usable | **Material gap SYS-001:** the new CLI forbids the ordinary endpoint override precisely on authored files that may need registry hydration or the public Wyrd gateway. |

## Affected capabilities

The changed runtime-facing capability is the CLI projection over existing local
and server Workflow owners. Its failure behavior also affects authored bundles
with registered references, local public-gateway execution, registered local
execution, accepted server runs, and operational routing to a selected Wyrd
endpoint. The documentation changes affect deployment expectations for
multi-replica affinity and process restart. The SDK changes are journey proof,
not new runtime ownership.

## Material proposed findings

### SYS-001 — `DRIFT` — authored files cannot use the ordinary server endpoint override

- **Violated obligation:** Spec Revision 14 REQ-026 says the existing
  `--server <url>` connection option continues to select the Wyrd endpoint and
  must not be overloaded as an execution-mode flag. TASK-005 likewise requires
  ordinary `--server` and authentication behavior to be preserved while file
  execution remains local. An authored file may contain registered refs or a
  `wyrd_gateway` route, so local execution can still require a Wyrd endpoint.
- **Exact location:** `crates/wyrd/wyrd-cli/src/workflow.rs:105-107` declares
  `server` with `conflicts_with = "file"`. The new assertion at
  `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:217-253`, specifically line
  242, freezes that extra prohibition as usage failure.
- **Evidence:** Registered local and server paths pass the override through
  `crate::client::from_global(self.server.as_deref())` at
  `workflow.rs:242-249` and `workflow.rs:279-282`. The authored-file path at
  `workflow.rs:213-215` instead calls `Workflow::from_path` after Clap has
  rejected `--server`; the shared authored loader lazily needs a client for an
  external Card ref and local `wyrd_gateway` execution needs a public gateway
  client. Neither the approved spec nor the established CLI meaning makes a
  connection endpoint incompatible with a local file source.
- **Observable system consequence:** A user cannot direct an authored local
  Workflow with an external registered dependency or public Wyrd-gateway route
  to an alternate, failover, or explicitly selected Wyrd endpoint. The command
  exits with usage code 64 before loading, forcing mutation of ambient
  configuration even though `--server` is the CLI's established per-invocation
  endpoint selector. This narrows dependency recovery and makes the option's
  behavior source-dependent.
- **Testable correction:** Remove the file-specific prohibition and make the
  selected endpoint participate in the shared authored-file client composition
  used for external-ref hydration and public gateway calls. Reuse the existing
  client configuration, authentication, loader, and Workflow runtime owners;
  do not add CLI transport, an environment-mutation shim, or a second config
  owner. Add a compiled-CLI journey where ambient configuration cannot reach
  the server, `--file ... --server <bound-server>` contains an external
  registered ref or `wyrd_gateway` route, and the registry/gateway request is
  observed only at the explicit endpoint while execution remains local.

No other material system-resilience finding was identified.

## Verification assessment and limits

Recorded evidence in the task reports a green `mise run gate`, all three
ignored compiled-CLI journeys, the focused Rust/Python/TypeScript journeys, the
Python gateway fixture move, `docs:check`, and `git diff --check`. The lifecycle
journey directly records detach/status/cancel, SIGINT, continued server
execution, terminal success, and provider-call count. Existing cumulative
server source and tests cover process-owned preparation, idempotency,
foreign-owner lookup, retention, shutdown, and gateway settlement. Per the
review instruction, none of those commands was rerun.

The recorded CLI contract test treats `--server` plus `--file` as an intended
parse failure, so green recorded verification does not close SYS-001; it proves
the drift is implemented consistently. Live cloud/provider credentials are not
required for this capability, and the local controlled dependencies are the
appropriate proof class.

## Overall result

**FAIL**

The process ownership, interruption, shutdown, and explicit restart-loss
boundaries are otherwise coherent, but SYS-001 leaves one approved and
operationally important dependency-selection path unavailable.
