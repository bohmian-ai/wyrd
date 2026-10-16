# TASK-004 independent behavior review

Overall result: **FAIL**. Proposed findings: **BEH-001, BEH-002, BEH-003, BEH-004**.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004`.
- Complete cumulative implementation: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0..07adb7907951f343544e550f6cabc5b7aa3aadea`.
- Packet/evidence input HEAD remained `aa32c821ef74886526b7367bfd77d51c87e0d521` as recorded by subject and confirmed by `git rev-parse HEAD`.
- Approved authority: `spec.md` r7; original `tasks/TASK-004-sdks-examples-and-docs.md`; coordination `tasks/README.md`, including mandatory action/check matrix and deferred integrated execution; approved Task005 handoff.
- Read repository rules, reference router, spec-driven development, maintainer style, testing workflows, error conventions, relevant evaluation reference, and applicable design/SDK/verification sections. CodeGraph was consulted first for changed client and Python capability ownership. The full cumulative name/stat inventory, changed client/foreign-runtime bodies, relevant callers/tests/fixtures/docs and external snapshot were inspected; this is an acceptance pass rather than a completion-summary review.
- External demo candidate inspected at `/tmp/wyrd-task004-external-07adb7907`, including its application/projection/test/build packaging files. It is a frozen filesystem snapshot, not an external implementation commit; reproducible checkout provenance remains limited accordingly.

## Existing owner and caller comparison

| Changed workflow | Existing alternative inspected | Missing behavior and reuse assessment |
|---|---|---|
| Named verification inputs and inline/queued calls | Existing Observe/Run and retained WyrdState client, former `observe/verify.rs`, transport request methods | The new target/evidence projection is needed by the approved slot vocabulary. It extends the same Observe/Run identity and transport; no second durable orchestration was identified. |
| Lazy file pinning and upload | `cards/saga/hash_artifacts.rs::hash_source`, `storage/upload/mod.rs::ArtifactSource`/`FileSource`, `WyrdStorageClient::upload` | Shared stream hashing was extracted and Artifact hashing now calls it. Evidence extends existing upload dispatch rather than adding a PUT client or registering an Artifact. Server-owned digest validation remains Task005. |
| Execution polling and replay | Existing WyrdClient auth/cache and HTTP retry owner | One per-submit key goes through existing replay-safe transport. Execution needs a retained ID/client and owns only polling. Its caller deadline is not applied to the existing request future (BEH-002). |
| Python runtime handle | Existing `client::LazyConnection`, OperatorConnections/Gateway/Bifrost lazy/eager projections | Execution directly wraps only an already-connected native handle and omits the required lazy form (BEH-004); reuse the existing boundary helper. |
| TS and Rust projections | Existing lifecycle/error projection, native spawn, Rust shared-client reexports | Input/JSON and foreign-runtime conversions are legitimate boundary work; no separate transport/poll loop identified. |
| Three-language stories | Existing verify-in-real-time, observe-a-run, scheduled-drift/Operator, support-desk and local provider/server helpers | Reused and extended, including Custom and PSI drift and unready SPC. Missing assets and mandatory authored negative/tool/SPC coverage remain (BEH-001/003). |

## Reconciled acceptance matrix

“PASS” below is task-local acceptance of implementation/authorship, not a claim that deferred server journeys passed. Task005-owned live contracts are assessed only for preservation and handoff.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001/002: consume generic Verifier, named JSON/media/trace/samples slots | `verify/mod.rs::Target::project`; hydrated `state.rs::bound_verifier`; all SDK projections | Shared `values_project_onto_named_slots_by_slot_type`, local wrong-slot journeys | PASS |
| REQ-002/010: construct File locally; hash lazily; missing-digest upload only | `File::{path,bytes,pin}`, `RunEvidence::pin`, shared storage upload extension | `execute_pins_files_and_uploads_only_missing_digests`, `file_refusals_happen_locally_before_any_request` | PASS |
| REQ-010: ArtifactRef keeps exact Card identity and path | `ArtifactRef`, `InputDeclaration::Artifact`; Python/TS thin conversion | Named projection unit test and wrapper/type checks reported | PASS |
| REQ-011: verify Run now; refuse Window with agreed 422 | `Observe::verify`, `Target::execute` | Window refusal authored in each verify-in-real-time story; client tests | PASS |
| REQ-011: submit Run returns queued handle; Window returns Recorded through existing drift producer | `Observe::submit`, `observe/drift.rs` | Authored three-language submit/Window/scheduled stories; isolated projection checks | PASS |
| REQ-012/013: stable key across transport retries; acceptance honest | `Target::enqueue`, `submit_with_idempotency_key`; docs acceptance/loss explanation | `enqueue_replays_its_key_and_wait_returns_only_a_real_result` | PASS |
| REQ-012: status, ID and ambient/explicit reconnect preserve client authority | `Execution::{id,status,connect,connect_with}` and SDK projections | Shared reconnect/error tests and authored same/foreign-tenant reconnect status+wait paths | PASS |
| REQ-012: wait bounded by supplied timeout, including delayed status IO | `Execution::wait` checks time only after awaiting status and after result/error returns | Existing tests return status immediately; slow-response/deadline proof absent | FAIL — BEH-002 |
| REQ-012/INV-002: Python retains IO-free constructor alongside eager connect | `PyExecution` has only static connect and already-connected inner Execution | No lazy form or lazy/eager Execution tests | FAIL — BEH-004 |
| REQ-012/015: only real result, terminal error or honest timeout; completion alone not a verdict | Poll loop handles completed-without-result and terminal error catalog mapping | Both shared wait tests; lost-input behavior represented by pending/no-result timeout | PASS except deadline issue |
| REQ-007/008: preserve built-in names; do not register arbitrary local callables as durable tools | Wire Agent `tool_names`, generated shared vocabulary, unchanged local/runtime distinction | `fs.read` authored journey and declaration/type checks | PASS for implemented projection |
| REQ-008 / task Scenario3: hosted Card/Bifrost/code tools, denied and delegated authority authored | Building-block Agent fixture declares only fs.read; no hosted Card/Bifrost/code dispatch scenario in authored SDK journeys | Direct Cards/Bifrost delegation tests exercise different entry paths | FAIL — BEH-003 |
| INV-001/002: one shared behavior owner, native-only wrappers | Rust client Target/Execution; Python wrappers; TS NativeExecution; Rust export | Reported Clippy, dependency, N-API and runtime checks; source comparison above | PASS |
| INV-003: no client tenancy/permission decision or execution-ID authority | Requests contain exact Card/Run/input data, shared transport provides authenticated client; reconnect does not grant rights | Cross-tenant reconnect/invoke and exact/missing verifier grant journeys authored | PASS |
| INV-004/REQ-013: no durable SDK queue, polling service, writer or exactly-once claim | Client-only status polling and HTTP staging; explicit loss documentation | Source review; server durability final integration deferred | PASS |
| INV-006: failed verdict stays data, missing evidence does not become success | Shared typed result and wrappers; missing-policy/trace, uncited verdict stories | Three-language assertions authored | PASS |
| REQ-014 / Scenario5: Service registration, actual fitting, submit/drain, scheduled result and failed-only Operator | `scheduled_drift_alerts_operator` uses actual baseline wait, result reconnect and receiver; passing/empty cases; observe-a-run Custom | Three-language lifecycle authored, execution explicitly deferred | PASS |
| Mandatory README drift row: PSI, SPC, Custom scheduled scoring | PSI and Custom scenarios exist; tier-SPC fixture is deliberately unfit text data only | No ready/fitted SPC scoring journey | FAIL — BEH-003 |
| Mandatory README action/check negative rows: Agent schema/timeout and code timeout | Existing building-block stories cover uncited judge, raised code, forbidden file access, malformed/oversize files | No schema/Agent timeout/code timeout journey cases | FAIL — BEH-003 |
| REQ-004/AC-003: every format's exact output reaches consumer in all SDKs | `document-formats.yaml`, exact expected output maps in all three building-block test files | Sample paths refer to ignored assets missing from immutable candidate | FAIL — BEH-001 |
| REQ-016: matched support-policy assets and examples including immediate/background/reconnect/failing answers | `examples/support-desk` shared Cards/PDF and Rust/Python/TS `check`/`serve`/`wait`; external demo uses real SDK File/Execution and packed package | Authored support-desk and external app tests; no end-to-end runtime claim | PASS (authored; limits below) |
| Scenario4 examples/demo expose request adaptation and result presentation with authored proof | Authored support-desk and external application assertions plus wrapper local tests | Runtime typing/lint checks reported; actual SDK/server execution deferred by approved policy | PASS for authorship; no additional framework required |
| REQ-017: REQ-004 reference has JSON per format, null/order/table/locator/visual semantics | `reference/extraction-output.md` | Reported docs check; source review | PASS |
| REQ-017: complete runtime setting reference, startup/pool/deadline/security semantics and no false availability claim | `reference/verification-runtime-limits.md` includes approved settings and explicit pending parity confirmation | Reported docs check; final startup implementation belongs to Task003/005 | PASS task-local; final parity remains deferred |
| REQ-008/017: catalog, grants and independent domain/tool gates documented | `reference/agent-tools-and-authorization.md`, permission guide | Docs source review; runtime gate proof is server-owned | PASS for prose; missing hosted journey BEH-003 |
| REQ-017/INV-008: migrated SDK emits, old judge/example surfaces removed rather than aliased | Deleted eval/verify paths, Python eval module and replaced fixtures; updated guides/design/generated source | Reported codegen/export/dependency/doc/example checks | PASS for assigned cutover; old server/types final removal explicitly Task005 |
| r7 handoff: MCP exact names, dispatch/binding/template/SQL/lineage final server ownership | MCP doc table/spec/task handoff agrees; SDK state consumes shared contract | Task005 named seam obligations; no live MCP claim | PASS task-local |
| Non-goals: no general application runtime, SDK authorization, provider engine, UI redesign, extra durable client lifecycle | Cumulative source and diff inventory | Static review | PASS |
| AC-010/task verification policy: isolated checks now, full journeys once after Task005 | Supplied evidence and task execution policy | Orchestrator independently reran exact shared observe/verify selector: 28 passed, 233 skipped; other green results are reported, not reproduced here | PASS for permitted execution scope; authorship gaps remain |

## Proposed findings

### BEH-001 — Missing committed extraction samples make all three format journeys unrunnable

Classification: **MISSING**. Violated obligations: TASK-004 Scenario4 and acceptance criteria for all three-language action/check journeys; REQ-004/016, AC-003/005/008.

Exact locations: `sdks/wyrd-sdk-rust/tests/integration/verifier_building_blocks.rs:616-670`, Python `test_verifier_building_blocks.py:290-298`, TS `verifier-building-blocks.test.ts:287-353`; required files `fixtures/cards/verifier_building_blocks/evidence/sample.{pdf,pptx,docx,xlsx,csv,json,txt,md,html}`.

Evidence and reachability:

- `git ls-tree -r --name-only 07adb7907 fixtures/cards/verifier_building_blocks` contains only five YAML files, no sample document assets.
- The working checkout has all nine assets, but `git status --short --ignored fixtures/cards/verifier_building_blocks` reports `!! .../evidence/`; root `.gitignore` contains `**/evidence/`.
- All three tests directly construct `File` from those paths. Rust `support::fixture`, Python `support.FIXTURES`, and TS support `fixture()` simply resolve repository fixture paths. `PolicyDesk::start`, `policy_desk_bundle`, and TS desk setup register/hydrate Cards and configure the local provider; none creates the files. Repository searches of mise/scripts/fixtures setup found no sample generator. The owning mise journey lanes directly invoke these suites and have no evidence-asset generation step. `fixtures/README.md:26` explicitly calls the samples checked-in fixture content.
- Shared `File::pin` calls `sha256_file` and returns `WYRD_VERIFICATION_400_INPUT_INVALID` for missing files before any execution. Therefore a clean checkout cannot execute the exact-output obligations, regardless of Task005 integration.

Consequence: clean CI/developer checkout fails the first PDF iteration and never tests any extraction format; local ignored files concealed an incomplete deliverable.

Smallest correction: commit the existing required deterministic sample assets (or place them in an ordinary tracked fixture directory and update the three consumers together). Reuse checked-in fixture assets; no new generator/harness or permanent check is needed. Preserve the root rule protecting actual review evidence. Focused closure: confirm all nine exact paths are in the remediation candidate tree and readable from a clean candidate export; typecheck/compile affected consumers if paths change. Final live format journeys remain the agreed merged-change proof.

### BEH-002 — wait can exceed its deadline and return a result arriving after timeout

Classification: **INCORRECT**. Violated obligations: REQ-012 caller-supplied bounded timeout; TASK-004 Scenario2 and status/wait acceptance.

Exact location: `crates/shared/wyrd-client/src/verify/execution.rs:108-143` (`Execution::wait`, especially unbounded `self.status().await` before deadline test).

Producer-to-consumer evidence: wait computes a deadline, then immediately awaits `status`. `status` calls the existing `HttpTransport::request_json`, which invokes `send_with_retry`; GET is replay-safe and may spend the configured request timeout per attempt plus transport backoff and credential refresh. None receives the remaining wait budget. The wait loop returns a result or terminal error before checking its deadline. A delayed 200 result received after a 100 ms wait budget is therefore returned as success; a stalled status request with default seconds-long transport timeout postpones the documented SDK wait timeout. A zero budget still starts a server read. All Python/TS/Rust consumers delegate to this one shared function, including example multi-execution waits. Existing tests cover only immediately returned status responses and bounded sleeps, not IO under deadline.

Consequence: a caller cannot bound waiting with the advertised timeout; a single slow status/auth/read path can substantially overrun its budget, and an expired wait can produce a late verdict instead of the declared timeout.

Smallest correction: apply the one caller deadline to the complete shared wait operation, including each status future and its existing auth/retry/read work, using Tokio's existing deadline primitive at this owner. Preserve standalone `status`, shared transport retry policy, terminal error behavior before deadline, delayed visibility and completed-without-result behavior; cancellation of polling must not cancel the server execution. No wrapper timers or transport fork. Focused closure: local scripted status reply delayed past a short wait budget must produce `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` near that budget and never return the late result; retain existing delayed-visibility/error tests. Run the focused shared-client tests only.

### BEH-003 — Mandatory authored negative/tool/SPC journeys are missing

Classification: **MISSING**. Violated obligations: TASK-004 Scenario3/4 and required README action/check matrix; REQ-008, AC-003/006/007. This is authored coverage, not a request to run deferred suites early.

Exact locations: `fixtures/cards/verifier_building_blocks/policy-review.yaml`, `refund-reader.yaml`; SDK building-block journey files; `fixtures/cards/verify_in_real_time/tier-drift.yaml`; `fixtures/README.md:31-68` coverage table.

Evidence: building-block journeys cover successful extract/agent/code, raised code, forbidden host-file access, an uncited judge and malformed/oversize media. The fixture has no schema-failure or deadline case, and the complete Rust/Python/TS building-block files have no Agent schema/timeout or code-timeout test. The only preparation tool declaration/call is `fs.read`; no journey declares and drives hosted Card/Bifrost tools plus `code.run` through the Invoker under allowed/denied/delegated authority. Existing direct Cards/Bifrost SDK delegation tests do not enter the hosted tool dispatcher and therefore do not close Scenario3. The drift stories exercise real PSI and Custom scoring and an unready SPC baseline on a text column; no fitted numeric SPC scoring journey exists. These are explicitly mandatory rows in `tasks/README.md:119-139`, not optional resilience additions. Supplied task evidence itself acknowledges part of this gap, but source is the basis here.

Consequence: the final merge cannot run the required negative/tool/SPC paths because they have not been authored. Healthy `fs.read` and assertion/PSI scenarios cannot expose a hosted tool authorization/authority-propagation regression, schema acceptance bug, ignored code/Agent deadline, or broken ready-SPC adapter.

Smallest correction: extend the existing shared fixture/story and local provider/server setup, mirrored in all first-class SDKs, to author (1) Agent invalid schema and timeout with dependent requirement semantics, (2) code action/check timeout with no passing dependent result, (3) hosted Card/Bifrost/code tool calls with independent tool/domain refusals and delegated-subject attribution, and (4) numeric fitted SPC scheduled scoring. Keep existing PSI/Custom/no-dispatch coverage. Reuse the existing provider mocks, permission/delegation helpers, fitter and server; no separate harness. Record exact focused selectors and compile/typecheck authored tests now; execute the full journeys only at merged change review as already agreed.

### BEH-004 — Python Execution omits the approved lazy construction path

Classification: **MISSING**. Violated obligations: REQ-012 explicitly says Python retains its lazy form; INV-002; TASK-004 construction conventions; `architecture/wyrd-design.md:310-318` requires every Python server handle to have an IO-free constructor beside eager connect.

Exact locations: `sdks/wyrd-sdk-python/src/verify.rs:96-158`, `python/wyrd/verify/__init__.pyi::Execution`.

Evidence and callers: PyExecution retains only an already-connected shared Execution. Its methods expose static `connect`, `id`, `status` and `wait`; there is no `#[new]` or authored `__init__` for reconstructing an ID without IO. Static connect immediately invokes shared connect/connect_with, which resolves credentials eagerly as intended. Pure-Python verify merely reexports this class, so it supplies no lazy constructor. `Observe.submit` also constructs only this connected wrapper. Contrast existing `OperatorConnections`/Gateway/Bifrost boundary owners, which use the shared SDK `client::LazyConnection`, offer IO-free `__new__`, and connect once on first server call. A caller holding a stored execution ID cannot create the promised offline handle with an optional explicit/ambient client; construction raises TypeError instead.

Consequence: Python lacks one of the approved reconstruction forms and cannot defer credential/environment resolution until status/wait, unlike other handles and the specified first-class language ergonomics.

Smallest correction: add the lazy ID/client form through the existing Python LazyConnection boundary helper and use the same retained native Execution for both lazy/eager/submitted handles; preserve eager connect's credential exchange and cheap local ID property. Update authored stubs/exports/docs consistently. Do not move polling/auth into Python. Focused closure: Python runtime test constructs a handle without configuration/network IO, then proves first status or wait resolves once using the supplied client (or ambient client), and eager connect remains eager; use existing local runtime server doubles and the nearest Python unit/typecheck lanes.

## Verification limits and non-blocking calibration

No source was edited. No Cargo/codegen/build command was run by this reviewer. The orchestrator's independent shared-client run passed 28 tests; reported broader Python/TS/lint/docs/codegen/example results were treated as evidence claims and not as proof of missing paths. Full server journeys intentionally remain unexecuted until Task005; that agreed scheduling alone is not a finding. Runtime configuration parity, MCP dispatch/binding cutover, generic Operator templates, analytical columns and lineage remain the explicit Task005 handoff and were not reclassified as missing Task004 server implementation. The unchanged Python example LookupError behavior is not newly reported as an independent defect here. No optional redesign or speculative hardening is requested.
