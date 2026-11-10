# System resilience review — TASK-001

Result: **PASS** within the assigned runtime and recovery scope. No material proposed findings.

Subject: base `5234961d51798581a026834cfffd5efaadc226b4` to candidate `790da3369c1b0107765db234d72c1ae0c4680446`. HEAD remained the candidate at the end of inspection. First review; no carried-forward evidence. Only this report was written.

## Scope and authorities

Reviewed publication-first sequencing, preparation and scratch ownership, cancellation and errors between publications, retry/retained state, and regression risk from factoring state pull/hydration. Inputs were approved `changes/active/card-authoring/spec.md` revision 3 (REQ-005, REQ-006, REQ-008, INV-002, AC-001, AC-003), original task, root AGENTS.md, task-review skill, `changes/spec-driven-development.md`, maintainer guide, registry lifecycle in `architecture/wyrd-design.md`, and supported topology in `architecture/operations/deployment-and-release.md`. No `.codegraph` exists; ordinary source navigation was used. No nested AGENTS.md was found under the affected shared crates or Rust SDK.

## Deployed path and ownership

The deployed server remains one logical gateway surface, potentially multiple replicas/role pods, with durable Postgres and object storage. This candidate changes SDK-side composition; it adds no server worker, readiness dependency, process exit, or deployment configuration.

1. `authoring/mod.rs:206` snapshots dependencies, deduplicates them, and calls loader validation/order. `wyrd-loader/src/lib.rs:172` delegates to the existing `validate_tree`, `order_cards`, and `submission_for`; local source paths stay outside submissions.
2. `cards/handle.rs:346` calls the existing `upload_artifact` separately for each local Artifact, awaiting each success before advancing. Only then does it call existing `Cards::declare` for the containing graph. Existing CardRef inputs have no local Artifact dependencies and skip preparation entirely.
3. `cards/handle.rs:307` moves preparation into the existing blocking task, keeps the returned `ArtifactUpload` in scope across `self.declare(&upload.input).await`, and returns the existing exact root. `wyrd-loader/src/upload.rs:48-56,147-151,215` gives that value ownership of extraction scratch. Its temporary files therefore remain present until transfer/completion finishes or the owning future is dropped.
4. Existing `cards/saga/mod.rs:51` prepares, submits, uploads, completes, and verifies Active state. `saga/submit.rs` uses authenticated `POST /v1/cards`; `saga/upload.rs` delegates bounded transfers to storage; `saga/complete.rs` uses `/v1/cards/{uid}/complete`. Durable lifecycle remains server owned. There is no new retry loop or recovery state.
5. `state.rs:698-712` uses the same client for authored declaration and hydration. Both path and authored entry points now call `pull_declared` (`state.rs:722`). The base-to-candidate diff moves the existing tail without changing its complete-pull mode, exact root selector, supplied-client binding, caller bundle handling, or temporary-directory lifetime.

## Failure and recovery assessment

| Reachable interruption | What stops and what survives | Source/proof assessment |
|---|---|---|
| Invalid complete local graph | SDK call errors before publishing any Artifact | `CardDefinition::declaration` completes before the publication loop; loader owns validation. No server/process impact. |
| Missing source, changed bytes, extraction error/collision | Current Artifact preparation/publication errors; later Artifacts and graph are not submitted. Any earlier successful Artifact publications remain durable | Existing `ArtifactUpload::prepare` and saga hash/source validation are called unchanged. No merged source map is introduced. The new real-server Artifact journey covers unreadable source and changed-version bytes. |
| Cancel while blocking preparation runs | Caller receives no result; detached blocking work may finish, then drops its prepared scratch; no submission occurs for that preparation | Existing `spawn_blocking` behavior and ownership unchanged. Candidate awaits it through `upload_artifact`; no second background registration task exists. |
| Transfer/completion outage, timeout, replica replacement, or cancellation after submission | Current saga can leave pending/partially transferred/Active server state; outer call does not proceed to the containing graph without successful Artifact return | Existing saga and storage handle failure. Earlier successful publications survive. Client-local scratch is dispensable after cancellation; no promise of atomicity or transparent replay was added. Server metadata and reconciliation remain existing owners. |
| Graph refused after Artifact publication | Graph declaration errors while the Artifact remains readable/reusable; no compensating delete | `handle.rs:357-369` only propagates the error. `authoring::local_artifacts_reuse_and_keep_versions` forces a real unresolved component after publication, checks policy visibility, then reuses its registered reference in a successful graph. This proves changed recovery behavior beyond the healthy path. |
| Caller reruns after success or failure | Existing publication/version rules and fresh saga idempotency keys apply; no new backoff, retry amplification, or rollback loop | Same `upload_artifact` and `declare` owners. Journey checks unchanged graph identity and refusal of changed bytes at an immutable version. Uncertain interrupted publication is not represented as automatically resumable by this adapter. |
| Declaration succeeds, subsequent pull/hydration fails or is cancelled | Server declaration remains committed; no state is returned. Private temporary directory drops; caller-owned pull retains existing destination until publication | Shared tail is unchanged. `CardGraphPuller::pull` uses isolated staging and explicit abort on errors; `BundleWorkspace::publish` retains its rollback behavior. Its existing documented caller-owned staging residue on cancellation is unchanged, not a candidate regression. |
| Server process failure while unrelated capabilities serve | Errors are scoped to the SDK operation/request and existing dependent services | No added server task or process-fatal handling; the adapter uses normal authenticated existing endpoints. Nothing in this change takes unrelated server capabilities offline. |

## Acceptance and verification matrix

| Obligation in scope | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Local Artifacts publish separately before graph; registered references skip upload | `handle.rs:346-369`; separate existing `upload_artifact` calls | New Artifact journey and parity journey inspected; graph contains no heavy submission | PASS |
| Preparation validation, scoped source maps, scratch lifetime and existing failure/retry semantics remain owned by current machinery | `handle.rs:307-321`; `upload.rs:48-56,147-151`; saga prepare/upload/complete owners | Source tracing; existing owner checks unchanged; journey proves extracted policy is mounted and immutable bytes refuse | PASS |
| Successful Artifact persists when graph fails and can be reused | No cleanup branch in outer sequencing; saga owns completed publication | Explicit post-publication failure/recovery assertions at integration `authoring.rs:925-978` | PASS |
| Both state inputs use existing pull/hydrate and one identity | `state.rs:648-750`; unchanged tail extracted into `pull_declared` | `declarations_match_files_and_keep_refusals` compares code/path component refs; existing `declare_and_pull::declares_ready_verification_state` inspected for caller bundle/offline payload coverage | PASS |
| No new durable state, upload engine, lifecycle or retry policy | Production diff and traced existing consumers | Static comparison of complete changed runtime path | PASS |

Task evidence records all four real-server authoring journeys and local checks passing. I inspected those tests and their assertions, but did not independently rerun them. No Cargo was executed, as directed. Sent the orchestrator the existing `declare_and_pull::declares_ready_verification_state` selector as the narrow additional regression check if reruns are performed. Crash/outage and dropped-future paths were assessed through ownership and unchanged lower-layer source, not fault-injection execution; healthy-path tests are not claimed as dynamic proof of those interruptions.

No source, fixture, packet or unrelated report was edited. No optional hardening or new recovery machinery is requested.
