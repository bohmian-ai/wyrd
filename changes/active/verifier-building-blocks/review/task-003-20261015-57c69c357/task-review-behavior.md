# TASK-003 independent behavior review

**Result: FAIL.** Review subject: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0..57c69c3579e4c331e8d3258e00b22ee7aa0155cb`.

Reviewed the original approved revision-6 task and specification snapshots, cumulative changed owners and surrounding callers, and repository/design authority. This is the first TASK-003 review. No implementation summary or another discovery report was used to establish behavior. The task snapshot's appended implementation claims are not acceptance evidence. No source was edited; only this report was written.

## Scope, paths, and reuse assessment

The source paths below are relative to the repository. Hosted paths abbreviate `crates/wyrd/wyrd-server/src/verification/hosted/`. Vala block paths abbreviate `crates/vala/vala-eval/src/blocks/`.

| Workflow | Existing alternative and actual reuse | Missing behavior/new owner assessment |
|---|---|---|
| Agent call → Prompt rendering → provider/gateway → tools → response | Existing `skald-agent::Agent::run_prompt`, Prompt binding, `run_config_from_agent_run_config_spec`, and gateway caller are reused by `blocks/agent.rs`. Existing `Agent::from_card` also resolves tools/configuration, but it expects a hydrated Card and does not own this block call's input/source/position context. | New executor owns injected resolvers and per-call route policy, rather than a second Agent loop or provider client. It is a justified extension. Resolver/error/render helpers are relocated for eventual deletion of the old judge; old scenario consumers retain an adapter. |
| File → tenant storage → extract/native media/workspace/citation | Existing `verification/eval.rs::TenantMedia` verifies tenant-qualified URI and bounded reads. New `TenantEvidence` reuses `StorageHandle::object_len/get_object_bounded` and `tenant_path`; it adds the approved digest namespace, declared-length/digest/format checks and source slots. | New evidence vocabulary and extraction requirements justify this owner; it does not copy a transport or new registry. The existing Artifact inventory/storage workflow was not extended, leaving both Artifact entry paths unsupported (BEH-01). |
| Tool declaration → executable → authorization → domain read | `ToolRegistry`, `AgentTool`, and the existing Skald loop are used. Card tools call `get_card_for/get_card_by_ref_for/list_cards_for`; Bifrost tools call the existing service and `BoundedQuery`. Shared MCP descriptors provide immutable schemas without loopback RPC. | Per-session `Invocation` retains accepted Caller and workspace. That lifecycle is earned, but its resource counters are scoped/charged incorrectly (BEH-03/04). No replacement Card/query implementation was found. |
| Evidence bytes → format output → located units → read/search/citation | One `Extraction` path is reused by actions, workspaces, and citation validation. Existing storage/media has no equivalent Office parser or located workspace owner. `lopdf`, `calamine`, `csv`, `quick-xml`, and archive libraries are installed narrowly in the server. | New parser/workspace capability is justified. Resource amplification and bounded-read completeness remain wrong (BEH-05/07). |
| Python operation → admission → worker → mounted filesystem → result | There was no existing isolated Python execution owner to extend. New pool uses installed Monty pool/protocol/mount capability machinery, with a dedicated excluded worker dependency cone. | Worker session isolation, process ceiling, and deadline handling have real component proof. A sampled watchdog is not the approved hard group boundary (BEH-02); parent print handling has an ordinary Unicode panic (BEH-06). |

Caller-to-result paths traced include Agent action/check executor → resolve/bind/route → `run_prompt` → `RecordedTool` → hosted `Invocation` → domain owner or sandbox; extract action → tenant read → blocking extraction → locked JSON; workspace open → pinned bytes → archive guards/materialization → read/search; code action/check → `CodeExecutor::run` → sandbox entrypoint → JSON/Verdict; and verdict → reachable input slots → `TenantEvidence::cited` → canonical locator validation. Sibling action/check, native-media, workspace and citation consumers of the evidence owner were included. Monty 1.1.0's installed mount implementation was inspected to establish where write counters live.

## Acceptance matrix

PASS means the scoped implementation is supported by source and the available focused proof; it does not assert TASK-005 integration is already complete. Deferred whole-change SDK/server journeys and startup wiring are not independent TASK-003 defects.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002: pinned named evidence supplied through task-1 seams | `TenantEvidence` holds slot declarations, typed tenant, digest path and bounds; workspace consumes `CallInput` | Hosted evidence tests in `hosted-check.log` | PASS for uploaded files; Artifact portion FAIL (BEH-01) |
| REQ-004: extract nine supported formats into locked discriminated shape, no duplicate counts/images | `Extraction::run`, format methods and `units`; third-party PDF/XLSX/CSV readers | Nine extractor cases passed in hosted log; fixtures cover sparse/error/formula/date cells and ordered/merged DOCX | PASS for ordinary fixtures; bounded malformed DOCX FAIL (BEH-05) |
| REQ-004: inline and exact Artifact Python source, JSON output | `CodeExecutor::run` supports only `CodeSource::Inline` | Real-worker entrypoint JSON proof passes; no Artifact proof | FAIL, BEH-01 |
| REQ-005: tool-less/workspace-less Agent check returns closed Verdict | `AgentExecutor` check dispatch refuses tools/workspace, deserializes `Verdict` | Nine Agent cases pass in `agent-check.log` | PASS |
| REQ-007: Prompt-derived inputs/schema, exact route replacement, existing gateway/native behavior | `AgentExecutor::resolve/bind/route/invoke`; `Agent::run_prompt` | Agent binding/schema/route/position cases pass; existing judge callers updated | PASS for exercised seams; full registered/gateway proof remains incomplete (BEH-08) |
| REQ-008: all ten declared built-ins resolve before model call, only selected tools attach | `BuiltinTool` name conversion, `HostedTools::open`, `ToolSession::resolve` | One filesystem double through Agent loop; hosted catalog construction/direct dispatch checks | Implementation present; required combined/complete proof FAIL (BEH-08) |
| REQ-008 / INV-003: tool permission before dispatch, independent domain authorization, original authority | `audit::authorize(tool_invoke)` precedes match; cloned Caller passed to existing Card/Bifrost owners; unknown authority fields refused | Hosted grant/forged-argument cases pass; no actual query scope/delegation/restart suite authored here | Source path PASS; required proof FAIL (BEH-08) |
| REQ-008: inputs read-only, writable isolated work retained within invocation | Fresh `TempDir`; read-only `/inputs` and read-write `/work` MountSpec | Workspace and actual mounted-worker cases pass | PASS for isolation; cumulative budget FAIL (BEH-03) |
| REQ-008: bounded readable/searchable page/slide/line/JSON-pointer locations | Shared `units`; read/search serve bounded values | Hosted workspace read/search ordinary fixtures pass | FAIL for oversized unit continuation, BEH-07 |
| REQ-009 / INV-006: source-bound citation validation; uncited Agent pass becomes inconclusive | `AgentExecutor` builds reachable slots, filters citations and downgrades uncited pass; `TenantEvidence::cited` resolves canonical units | Separate real-validator and scripted-check cases pass | Scoped implementation PASS; required preparation→judge propagation proof FAIL (BEH-08) |
| REQ-015 scoped provenance: Agent/Prompt/route, code digest/runtime, extractor identity, tool arguments/digest/bytes | `CallProvenance::{Agent,Code,Extract}`, `RecordedTool` and source-bearing `CallInput` | Agent provenance/tool-record assertions and hosted extractor proof pass | PASS for inline/File paths; Artifact identity unavailable with BEH-01 |
| REQ-010: length/digest/format refusal and safe legacy/native behavior | `TenantEvidence::read/media`, `sniffs_as`, extractor legacy refusal | Hosted mismatched/tampered/native cases pass | PASS for File; Artifact FAIL (BEH-01) |
| INV-001: reusable runtime in Skald/Vala; host policy/parsers in server; no foundation/client parser deps | Vala Agent executor calls Skald; hosted owner holds storage and sandbox; manifests | Diff/dependency inspection; root reports check:deps passing | PASS |
| INV-005: no general runtime, network/host file/credential escape | Worker namespaces/Landlock/no_new_privs; parent handles only mounts and refuses callbacks; fresh process checkout | Five real-worker cases pass in sandbox log, including host-path refusal and state separation | PASS within inspected host support; memory/output boundaries FAIL below |
| INV-007: archive traversal/link/bomb rejection; per-call cleanup | ZIP/TAR guards and relative-path validation; TempDir and held semaphore | Four workspace cases pass, including cleanup/drop | PASS for exercised cases; no cancellation while blocking extraction proof (BEH-08) |
| Deployment: 2 warm/8 max, bounded 16 waiters, operation deadline includes checkout | `Sandbox::start/exec`, pool configuration/admission and `timeout_at` | Actual worker saturation, timeout, recovery cases pass | PASS for admission/deadline; min-warm replenishment not proven |
| Deployment: 256 MiB operation / 384 MiB process / hard 2 GiB group | ResourceLimits, `prlimit`, RSS watchdog | Allocator breach and watchdog-kill case pass; hard group boundary is absent | FAIL, BEH-02 |
| Deployment: cumulative Python budget, no reset/overbooking across tool calls | Invocation counter read before operation and charged after operation | Only initially zero budget is tested | FAIL, BEH-04 |
| INV-007: output flood safe and bounded | Sandbox print callback; `RecordedTool` result length check | ASCII print bound passes | FAIL on non-ASCII boundary, BEH-06 |
| Standalone components, explicit dependencies, no task-2 driver or live credentials required | Executors expose resolver/evidence/gateway/toolset seams; sandbox independent of AppState | Agent and worker suites standalone; hosted protected-owner tests use managed Postgres | PASS |
| Scope exclusions: no endpoints, queues, schedules, Operators, SDK or graph implementation | Complete changed-file list and cumulative diff | Static inspection | PASS |
| AC-003/004/006: ordered negative/edge/protected-owner proof authored with behavior | Existing tests cover ordinary extraction, route, direct tools, worker containment | Required scenarios absent from source despite current tests green | FAIL, BEH-08 |
| AC-010: focused Rust checks, format/lint/dependency checks | Scoped mise targets and separate worker build/clippy lane | Root's raw focused logs: 9 Agent + 21 hosted + 5 actual-worker pass; root reports checks passing | Focused existing tests PASS; missing acceptance proof still FAIL |

## Material proposed findings

### BEH-01 — MISSING: Artifact inputs and Artifact code sources always fail

**Obligation:** REQ-004 permits exact Artifact code; REQ-010 permits existing registered Artifact evidence. TASK-003 supplies real code/evidence capabilities to TASK-005, not placeholder refusal branches.

**Locations:** `hosted/sandbox.rs:356` and `hosted/evidence.rs:115` (File-only destructure in `read`).

**Producer → consumer:** task-1 contracts explicitly represent `CodeSource::Artifact { card, path }` and `InputDeclaration::Artifact { artifact, path }`. Both code action and code check share `CodeExecutor::run`, which unconditionally rejects non-inline source. Extract actions, native media, workspace materialization and citation location all use `TenantEvidence::read`, which rejects every Artifact declaration. Passing an accepted exact Artifact therefore cannot reach its sandbox or parser; citation lookup silently drops its citations after the terminal read refusal.

**Correction:** extend the existing tenant evidence/source reading boundary with the accepted caller's authorized exact Artifact inventory/path resolution and bounded storage read. Reuse existing Card inventory/registry, `wyrd_storage::service` download authorization/path mechanisms, `tenant_path` and `StorageHandle`; do not add a transport, host path, or second artifact registry. Feed the resulting immutable bytes through the same validation/parser or code executor used for inline/uploaded content. Preserve authorized exact identity/provenance. Add focused Artifact-code and Artifact-media success plus wrong tenant/permission/path/kind refusal proof across the shared consumers. Constructor wiring alone in TASK-005 cannot make the current unconditional refusal branches work.

### BEH-02 — VIOLATION: worker-group memory is sampled, not a hard ceiling

**Obligation:** Deployment limits require an independent hard combined memory ceiling, count idle workers, and fail safely without taking down the parent. Task scenario 5 explicitly requires OS enforcement and stops on unsupported containment.

**Location:** `hosted/sandbox.rs:283–312` (`watch`), configured by `Sandbox::start`.

**Path and consequence:** up to eight workers can independently allocate under their process/allocator ceilings. The only aggregate control reads RSS once per 100 ms and kills one busy worker after the sum already exceeded the limit. The source explicitly admits overshoot. No group allocation barrier exists; short bursts can complete between ticks, and allocation can exhaust a container hosting the parent before the next sample. If only idle workers exceed a configured group cap, no busy victim exists at all. The existing one-byte-limit test proves eventual killing of a busy loop, not the promised hard bound.

**Correction:** enforce the approved worker-only group with an actual OS aggregate limit (the conventional Linux memory cgroup mechanism) including idle, newly spawned and replacement processes, while retaining the independent per-process limit and safe operation failure. Refuse runtime initialization if the required isolation cannot be established; do not silently relax the contract to a watchdog. Prove aggregate memory refusal and parent survival/recovery with ordinary bounded worker concurrency. Do not add a scheduler or reserve workers during model calls.

### BEH-03 — INCORRECT: writable-work accounting resets on every code tool call

**Obligation:** REQ-008 / Deployment: cumulative `/work` writes are at most 50 MiB per invocation, and total scratch is at most 250 MiB.

**Location:** `hosted/tools.rs:260–268`; `hosted/workspace.rs:190–209` charges scratch only during initial placement.

**Producer → sink:** every `Invocation::run` creates a fresh MountSpec with the entire work budget. Installed monty-pool 1.1.0 converts each spec to a fresh `monty_fs::Mount::with_root`, whose `write_bytes_used` starts at zero. The same persistent host work directory survives these operations. Two sequential calls writing 30 MiB to distinct files each pass a 50 MiB limit and leave 60 MiB. Additional calls keep adding files with no invocation scratch check; overwrites can exceed the cumulative-write allowance without increasing disk size. `Workspace::work_units` checking one file's size does not protect either invariant and is not in the write path.

**Correction:** retain and atomically charge the invocation's cumulative write allowance at the shared parent-serviced mounted-write boundary, also accounting for total scratch retained with inputs. Reuse Monty's mount accounting/capability boundary, rather than adding checks to individual read consumers. Include sequential and concurrent multi-operation writes, overwrite, and near-full input scratch proof using the real mount path. Keep inputs read-only and preserve within-invocation work persistence.

### BEH-04 — INCORRECT: concurrent Python tools spend the same remaining time budget

**Obligation:** Deployment requires cumulative Python-operation wall time per Agent invocation, not independent full allowances for concurrent calls.

**Location:** `hosted/tools.rs:251–254,269–283`.

**Path:** the existing Skald loop dispatches tools with `buffer_unordered` (`skald-agent/src/loop_runtime.rs:661`, normal cap 8). Each `code.run` reads the same remaining Duration, releases the mutex, runs with that whole remaining deadline, then subtracts elapsed time only after completion. Eight calls can each use nearly ten seconds while sharing a sixty-second allowance, consuming about eighty operation-seconds before any debit prevents work. A smaller configured budget makes the gap easy to exercise without a long test. Saturating subtraction merely hides the overspend afterwards.

**Correction:** make budget admission/accounting atomic for concurrent Python operations within the existing invocation owner; a Python-only serialization/permit is a sufficient minimal option if it preserves the intended cumulative semantics. Do not serialize unrelated tools/model operations or weaken the shared pool. Prove that concurrent calls cannot together run beyond a small configured cumulative allowance, and that subsequent calls receive only the actual remainder.

### BEH-05 — VIOLATION: a tiny DOCX can allocate unbounded parent memory through gridSpan

**Obligation:** INV-007 and task scenarios 2/5 require bounded extraction/resources and isolation of failures from the parent server.

**Location:** `hosted/extract.rs:320–331`.

**Producer → consumer:** a tenant-supplied DOCX XML `w:gridSpan w:val` parses directly as `usize`; `cells.extend(repeat_n(String::new(), span - 1))` allocates that many String entries. Neither XML/archive bytes nor `text_bytes` bounds this derived expansion, because covered cells are empty. A small valid ZIP below all byte/ratio/file limits can request hundreds of millions of covered positions (or an enormous usize), consuming gigabytes or triggering capacity/allocation failure in the parent blocking task. This path is shared by extract actions, filesystem preparation and citation extraction. It is outside the worker memory boundary.

**Correction:** validate the source span against the actual supported DOCX table/grid structure and the existing extraction resource budget before expansion; reject an unreasonable/malformed span safely rather than allocating it. Do not add another parser or independent service. Add a small malicious-span fixture that proves prompt refusal without allocating the claimed size, while retaining normal merged-cell output semantics. Apply the same source-to-derived-size check wherever this parser expands caller-controlled structure.

### BEH-06 — INCORRECT: bounded print collection panics on a UTF-8 boundary

**Obligation:** task scenario 4/5 and INV-007 require safe bounded output and worker failures contained to the operation.

**Location:** `hosted/sandbox.rs:194–197`.

**Path:** the print callback runs in the parent and slices `text[..min(text.len(), remaining)]` at an arbitrary byte offset. With a 64-byte output limit, printing 63 ASCII bytes followed by `é` tries to take one byte of a two-byte scalar and panics; the default 64 KiB limit is equally reachable. Both `Sandbox::run` (code actions/checks) and `Sandbox::exec` (`code.run`) share this path. The ASCII flood test cannot detect it.

**Correction:** truncate at a UTF-8 boundary before appending, reusing the already-existing safe truncation approach in `hosted/workspace.rs::truncate` or its minimal equivalent. Add a real-worker print case crossing the byte boundary with multibyte characters; require bounded valid UTF-8 and successful subsequent operation. Do not catch a panic as the normal output protocol.

### BEH-07 — INCORRECT: fs.read permanently drops the tail of a large page/line

**Obligation:** REQ-008 requires bounded reads; the advertised `fs.read` contract says follow `next` to continue. INV-006 prohibits quietly treating absent evidence as complete.

**Location:** `hosted/workspace.rs:432–446` and `hosted/tools.rs` filesystem descriptor.

**Path:** for a first unit larger than the byte budget, `read` emits only its truncated prefix, then advances to the next unit. If the file contains one long line, one large PDF page, one large PPTX slide, or one long JSON leaf, the result has `next: null`. With more units the cursor skips the truncated unit's remainder. These files can be well within 20 MiB input/10 MiB extraction limits. No available `start` value retrieves the omitted bytes, so preparation may omit exactly the condition being checked while appearing to have finished reading.

**Correction:** preserve bounded access to every part of an oversized located unit using the private filesystem read continuation contract, keeping original locator attribution. If complete continuation cannot be represented, refuse explicitly instead of reporting apparent EOF. Add a focused single-unit-over-budget case whose relevant text is in the tail, and prove bounded continuation or honest explicit refusal; retain normal page/slide/line semantics.

### BEH-08 — MISSING: required integrated and negative proof has not been authored

**Obligation:** TASK-003 scenarios 2/3/5/6 explicitly require focused cases plus authoring protected-owner and queued retry/restart regressions now; only their full execution is deferred to the merged pass. AC-003/004/006 require these boundaries, rather than green leaf-only substitutes.

**Locations:** `vala-eval/tests/integration/blocks_agent.rs`, server `hosted/{extract,workspace,tools}/tests.rs`, server `tests/integration/sandbox_worker.rs` and integration target registrations.

**Evidence:** the Agent suite uses inline Agents plus `NoRegistry`; its toolset supplies only FixedRead, and its citation reader double filters directly. It does not exercise a registered Agent declaring `bifrost.query` and `code.run`, two actual preparation/judge invocations with ordinary citation-carrying output, or concrete hosted tool/evidence composition. Hosted tools cover direct grants and Card/list refusal, with a never-spawning sandbox; they do not author real Oracle multi-table/schema scoped denial, narrowed delegation, distinct-tenant interleaving, or queued authority retry/restart tests. The worker suite has five cases, covering fresh state, basic mounts, allocator/time/watchdog and checkout capacity; it does not exercise cancellation during parent-serviced filesystem work, independent process-memory enforcement, or cumulative multi-call work/time budgets. The workspace drop test proves dropping an already-completed workspace, rather than cancellation during materialization. The new source contains no corresponding integrated/regression target for these omitted paths.

**Consequence:** the handoff leaves no executable proof of the critical cross-owner contracts and misses the concrete counter/reset issues above. Deferring suite execution does not defer authoring these tests.

**Correction:** add the smallest existing-harness regressions that cover the explicitly required paths, reusing scripted providers, existing resolver/gateway seams, real protected owners, `WyrdTestServer` and managed Postgres where required. Do not create another engine/harness or run all service journeys during this task. Run focused component/seam cases now and record the real protected-owner/retry/restart selectors for the merged pass. Consolidate new focused cases for BEH-01–07 with this closure rather than duplicating tests per symptom.

## Verification limits and verdict

Read root-provided raw `agent-check.log`, `hosted-check.log` and `sandbox-check.log`: existing focused tests pass (9, 21 and 5 respectively). No full service suite, SDK journey, live provider, adversarial OOM or synthetic load was run by this reviewer. Source establishes the deterministic refusal/accounting/slicing/continuation gaps; no host-destabilizing reproduction was needed. Startup environment loading, background capacity integration, worker packaging and full SDK/server composition remain TASK-005 work and are not treated as already delivered.

The seven concrete implementation defects plus missing required boundary proof prevent accepting the cumulative candidate. No optional features, compatibility surface, plugin framework, replacement loop, or speculative hardening is requested.
