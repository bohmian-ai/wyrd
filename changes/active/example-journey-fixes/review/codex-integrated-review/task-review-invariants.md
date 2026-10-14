# Invariant review: integrated `example-journey-fixes`

## Immutable subject

- Repository: current detached worktree
- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`
- Approved specification: `changes/active/example-journey-fixes/spec.md`, revision 4
- Binding decisions: `changes/active/example-journey-fixes/owner-decisions.md`, OD-1 through OD-7
- Tasks: `TASK-001` through `TASK-005`; `TASK-005` was reviewed in its OD-5 replacement form

The candidate commit remained `c4ba2b0502f8968a78e5e960bcc9eba43e960643` during this review. The pre-existing/generated working-tree change to `sdks/wyrd-sdk-python/python/wyrd/_card_types.py` was not used as candidate evidence and was not modified by this reviewer.

## Navigation map and invariant paths

| Changed boundary | Producer | Durable or observable sink | Sibling consumers / proof |
|---|---|---|---|
| Jev typed dialect | `skald-spec` `JevRequest`/`JevResponse`, Prompt rendering | Skald provider client, gateway native and unified adapters, Vala judge structured output | Rust/Python/TypeScript gateway journeys, generated schemas/types, Agent-loop refusal |
| Bifrost planning errors | DataFusion logical-planning errors | `BifrostError::QueryInvalidSql` over HTTP/SDK/MCP | authorization-before-planning, internal-error mapping, unauthorized-table negative journeys |
| Bifrost live subqueries | DataFusion pushed physical filters | local/peer Scribe live fragments and leader `FilterExec` | DataFusion oracle workload across published/live/mixed/remote states |
| MCP schemas | server tool catalog and `CreateArgs` | MCP `tools/list`, official SDK conversion, OpenAI Agents conversion | full administrator catalog and read-tool call |
| Verification actor/origin | caller, SYSTEM principal, observation record, subject/run | `wyrd.verifier_runs`, result Arrow row, gateway call row, span link | manual/direct/scheduled/observation writers and claim/result consumers |
| Trace context | inbound `traceparent`/`tracestate`; additionally, candidate-created outbound SDK context | server task-local context, gateway span parentage | gateway HTTP, direct verify, MCP task handoff, Rust/Python/Node SDK transports |
| Managed secrets | admin-submitted secret plus tenant/name/provider binding | keyring envelope in `wyrd.gateway_provider_credentials` | resolver open, boot rewrap, rotation, CLI-created sealing-key file |

No second graph store, queue, durable identity resolver, or sealing mechanism was introduced. The material duplicated/extra path is the outbound client trace injector described in `INV-REV-001`.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001: typed Jev request/response, Prompt, provider, gateway, SDK types, and judge | `crates/skald/skald-spec/src/wire/jev.rs`; `request.rs`; `response.rs`; `crates/skald/skald-providers/src/clients/jev.rs`; gateway adapter; `vala-eval/src/orchestrator/judge.rs` | TASK-001 records wire/provider/adapter tests, Rust/Python/TypeScript journeys, codegen, and judge integration | **FAIL** — the unified route adds forbidden per-field validation (`INV-REV-002`) |
| REQ-002: actionable 4xx planner failures without unauthorized-schema disclosure | `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3728-3794`; table authorization remains before logical planning | TASK-002 records unit plus Rust/Python/TypeScript/MCP negative and shape journeys | PASS |
| REQ-003: common MCP tool-schema compatibility and full catalog conversion | `crates/wyrd/wyrd-server/src/mcp/operators.rs`; `mcp/principals.rs`; full catalog descriptors | TASK-003 records full-catalog Rust checks and official MCP/OpenAI Agents TypeScript journey | PASS |
| REQ-004: independent run/Card correlation, correct actors, subject joins, and observation span links | `gateway/invocation.rs:94-134`; `verification/eval.rs:183-185,253-263`; `wyrd-sql/src/queries/verifier_runs.rs:87-188`; `verification/results.rs:141-180` | TASK-004 records SQL, direct/queued, Rust/Python/TypeScript, MCP, and Bifrost result/call join evidence | PASS |
| REQ-005: inbound W3C context parents gateway/in-process work; malformed input ignored | `http/middleware/trace_context.rs:28-70`; `http/router.rs:140-146`; `gateway/invocation.rs:227-231,489` | Explicit-header gateway and direct-verify journeys plus malformed-header tests are recorded | PASS for required server behavior; OD-3 still fails because extra outbound client injection was added |
| REQ-006: scalar, IN, and EXISTS subqueries over live tables | `oracle/live.rs:373-438` keeps scalar-subquery filters at the leader | TASK-002 records analytical comparison against plain DataFusion in published/live/mixed/remote states | PASS |
| REQ-007: deployment sealing keyring, binding, rewrap, local first-boot key, no compatibility path | `wyrd-gateway/src/managed.rs:88-145`; `credential.rs:106-164`; sealed-table query owner; migration; CLI `dev.rs:541-573` | TASK-005 records binding, tenant, restart, rotation, keyless-boot, CLI journey, docs, and checks | PASS |
| INV-001: tenant isolation | SQL RLS/capability owners remain; managed payload binds tenant; Bifrost authorizes resolved tables before planning | Recorded cross-tenant gateway/secret/Bifrost journeys and `check:tenant-isolation` | PASS |
| INV-002: provider credentials and keys never logged or returned | `SealedPayload` custom redacted `Debug`; keyring IDs only; credential refusal scrubber retained | Recorded sentinel/output tests and CLI journey | PASS |
| INV-003: audit remains non-blocking | No audit persistence or authorization-order change in the cumulative diff; existing Scribe outbox path remains | Diff/source audit; task lanes exercise the affected server paths | PASS |
| INV-004: no old Jev shim/alias; only typed unified pass-through | Typed `ChatQuestions` is the only Jev chat input; Agent loop refuses Jev | TASK-001 route/fallback/session tests | PASS, apart from the separate OD-4 validation violation |
| AC-001: user journeys for each shipped surface | Task records identify Rust/Python/TypeScript, MCP/HTTP, and CLI journeys for the applicable requirements | Recorded task commands/results | PASS |
| AC-002: specified negative flows | Jev mapping, Bifrost authorization-before-planning, malformed context parsing | Recorded unit and journey evidence | PASS |
| AC-003: format, lints, codegen, owning lanes | No gate was weakened in the diff | All five task records report the applicable commands green; local rerun was unavailable as noted below | PASS on recorded evidence |
| AC-004: unmodified `bohmian-ai/wyrd-example-ts` runs clean | No implementation or evidence location in this repository can stand in for executing that external example | Repository search finds only the obligation in `spec.md:192-194`; no task evidence records the run | **FAIL** (`INV-REV-003`) |
| Non-goals: no DeepSeek/other provider, new Verifier semantics, or release | Diff is limited to the seven requested defect families and their proof/docs | Full `base..candidate` name/status and source diff | PASS |
| OD-1: caller for manual/direct; SYSTEM for internal; origin by references; observation span link; judge authority remains open | `verifier_runs.rs:87-188`; `verification/eval.rs:183-185,204-225`; result mapping uses frozen requester | TASK-004 SQL and continuous-eval evidence | PASS |
| OD-2: optional independent gateway context; no binding enforcement | `GatewayCallSubject` has two independent `Option`s; requested attribution authorizes only a present Card | Four-case gateway journey and unchanged direct authorization | PASS |
| OD-3: no further client-side injection; server consumes inbound context only for observability | Server inbound handling exists, but client transport and foreign SDK boundaries inject new outbound context | Source itself and new traced-SDK journeys prove the forbidden behavior is reachable | **FAIL** (`INV-REV-001`) |
| OD-4: both Jev routes; unified typed pass-through with no schema mapping or per-field validation; scrubbed 401; Agent typed refusal | Both routes, typed answer mapping, refusal path, and Agent refusal exist; unified route calls custom field validator | TASK-001 proves the route and also the extra validation | **FAIL** (`INV-REV-002`) |
| OD-5: deployment keyring replaces rejected gateway keyring; no restart/stale code | Shared `SealingKeyring`, `SealedSecretTable`, migration, and CLI env handoff | TASK-005 source search and journeys | PASS |
| OD-6: representative oracle dataset, hand counts, four states | `wyrd-testing/src/bifrost/support_workload.rs` and analytical journeys | TASK-002 records plain-DataFusion and hand-count comparisons in every required state | PASS |
| OD-7: root causes, no unrequested complexity, one independent integrated review | Bifrost fix is at filter pushdown; sealing extends shared rewrap; this report reviews cumulative diff | This review | **FAIL** because OD-3's explicitly cancelled client propagation remains |

## Proposed findings

### INV-REV-001 — VIOLATION: cancelled client trace propagation remains active

- **Violated obligation:** OD-3 says existing run telemetry is sufficient and “No further client-side injection is added” (`changes/active/example-journey-fixes/owner-decisions.md:50-55`). OD-7 also forbids complexity the owner did not request (`owner-decisions.md:97-100`).
- **Location:** `crates/shared/wyrd-client/src/transport/trace.rs:1-64`; `crates/shared/wyrd-client/src/transport/http.rs:876-904`; `sdks/wyrd-sdk-python/src/observe/mod.rs:141-167,393-435`; `sdks/wyrd-sdk-ts/native/src/cards.rs:666-710`.
- **Evidence and producer-to-sink trace:** the new `with_caller_trace` task-local accepts Python/Node context, while `outbound()` otherwise reads Rust's current tracing span. Every ordinary authenticated HTTP builder calls `outbound()` and emits `traceparent`/`tracestate`, not only the direct verification path. Python `observe.verify` and Node `NativeRun::verify` explicitly scope the server request with foreign-runtime context. The new Rust, Python, and TypeScript traced-SDK journeys exercise this exact path, so it is reachable production behavior rather than dormant scaffolding. The server-side inbound owner at `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:28-70` already satisfies the approved behavior whenever a request carries a header.
- **Observable consequence:** the candidate changes every Rust-client authenticated HTTP request's propagation semantics and adds Python/Node boundary APIs specifically rejected by the binding owner decision. It also makes unrelated SDK calls join ambient traces, broadening behavior beyond REQ-005's inbound server contract.
- **Required testable root correction:** remove the new outbound trace task-local/module, the shared HTTP builder's ambient-context injection, and the Python/Node verify bridges and their SDK-auto-propagation tests. Preserve the server inbound middleware, MCP task re-scope for an already-present inbound header, and explicit-header gateway/direct-HTTP journeys. Prove that an explicit valid inbound header still parents gateway/judge spans, malformed input is ignored, and ordinary SDK requests do not synthesize `traceparent` from ambient client spans.

### INV-REV-002 — VIOLATION: the unified Jev pass-through performs forbidden per-field validation

- **Violated obligation:** OD-4 requires the unified route to deserialize the typed `{state, questions}` body and explicitly says, “There is no schema mapping and no per-field validation” (`changes/active/example-journey-fixes/owner-decisions.md:67-73`).
- **Location:** `crates/wyrd/wyrd-gateway/src/adapter/jev.rs:63-95,111-120`.
- **Evidence and producer-to-sink trace:** `chat_request` correctly deserializes the last user message into `ChatQuestions`, but then constructs `JevRequest` and sends it through `checked()`. `checked()` calls `JevRequest::validate`, whose producer in `crates/skald/skald-spec/src/wire/jev.rs:168-247` adds custom field/bound checks for non-empty model/questions/instructions, choice counts/names, and score level counts. The resulting `UnsupportedRequest` is returned before provider IO. Native `/v1/systemone` is a sibling caller for which the spec separately requires typed validation; that native behavior does not justify applying the custom validator to the owner-defined unified pass-through.
- **Observable consequence:** a structurally typed Jev request in the last chat message can be rejected locally for one of Wyrd's custom bounds instead of being passed to Jev, contradicting the approved unified-route semantics and potentially rejecting a request the upstream accepts.
- **Required testable root correction:** keep typed deserialization and the single typed bad-request for malformed/unknown shapes, but do not call the custom `JevRequest::validate` path for the unified route. Retain native-route validation. Add a focused adapter test showing the unified route passes a structurally typed request that violates one custom bound through preparation, while an invalid question discriminator still fails before provider IO.

### INV-REV-003 — MISSING: AC-004 has no execution evidence

- **Violated obligation:** AC-004 requires `bohmian-ai/wyrd-example-ts` itself to run clean against the candidate without client-side escaping or a tool allow-list (`changes/active/example-journey-fixes/spec.md:192-194`).
- **Location:** the obligation is at `changes/active/example-journey-fixes/spec.md:192-194`; none of `changes/active/example-journey-fixes/tasks/TASK-001-jev-typed-provider.md` through `TASK-005-local-keyring.md` records an example-repository command, revision, or result.
- **Evidence:** a repository-wide search for `wyrd-example-ts` finds only the specification's objective and AC-004. The component journeys are valuable but do not execute the application whose failure defined the change.
- **Observable consequence:** the primary end-to-end outcome remains unproven; component suites cannot establish that package installation, the exact example configuration, and the complete agent investigation path work together without the prohibited workarounds.
- **Required testable correction:** run a pinned, clean checkout of `bohmian-ai/wyrd-example-ts` against this candidate, without patches, client-side escaping, or a tool allow-list, and preserve the exact example revision, command/configuration, and terminal result as reviewable evidence. If the unmodified example fails, correct the smallest owning product path and rerun its focused lane plus the example.

## Non-blocking notes

None. The proposed findings above have public-contract or acceptance consequences; no style-only issue is included.

## Verification assessment and limits

The task files record green focused and aggregate evidence across Rust, Python, TypeScript, MCP, Postgres, codegen, dependency, tenancy, docs, formatting, and lint lanes. Source inspection confirmed that the named production and test paths exist and that the two violations are directly exercised.

I attempted fresh focused commands through the required `mise exec --` path. The first attempt could not create mise's trust symlink under the read-only home-state directory. A second attempt isolated mise state/data/cache under `/tmp`, but mise then tried to install Rust 1.98.1; restricted network/process behavior caused rustup to abort before any test was selected. Therefore no fresh command completed in this reviewer environment. This does not erase the task-record evidence, but it leaves AC-003 dependent on those immutable records. The external `wyrd-example-ts` run is absent from those records and is separately reported.

## Overall result

**FAIL**

The candidate implements the core seven product fixes, but it does not satisfy the binding owner decisions exactly: the explicitly cancelled outbound SDK trace propagation remains, the unified Jev pass-through adds expressly forbidden per-field validation, and the primary external example acceptance run is unproven.
