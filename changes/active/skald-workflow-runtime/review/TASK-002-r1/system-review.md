# System-resilience review: TASK-002

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `e165360b1264d3628b13b02c41567c047bf96930`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 11
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-002-load-and-register-graphs.md`

The candidate and `HEAD` both resolved to the candidate commit when inspected.

## Deployed-path assessment

| Path | Process and dependencies | Failure boundary | Recovery |
|---|---|---|---|
| Offline authored load | Caller process: `WorkflowLoader::load_file` -> filesystem `wyrd_loader::load` -> in-memory `WorkflowGraph` -> Skald hydration (`crates/shared/wyrd-client/src/workflow_loader.rs:74-103`) | One load operation. Local parse, path, graph, tool, or validation failure returns before a runnable `Workflow` escapes. | Correct the bundle or caller tool registry and retry from the file. No server, database, provider, or durable state was touched. |
| Authored load with external refs | Caller process plus stateless Cards HTTP reads: `fetch_missing` performs exact active reads for missing Agent/Prompt bodies (`workflow_loader.rs:142-176`; `cards/reads/get.rs:39-105`) | One read can fail after earlier bodies were fetched, but the partial graph remains private to the failed future and cannot execute. Existing HTTP transport bounds each attempt and retries replay-safe GETs (`transport/http.rs:772-889`). | Retry the whole load after registry/network recovery. Exact `(kind, space, name, version)` and optional UID assertions prevent a recovered request from floating to a newer Card. |
| Registered local load | Caller process -> exact active Workflow GET -> exact active Agent/Prompt GETs -> pure `WorkflowGraph` -> Skald (`workflow_loader.rs:122-176`) | Missing, inactive, foreign-tenant, mismatched, or unreachable dependencies fail the load before provider/tool dispatch. | Retry after dependency or service recovery. Already-returned hydrated workflows are pinned in caller memory; subsequent registry lifecycle changes do not rewrite them. |
| Composite registration | One `wyrd-server` replica -> tenant-RLS `TenantConn` preflight resolution/validation -> separate atomic registration transaction -> Postgres (`components/cards/resolve.rs:46-82,388-440`; `components/cards/service.rs:567-597,995-1004,1055-1128`) | Validation and dependency lookup fail the request before Card writes. The write transaction rechecks every external reference as Active and holds `FOR SHARE` locks through commit (`wyrd-sql/src/queries/cards/relationships.rs:17-54`), closing the preflight/write race. Any write error or handler cancellation before commit drops the transaction; no partial Card graph or idempotency completion can commit. | Retry with the stable idempotency key. If commit completed but the response was lost, existing registration replay returns the stored result. A failed request records only its required standalone authorization audit, not partial Cards. |

The change adds no background worker, queue, process-local server lifecycle, health check, or replica-affine state. All server-side additions run inside the existing request and Postgres topology. Rolling replicas therefore share the database authority and do not need graph cache coordination. Skald remains isolated from registry IO.

## Failure and recovery paths

### Missing registry and dependency outage

An offline loader encountering an authored `ref` fails with `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` (`workflow_loader.rs:160-176`). With a client, an unavailable registry is bounded by the existing HTTP timeout/retry policy; GET retries are safe and cancellation drops a read-only request. A failed dependency fetch cannot return a partially hydrated `Workflow`, so provider and tool execution remain unavailable until a complete retry succeeds.

### Missing, inactive, foreign, and mismatched Cards

`WorkflowLoader::read` requires an Active response, while Cards exact reads assert kind, space, name, version, and supplied UID (`workflow_loader.rs:160-176`; `cards/reads/get.rs:152-205`). The real client/server journey exercises missing versions, wrong kind, bad UID, invalid version, cross-tenant invisibility, pending dependencies, deleted dependencies, and verifies zero additional gateway dispatches (`pg_workflow_registration.rs:612-694`). This fails the requested workflow only; unrelated Cards and server routes remain serviceable.

### Client cancellation and partial graph loading

Every remote operation in `fetch_missing` is a read. Cancellation can leave the server completing a harmless GET, but the caller owns no durable mutation and the private `WorkflowGraph` is dropped. There is no resume token or cache to corrupt; retrying reconstructs the graph and repeats exact reads. This is the smallest safe recovery model for this task.

### Database outage, transaction abort, and concurrent lifecycle change

Preflight lookup or graph validation failure returns before the registration write transaction opens. The preflight `TenantConn` commits only after all resolution and Workflow validation succeeds (`service.rs:995-1004`); an error drops it without writes. The write transaction then re-resolves all external identities as Active with row locks before idempotency reservation or Card writes (`service.rs:1063-1076`). An outage or concurrent deactivation therefore refuses or rolls back the whole attempt rather than persisting a partially bound Workflow graph. Sibling Cards are inserted in topological order inside that same transaction, so a later node failure rolls back earlier siblings as well.

### Server cancellation, crash, restart, and rolling replacement

Before commit, request cancellation, process crash, or replica replacement relies on Postgres transaction rollback. After commit, the existing idempotency record and Card graph are durable and another replica can replay/read them. This task introduces no process-owned graph state, so restart loses no accepted Workflow definition. Local registered loading may span replicas, but each exact read uses shared Postgres-backed registry state and identity assertions. The spec's intentionally ephemeral server WorkflowRun lifecycle is outside TASK-002 and is not altered here.

### Load and validation amplification

The loader and registration validator close only Workflow -> Agent -> Prompt references. `WorkflowGraph::missing` deduplicates exact identities and Prompts add no further graph edges (`workflow_loader.rs:233-264`), so a dependency outage causes bounded transport retries rather than an unbounded traversal. Server validation currently issues additional tenant-scoped reads for Prompt refs discovered inside external Agents; this is finite and request-scoped. No retry loop, background health probe, or crash-on-error path was added to amplify an unavailable dependency.

## Capability impact

- A failure stops only the requested local load or composite registration.
- No failure path crashes the shared server process or disables unrelated Cards, gateway, Bifrost, or Skald capabilities.
- Registration remains fail-closed for integrity: exact Active dependencies and complete resolved validation are required before persistence.
- Offline local loading remains available without a registry when the bundle contains no external `ref`.
- No secret or provider endpoint resolution was added to load or registration.

## Recovery proof and verification assessment

Source evidence establishes transaction rollback, write-time Active recheck/locking, replay-safe exact reads, private partial graph state, and the absence of new process-local server state. Candidate tests provide direct proof for:

- complete client -> server registration with stored locked relationships;
- invalid sibling and external graphs producing identical refusal with no registration operation or Card rows (`registers_only_valid_explicit_workflow_graphs`);
- exact registered loading with a newer Agent version present;
- cross-tenant, missing, mismatched, pending, and deleted references refusing before gateway dispatch (`fetches_and_executes_locked_workflow_graph`).

The task records passing focused tests plus `test:shared`, `test:skald`, `test:cards:integration`, boundary checks, formatting, lints, and code generation. I did not start an overlapping Cargo lane because another Cargo-backed verification was active against the shared target directory during this review. There is no candidate-specific fault-injection test for mid-fetch cancellation, network outage, process restart, or concurrent deactivation. Those paths reuse existing read-only transport and registration transaction/lock owners and do not expose a new recovery contract, so this is a residual proof limit rather than a material finding.

## Material findings

None.

## Overall result

**PASS**

The candidate changes runtime load and registration paths, but all reachable failures remain bounded to the request or caller operation, partial graph state is not externally observable, durable registration stays atomic, exact dependencies are rechecked at the write authority, and restart/replica behavior continues to rely on shared Postgres rather than new process-local state.
