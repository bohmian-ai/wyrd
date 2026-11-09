# System review — TASK-001-R3

Result: **PASS** within the assigned failure-propagation and recovery scope. No material proposed system findings.

Subject: immediate base `a387f2a47a145a867c8a4948966fcbd136af9961` to immutable candidate `37b3c2f96b1bd425bf1f0923a1950d0363c40907`. HEAD matched the candidate when inspected. Only this report is written.

## Scope and authority

Reviewed approved revision 4 REQ-002, REQ-004, REQ-005, REQ-006 and REQ-008; TASK-001-R3; root AGENTS; task-review skill; spec-driven workflow; maintainer guide; `architecture/wyrd-design.md` In-memory authoring and registry/predeclaration lifecycle; and `architecture/operations/deployment-and-release.md` supported topology. There is no CodeGraph index. Subject records that no nested AGENTS applies to this write set.

The new declaration dispatch, deferred code-file reads and local Artifact decoding require fresh tracing. Prior review-01 `system-review.md` supplies source-backed evidence for the unchanged saga, preparation scratch ownership and pull/hydration tail; those concrete paths were checked again against the current callers rather than assuming the old authoring representation remains valid. No unrelated server resilience audit is implied.

## Deployment and ownership

The server remains one logical external gateway with potentially multiple replicas/role pods, durable Postgres and durable object storage. This diff changes client-side assembly and dispatch. It adds no server worker, role, health dependency, durable checkpoint, retry loop, process exit or deployment configuration.

- `authoring/mod.rs:617-683` reads code sources during envelope/snapshot decoding; `Graph::declaration` at 911 first gathers captures, decodes Cards and local Artifact metadata, then calls existing `declaration_input_from_cards`. Parsing enters `wyrd_loader::parse_card_value` → `parse::parse_value` → existing `parse_single_envelope`; tree validation/order remain the existing owner. Snapshot failure occurs before the publication loop.
- `cards/handle.rs:295-320` dispatches native inputs to `submit`, paths to the existing file loader, and Cards to snapshot/validation followed by sequential local Artifact publication and graph submission. Each Artifact independently calls the same `upload_artifact` with its own metadata/spec/source. Registered references contribute no local publication. Graph source maps remain empty; local paths do not become wire fields.
- `cards/handle.rs:355-369` retains existing `spawn_blocking(ArtifactUpload::prepare)` and keeps the resulting upload object alive while awaiting its saga. `wyrd-loader/src/upload.rs:48-56,147-151,215-245` retains extraction scratch in that object and maps extracted paths into that request's sources. Replacing the internal call with `submit` reaches the same saga as the old native-input `declare`.
- `cards/saga/mod.rs:51-83` prepares/hashes, mints a request key, submits, uploads, completes and verifies Active status. `build_submission.rs:39-50` works on cloned submissions and validates source correspondence before any network write; storage remains the transfer owner. Authentication remains the existing `RegistryEngine.client` transport through `saga/submit.rs:25-35` and `/v1/cards`; no new permission or tenant authority is introduced.
- `state.rs:652-666` resolves one client, creates Cards from that client, dispatches declaration, then passes that same client and receipt root to unchanged `pull_declared` at 683-710. Exact selection, complete bundle mode, caller bundle and private temporary-directory ownership remain the prior implementation.

## Credible failure and recovery paths

| Failure or interruption | What stops, survives and resumes | Evidence and consequence |
|---|---|---|
| Incomplete/invalid graph, duplicate identity, unreadable or invalid UTF-8 code source | Snapshot errors before any Artifact or graph request. A later call re-reads source files and revalidates current fields. | `Card::envelope`, `Card::decode`, `Graph::declaration`; local `field_edits_defer_validation_and_preserve_snapshots` explicitly constructs before a file exists, observes validation refusal, writes it and compares fresh snapshots after an edit. No failed snapshot is cached. |
| Local Artifact missing/invalid source or extraction failure | Existing preparation errors stop that Artifact and prevent later publications/graph submission. Earlier successful Artifact calls remain registered. | `Artifact::local` validates metadata/spec without reading bytes; `ArtifactUpload::prepare` owns filesystem/extraction checks. Existing real-server Artifact journey observes missing-source refusal and absence of its Service version. The approved contract allows fallible preparation at this boundary; it does not promise a transaction across all local sources. |
| Cancel during blocking Artifact preparation | The waiting operation stops; blocking work can finish and its abandoned upload/scratch is dropped. That preparation has not submitted a request. | Unchanged `upload_artifact` spawn/await ownership. There is no spawned publication task following preparation independently of the caller. |
| Source changes after preparation | Existing manifest hashing verifies source size/digest before submission; transfer/completion maintain existing integrity validation. Graph submission waits for successful publication. | `saga/build_submission.rs`, `saga/hash_artifacts.rs`, unchanged storage handoff. No candidate bypass or relaxed hash check. |
| Network outage, timeout, server replica replacement or cancellation after an Artifact submission | Current saga may leave pending, partially uploaded or Active durable state; later Artifact/graph steps do not run without its successful return. Already completed earlier publications survive. | Unchanged saga error propagation, upload bounded concurrency and completion lifecycle. Scratch has the same owner and lifetime as before. No new fanout, retry amplification or compensating deletion. |
| Graph refuses after successful Artifact publication | Containing graph returns its registry refusal; successful Artifact remains available for an explicit reference on retry. | `Cards::declare` only propagates failure. `authoring::local_artifacts_reuse_and_keep_versions` creates unresolved external component after publication, verifies retained policy publication and declares a corrected graph using the registered policy. |
| Retry after completed or interrupted call | A new call uses existing immutable version/content rules and a newly minted saga key. It is not transparent replay of an uncertain interrupted request. | Existing saga/idempotency owner; journey proves reuse, immutable-content drift refusal and old-version attribution. No stronger interrupted-call recovery promise is added. |
| Graph accepted but pull/hydration errors or caller cancels | Registry state stays committed and caller receives no ready state. Private temporary directory is dropped. Caller-owned bundle retains existing staged-publication behavior. | Unchanged `pull_declared`; `CardGraphPuller::pull` explicitly aborts on errors, `BundleWorkspace::publish` promotes staging with prior-bundle restore. Cancellation can leave sibling staging for caller-owned bundles, as already documented; this diff does not change it. |
| Client process exits | Local snapshot/preparation work and disposable scratch are lost; accepted server records/object transfers remain governed by existing registry reconciliation. Restarting the client makes an ordinary new declaration. | No durable client checkpoint or resume machinery was added or removed. No unrelated serving capability shares the client operation's lifecycle. |

## Acceptance and proof

| Scoped obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| Fresh validated snapshot before publication; deferred IO errors remain fallible | `Card::envelope/decode`; `Graph::declaration` before `Cards::declare` publication loop | Local deferred-edit/file-read test and YAML/code diagnostic test | PASS |
| Existing Artifact publication, isolated sources, scratch, immutable version and error semantics | `Artifact::local`; `Cards::upload_artifact`; existing preparation/saga owners | Real-server `local_artifacts_reuse_and_keep_versions` assertions, including missing source, changed bytes and old versions | PASS |
| Published Artifact survives later graph refusal and can be reused | Sequential awaits, no rollback branch | Explicit retained-policy/refused-graph/recovery section of that journey | PASS |
| Native input/path/Card dispatch reaches existing lifecycle; references skip local publication | `Declarable` adapters and dispatch; unchanged file loader and saga | Authoring parity/refusal journey, native callers, path-state regression source | PASS |
| One client and unchanged pull/hydrate, bundle and cancellation behavior | `WyrdState::declare_with` and unchanged `pull_declared` | `declare_and_pull::declares_ready_verification_state`: supplied client, reader refusal, Artifact-backed state, caller bundle and offline reload | PASS |
| No changed server failure domain or retry policy | Production diff and traced owners | Source comparison with prior system proof | PASS |

No material system finding is proposed. Snapshot/inventory and durable identity completeness outside these failure-propagation paths remain the assigned independent reviewers' scope.

No Cargo or fault injection was run; root coordinates execution. Crash/outage/cancellation conclusions rely on current ownership and unchanged lower-layer source, with real failure/recovery test assertions inspected but not claimed as independently executed here.
