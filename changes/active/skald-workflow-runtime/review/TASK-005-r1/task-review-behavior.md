# TASK-005 Behavior Review

## Review Findings

### Critical

None.

### Important

- **BEHAVIOR-001 — INCORRECT** — [`crates/wyrd/wyrd-cli/src/workflow.rs:105`](../../../../../crates/wyrd/wyrd-cli/src/workflow.rs#L105) declares `--server` with `conflicts_with = "file"`, so Clap rejects every authored-file invocation that supplies an endpoint. REQ-026 and the task's packet-local CLI contract say the existing `--server <url>` remains the connection endpoint, not an execution-mode flag; the public guide likewise says it “only selects the Wyrd endpoint.” A local file is a valid local source, and a file whose route is `wyrd_gateway` needs that endpoint for its public gateway call. The implementation instead forces the ambient config endpoint and makes the ordinary one-command override unavailable. The journey asserts the incorrect rejection at [`crates/wyrd/wyrd-cli/tests/workflow_journey.rs:242`](../../../../../crates/wyrd/wyrd-cli/tests/workflow_journey.rs#L242), so its passing evidence cannot establish this obligation. **Observable consequence:** `wyrd workflow run --file workflow.yaml --server https://...` exits with usage code 64 before loading, including for a local `wyrd_gateway` run that should use that server. **Required correction:** accept `--server` with a file source and route the override through the existing shared Wyrd client/workflow dependency composition used for gateway execution and external-ref hydration; do not introduce a CLI-owned transport, parser, global registry, or second configuration mechanism. Replace the rejection assertion with a compiled-CLI journey that has no ambient server URL, supplies `--server`, runs an authored `wyrd_gateway` bundle, and proves the named endpoint received the call.

- **BEHAVIOR-002 — VIOLATION** — [`crates/wyrd/wyrd-cli/src/workflow.rs:181`](../../../../../crates/wyrd/wyrd-cli/src/workflow.rs#L181) calls `self.input()` before rejecting local `--detach` or a file source with `--execution server` at lines 184–197. `self.input()` performs `std::fs::read_to_string` at lines 325–332. The task requires these invalid invocation choices to fail before side effects, and the method's own contract says execution choices are checked before any file is read. **Observable consequence:** an otherwise-invalid command can read or block on an input path first, and a missing input file changes the result from `WYRD_CLI_400_INVALID_ARGUMENT` to `WYRD_CLI_500_IO`; a FIFO can delay the refusal indefinitely. Existing tests cover the combinations only without `--input-file`, so they do not exercise the ordering. **Required correction:** finish source/execution/detach compatibility validation before reading run input, retaining the existing stable invalid-argument errors and shared execution paths. Add focused compiled-CLI cases using an unreadable or nonexistent `--input-file` with both invalid combinations and assert the invalid-argument result, proving the path was not opened.

### Suggestions

None. Optional improvements and the five deliberately deferred TASK-004 r6 findings are outside this acceptance audit.

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded evidence only; no build or test command was run

Revision 14 supersedes Revision 12 where they conflict. The approved human decisions supplied with the review were treated as fixed: Rust and TypeScript register through `Cards::register_from_path`, compiled `wyrd apply` proof lives in the CLI journey, Python uses the installed CLI, TypeScript's Skald packaging cone and the dev-only wiremock allowlist are accepted, the five TASK-004 r6 findings remain deferred, and all Revision 13/14 provider decisions stand.

## Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-026 / Scenario 1: exactly one file, UID, or complete exact selector; mutually exclusive object input sources; local default; file cannot execute on server; detach is server-only; `--server` remains an endpoint override | `RunArgs`, `Source`, and `Registered` in `crates/wyrd/wyrd-cli/src/workflow.rs:64-340` | Recorded `workflow_journey::workflow_cli_contract` | **FAIL** — `--server` is incorrectly incompatible with a file (BEHAVIOR-001), and execution compatibility is checked after input-file IO (BEHAVIOR-002) |
| REQ-027: direct `WorkflowRun` JSON, named human outputs, opt-in intermediate steps, stable errors, accepted ID before polling, interrupt leaves run active | `Report`, `RunArgs::run_server`, and exit-code projection in `workflow.rs:229-472` and `lib.rs` | Recorded CLI contract and `workflow_server_detach_status_cancel`; source assertions cover JSON, text, SIGINT 130, continuing run, and no resubmission | PASS |
| REQ-024 / REQ-047 / INV-016: CLI and Rust local execution use the one shared client/Skald runtime rather than a CLI executor | `RunArgs::run_local` calls `wyrd_client::Workflow::{from_path,run}` or `Cards::workflow().load`; no CLI execution engine | Recorded CLI file/registered journeys, Rust SDK journey, and aggregate gate | PASS |
| REQ-028 / AC-002: the actual bundle registers through existing apply without execution | Existing `dispatch_apply` remains the registration owner; CLI journey drives the compiled binary and checks upstream count | Recorded `workflow_file_apply_registered_local` | PASS |
| AC-001 / AC-003: checked-in bundle runs before registration and after exact/UID registered loading with concurrent reviewers, explicit bindings, named outputs, locked identities, and no version floating | `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:883-1003` traces file, apply, exact, UID, and team-reuse paths | Recorded ignored CLI journey passed after the aggregate | PASS |
| REQ-031 / Scenario 3: remote create/get/cancel/wait delegate to shared `Workflows`; default wait, detach, status, cancellation, and dropped polling preserve one accepted job | `RunArgs::run_server` and `RunIdArgs` in `workflow.rs:229-387` | Recorded `workflow_server_detach_status_cancel`; source asserts accepted ID, running status, idempotent cancel, owner/not-found behavior, SIGINT, and provider call count | PASS |
| AC-004 / AC-018 / AC-019 lifecycle and portable snapshot closure relevant to this task | CLI uses the direct shared `WorkflowRun` and existing remote handle; cumulative server/client owners are unchanged | Recorded aggregate gate plus focused CLI lifecycle journey and existing server/client suites selected by the gate | PASS |
| REQ-044 / AC-014–017 / AC-023 / Scenario 4: registered Native, direct external gateway, public Wyrd gateway, server in-process gateway, protocol matrix, precedence, fallback, Vertex boundary, and pre-dispatch refusals | `workflow_registered_route_protocol_matrix`; cumulative server route journey `server_routes_keep_gateway_and_external_ownership`; shared endpoint-policy owners | Recorded CLI route matrix passed; aggregate gate records prior server, gateway, boundary, and Vault regression coverage | PASS |
| REQ-050: CLI consumes the shared `GlobalConfig.workflow` binding setup and resolves selected secrets only at execution | CLI calls shared `Workflow::run`; shared `Workflow::run_with` and `SelectedRoutes` own config/binding preparation | CLI contract checks unconfigured refusal, selected secret delivery, no dispatch, and no secret output; SDK route journeys reuse the same setup | PASS |
| REQ-054–059 / AC-029–031 / Scenario 5: Rust, Python, and TypeScript authored/mixed/registered local journeys retain exact identities and routes without language-owned durable behavior | Extended Rust, Python, and TypeScript workflow-loading journeys; Python invokes installed `wyrd apply`; Rust/TS use Cards registration per approved decision | Recorded focused commands for all three SDKs; corrected Vitest full-name selector recorded; aggregate gate passed | PASS |
| INV-004 / INV-007: local Native and external routes remain available and every shipped local/CLI surface returns the same named `WorkflowRun` contract | CLI direct snapshot rendering and SDK journey assertions | Recorded CLI/SDK journeys and codegen/gate evidence | PASS |
| INV-015 and architecture/security/operations/docs alignment: capability-scoped surfaces, accepted-job lifecycle, affinity/restart loss, route ownership, bindings/secrets, bounds, and provider-tagged Revision 14 model | Changes to `wyrd-design.md`, doctrine, security posture, deployment/recovery, generated Prompt reference, and workflow guide | Recorded `docs:check`, codegen through gate, and final gate | **FAIL** — the prose correctly advertises `--server` as an endpoint-only option, but the shipped parser rejects it for a valid authored-file source (BEHAVIOR-001) |
| Explicit ownership constraints: no duplicate executor, transport, config owner, graph loader, parser, validator, or credential administration | New production behavior is confined to the CLI projection; it delegates loading, registration, runtime, transport, and configuration to existing owners | Full diff/source inspection and boundary checks recorded in the aggregate | PASS |
| Explicit non-goals: no Python/TS server lifecycle, MCP Workflow surface, durable queue/recovery/affinity implementation, arbitrary tool registration, compatibility alias, or migration narrative | No such public surface or machinery appears in the cumulative diff | Full diff inspection; recorded gate | PASS |
| Standing simplicity direction: no unestablished mechanism/check/file/setting/option may be required | No new bespoke production mechanism is needed by the retained findings; corrections reuse the existing endpoint/client/workflow composition and validation path | Source comparison with current CLI/client owners | PASS |

## Caller-to-Result Trace

1. `Cli::dispatch` sends `Command::Workflow` to `WorkflowCommand::dispatch`.
2. `RunArgs::run` resolves the source and input, then selects local or server execution.
3. Local file execution calls `Workflow::from_path` and `Workflow::run`; registered local execution calls the existing `Cards::workflow().load` before the same run method.
4. Shared `Workflow::run_with` selects routes, reads the shared configuration only when needed, reuses the loading client for registered `wyrd_gateway` execution, and otherwise builds its gateway client from ambient configuration.
5. Server execution builds the ordinary global client (including a CLI `--server` override), submits once through `Workflows::create`, prints the accepted ID, then detaches or polls through `Workflows::wait`. Status and cancel use the same shared remote owner.
6. `Report` prints the direct portable snapshot and maps terminal local/server run status to the documented process code.

This trace exposes both findings: the file path is blocked from supplying the endpoint override before step 4, while invalid execution/source combinations reach the input-file read between steps 2 and 3.

## Verification Notes

- Reviewed the complete `cc252339d5add2deff5c5cab826be92954076db0..f2f4b87dc783df790b831b6c818c6273106d661b` diff and the current bodies/callers of the changed CLI path, shared Workflow facade, route preparation, CLI journeys, SDK journeys, server route evidence, and changed architecture/docs.
- The task records `mise run gate` passing at `b88102317`, followed by all three ignored CLI journeys passing, the focused Rust/Python/TypeScript journeys passing, `docs:check`, and a clean `git diff --check`. Candidate commits after `b88102317` are the CI-expectation correction, test-state correction, and evidence record described in the task; this review did not rerun them.
- The recorded checks are credible for the paths they execute, but the CLI contract test affirmatively encodes BEHAVIOR-001 and omits the input-file ordering case behind BEHAVIOR-002. Green evidence therefore does not close those two obligations.
- The candidate remained `f2f4b87dc783df790b831b6c818c6273106d661b` throughout this review.

## Open Questions

None. Both corrections are bounded by the approved CLI contract and existing owners.

## Overall Result

**FAIL**

Proposed finding IDs: `BEHAVIOR-001`, `BEHAVIOR-002`.
