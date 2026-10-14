# System-resilience review

## Immutable subject

- Repository: detached worktree at the review root.
- Base: `ef03f624a1f3a6bcd51bbd17060271f0b5e8cabf`.
- Candidate: `c4ba2b0502f8968a78e5e960bcc9eba43e960643`.
- Authority: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
  `architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`,
  approved `changes/active/example-journey-fixes/spec.md` revision 4, and
  binding owner decisions OD-1 through OD-7.
- Reviewed range: the complete `base..candidate` diff, including production
  owners, migrations, generated contracts, SDK projections, fixtures, and the
  recorded verification evidence in TASK-001 through TASK-005. TASK-005 was
  evaluated only in its OD-5 replacement form.

The candidate commit remained unchanged throughout this review. A transient
formatter-only working-tree change produced by the review orchestrator was
identified, attributed, and restored before this report was written; `HEAD`
remained the candidate commit and the source used for the conclusions below is
the candidate source.

## Deployment and process topology

| Changed path | Deployed path and affected capability | Failure and recovery assessment | Evidence |
|---|---|---|---|
| Typed Jev direct, unified, and judge calls | SDK or Verifier -> Wyrd server pod -> gateway admission/routing -> typed Jev provider client -> TypeSafe SystemOne | Request validation happens before provider IO. Provider 400/422 and 401/403 are terminal; 408/429/5xx use the existing bounded provider retry policy. A provider outage or malformed answer fails only the model call/Verifier attempt; it does not crash the shared server or alter another provider. Dropping a request can leave an already accepted upstream call in progress, the existing gateway cancellation boundary. The next request or queued Verifier retry is the recovery path. | Typed dispatch is owned by `crates/skald/skald-providers/src/clients/jev.rs:114`; the non-streaming refusal is explicit at `crates/skald/skald-providers/src/clients/jev.rs:146`; retry classification remains the shared policy. TASK-001 records native/unified gateway journeys, judge pass/fail journeys, and 400/401/403/429/5xx coverage in Rust, Python, and TypeScript. |
| Bifrost planning errors and live subqueries | HTTP/MCP/SDK -> Gate -> Oracle leader -> local or remote Scribe live fragments, optionally unioned with published Iceberg data | Caller SQL errors stop before execution and remain request-local. Internal/resource failures keep their existing 5xx classification. Scalar-subquery predicates remain at the leader instead of being serialized into a Scribe fragment, so a remote or restarted Scribe never has to decode a leader-owned `ScalarSubqueryExec`. Existing query deadlines, cancellation, admission release, and fragment teardown remain unchanged. A Scribe or Oracle loss therefore has the existing bounded query consequence, not process-wide failure or retry amplification. | The leaf exclusion is at `crates/vala/vala-bifrost-redux/src/oracle/live.rs:515`; wrapped caller-fault classification is at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3746` and safe public mapping at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3771`. TASK-002 records exact comparisons against plain DataFusion in published, live, mixed, and remote-Scribe states, including nulls, ties, duplicates, and orphans, plus SDK and MCP error journeys. |
| MCP catalog compatibility and traced tool calls | Official MCP/OpenAI clients -> public HTTP edge -> `rmcp` stateless transport -> per-request spawned task -> Wyrd service owner | The catalog remains stateless and per caller; no session affinity or new durable state was introduced. Shutdown cancellation is wired into the transport and an MCP task-tracker token covers dispatch, so rolling replacement stops admission, cancels work, and waits for cleanup. Because `rmcp` changes task, the handler reparses only the original trusted HTTP parts to restore trace context. Framework incompatibility is confined to discovery/call failure and cannot take down other HTTP capabilities. | Trace rescoping is at `crates/wyrd/wyrd-server/src/mcp/mod.rs:245`; tracked dispatch at `crates/wyrd/wyrd-server/src/mcp/mod.rs:283`; process cancellation at `crates/wyrd/wyrd-server/src/mcp/mod.rs:406`. TASK-003 records the official TypeScript MCP SDK and OpenAI Agents SDK connecting, converting the full admin catalog, and calling a read tool; existing connectivity journeys cover request and shutdown cancellation. |
| Gateway and Verifier attribution | HTTP/MCP/direct/scheduled/observation execution -> verification queue/engine -> in-process gateway -> Scribe outbox plus `wyrd.verifier_runs` | `run_id` and `card_uid` remain optional independent context; they do not become admission dependencies. Manual/direct actors come from the authenticated caller; internal queue inserts resolve SYSTEM in the same tenant transaction. Queue state and requester identity survive process restart in Postgres. Gateway/result evidence remains on the documented non-blocking Scribe outbox: abrupt process loss can lose unacknowledged evidence, but cannot refuse or roll back the originating operation, matching the repository durability authority. Observation trace linkage is a span link and does not change the execution parent or authorization. | Independent header decoding is at `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:263`; direct execution attribution enters at `crates/wyrd/wyrd-server/src/components/verification/service.rs:417`; scheduled/manual and observation inserts are centralized at `crates/wyrd/wyrd-sql/src/queries/verifier_runs.rs:87` and `:159`; observation span linking is at `crates/wyrd/wyrd-server/src/verification/eval.rs:184`; judge context is composed at `crates/wyrd/wyrd-server/src/verification/eval.rs:339`. TASK-004 records direct, scheduled, observation, no-Run, independent-header, result/judge join, restart-safe SQL, and trace-link proof. |
| W3C trace propagation | Rust/Python/TypeScript SDK request -> HTTP edge task-local context -> gateway attempt capture or in-process judge; MCP explicitly re-enters the context in its spawned task | Malformed or repeated `traceparent` is ignored, never an availability failure. A valid context affects only observability and is not read by authorization. Foreign-runtime task-local context is bounded to the SDK operation; ordinary Rust spans remain the fallback. Captured gateway attempt spans reuse the caller trace while durable operation state is unaffected if trace export or Scribe delivery is unavailable. No background task is assumed to inherit task-local state implicitly. | Foreign-runtime scoping is at `crates/shared/wyrd-client/src/transport/trace.rs:35`; common authenticated HTTP injection at `crates/shared/wyrd-client/src/transport/http.rs:884`; server parsing/scope at `crates/wyrd/wyrd-server/src/http/middleware/trace_context.rs:31`; gateway span projection at `crates/wyrd/wyrd-server/src/components/gateway/capture.rs:365`. TASK-004 records malformed-header, HTTP gateway, traced verify, and MCP spawned-task journeys. |
| Managed-secret deployment sealing keyring | CLI local setup -> durable install-root key file -> child server environment -> boot rewrap -> gateway administration/resolution -> upstream provider | Local setup stages and fsyncs an owner-only random key, installs it without replacement, and reuses the install-root file across process restarts and release activation. Cancellation before rename leaves no partial destination; concurrent first runs converge via no-clobber. Server boot uses the same deployment keyring as other sealed secrets. Rewrap is compare-and-swap across replicas: a concurrent write or lost race remains for a later boot, and retained keys continue to open old envelopes during rollout. A keyless server with stored ciphertext fails readiness; with a configured key, an individual unopenable credential fails closed at resolution without taking unrelated capabilities offline. Secrets and keys are not logged or returned. | Key creation and reuse enter before boot at `crates/wyrd/wyrd-cli/src/server/dev.rs:93` and are implemented at `crates/wyrd/wyrd-cli/src/server/dev.rs:541`; binding-aware seal/open are at `crates/wyrd/wyrd-gateway/src/managed.rs:99` and `:125`; boot rewrap is at `crates/wyrd/wyrd-auth/src/sealing.rs:82`; the gateway table joins the existing rewrap owner at `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:433`. TASK-005 records CLI first boot/rerun, cross-tenant refusal, server restart, K2+K1 rollout, post-rewrap K2-only restart, and keyless-boot proof. |

## Owner-decision resilience check

| Decision | Result | System assessment |
|---|---|---|
| OD-1 actor and origin | PASS | Internal queue work resolves SYSTEM transactionally; observation origin remains references and a span link. The documented null-SYSTEM case and open judge-authority question are not contradicted. |
| OD-2 optional gateway context | PASS | Lone principal, lone Run, lone Card, and both remain representable; loss or absence of correlation does not disable the standalone gateway. |
| OD-3 trace context | PASS | Context is observability-only and malformed input is ignored. Spawned MCP work explicitly restores context without using it for identity. |
| OD-4 Jev r2 | PASS | Both routes share typed shapes and bounded provider behavior; Jev cannot silently enter the chat Agent loop. |
| OD-5 deployment sealing keyring | PASS | The rejected per-tenant keyring and restart choreography are absent; local boot provisions the existing deployment mechanism. |
| OD-6 analytical correctness | PASS | Recorded proof covers published, live, mixed, and remote-Scribe states against a plain DataFusion oracle and hand-counted anchors. |
| OD-7 root causes and complexity | PASS | The runtime corrections sit at the existing provider, Oracle leaf/error mapper, MCP schema/dispatch, attribution, trace, and sealing owners. No second lifecycle or availability mechanism was introduced. |

## Recovery and proof assessment

- Process restart and rolling replacement: durable verification queue state and
  managed-secret envelopes survive; sealing rotation converges with retained
  keys and compare-and-swap. MCP uses the process shutdown token and tracked
  tasks. Gateway evidence retains the repository's explicitly accepted
  in-memory-outbox loss boundary rather than inventing a WAL.
- Dependency outage: upstream Jev failures remain bounded to provider attempts;
  Postgres unavailability blocks the durable operation that needs it; Oracle or
  Scribe failures remain query-scoped and use existing deadlines/cancellation;
  unavailable sealing material refuses managed-secret use without exposing the
  secret.
- Delay and cancellation: existing gateway/provider deadlines and Bifrost
  query deadlines remain the owners. MCP request cancellation and server
  shutdown share the transport cancellation path. No new unbounded retry loop,
  detached task, or health-check amplification was found in the diff.
- State survival: Postgres owns queue/requester state; the sealing key file and
  sealed envelopes own managed-secret restart survival; trace context is
  intentionally ephemeral; Scribe outbox delivery retains its documented
  at-most-process-lifetime risk.

The task evidence is proportionate and includes the failure/recovery paths that
changed. It records full Bifrost state-matrix coverage, official third-party MCP
clients, gateway/Verifier attribution across origins, malformed trace input,
CLI restart, cross-tenant secret substitution, rotation, and keyless boot. I
also ran `git diff --check base candidate`, which passed.

Independent focused test reruns were attempted for the Bifrost error mapper and
managed-secret rotation. They did not start because this sandbox prevents mise
from writing its trust-state symlink under the user's read-only home. This is a
review-environment limitation, not a test failure. I therefore relied on the
exact commands and passing results recorded in the immutable task evidence and
on source tracing. I did not rerun Postgres, peer-cluster, Python, TypeScript,
or full aggregate lanes.

## Material findings

None. I found no reachable changed failure or recovery path that violates the
approved behavior, weakens tenant or secret isolation, expands an outage beyond
its existing boundary, loses newly required durable state, or introduces
unbounded retry/cancellation work.

## Overall result

**PASS**
