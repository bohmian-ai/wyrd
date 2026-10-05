# TASK-005 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded-evidence inspection only; no build or test was run.

Revision 14 was used where it supersedes the task's Revision 12 wording. The
standing human decisions about SDK registration paths, TypeScript's Skald
packaging closure, the dev-only wiremock allowlist, the deferred TASK-004 r6
items, and Revision 13/14 provider behavior were treated as fixed inputs and
were not reopened.

## State and value-flow review

The changed runtime path has one CLI owner, `RunArgs`, which produces a
validated `Source` and input object. Local execution delegates the selected
source to `wyrd_client::Workflow::from_path` or `Cards::workflow().load`, then
delegates execution to the one Skald workflow. Server execution resolves a UID
to an exact `CardRef`, constructs one `CreateWorkflowRunRequest`, and delegates
create/wait/get/cancel to `wyrd_client::Workflows`. The accepted run ID is
printed before polling, and dropping the wait on SIGINT does not call cancel or
create again.

The SDK journey changes continue through the shared Workflow/Cards owners and
the same Skald runtime. The Python installed-CLI apply path and the approved
Rust/TypeScript `Cards::register_from_path` paths preserve registration as a
declarative operation. Route preparation remains in the shared client: selected
external bindings resolve at run start, while file loading and registration do
not resolve their secrets. No second executor, graph, transport, credential
catalog, remote Python/TypeScript lifecycle, MCP workflow surface, durable run
queue, compatibility alias, or persistent recovery mechanism entered the diff.

Two CLI producer-to-sink defects remain. First, execution-mode validity is
checked only after `RunArgs::input` performs filesystem IO. Second, the
`server` endpoint override is discarded for authored-file execution by a Clap
conflict even though authored workflows may need registry or public-gateway IO.
Both defects are reachable from the shipped command and are detailed below.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-026/REQ-027/REQ-050 and Scenario 1: exact source/input/mode selection; invalid choices rejected before side effects; `--server` remains the endpoint override; portable JSON and named human output | `crates/wyrd/wyrd-cli/src/workflow.rs:64-125,158-339,389-470`; command registration in `src/cli.rs` and `src/lib.rs` | Recorded `workflow_journey::workflow_cli_contract` PASS | **FAIL** — INVREV-001 and INVREV-002 |
| REQ-024/REQ-028/REQ-031/REQ-047, INV-016, AC-001–AC-004/AC-013: authored, applied, exact/UID registered-local, and server execution use existing owners and one async engine | `RunArgs::{run_local,run_server}`, `Cards::workflow().load`, `Workflow::run`, `Workflows::{create,wait,get,cancel}`; compiled CLI journey uses the checked-in code-review bundle | Recorded focused CLI journey PASSes and `mise run gate` PASS | PASS |
| REQ-044, INV-004/INV-007, AC-014–AC-017/AC-023/AC-026: route/protocol/security behavior remains shared and portable | `workflow_registered_route_protocol_matrix`; SDK example-route extensions; local binding configuration flows through shared `GlobalConfig.workflow` | Recorded CLI route matrix and Rust/Python/TypeScript focused journey PASSes; aggregate gate PASS | PASS |
| Server accepted-job lifecycle: detach, status, idempotent cancel, interrupt without cancel/resubmit, exact snapshots (REQ-024/REQ-026/REQ-027/REQ-031, AC-004/AC-018) | `RunArgs::run_server` prints the accepted ID before `Workflows::wait`; `RunIdArgs` delegates get/cancel; no retry/cancel branch is introduced on SIGINT | Recorded `workflow_server_detach_status_cancel` PASS | PASS |
| REQ-054–REQ-059 and AC-029–AC-031: authored and registered team reuse in Rust/Python/TypeScript; exact UIDs and versions; lazy authority; no floating or dispatch during apply | Rust `workflow_loading.rs`; Python `test_workflow_loading_journey`; TypeScript `workflow-loading.test.ts`; approved registration-path deviations retained | Recorded Rust, Python, and corrected full-name TypeScript selectors PASS | PASS |
| INV-015 and documentation closure: CLI, scoped surfaces, accepted-job authority, route ownership, restart loss, affinity, bounds, and provider destination are recorded in current authority/docs | `architecture/wyrd-design.md`, `wyrd-doctrine.mdx`, `wyrd-security-posture.md`, operations docs, workflow how-to, generated Prompt reference | Recorded `docs:check` and aggregate gate PASS | PASS |
| Repository gates and changed dependency selection remain honest | Dev-only wiremock paths use the approved existing allowlist mechanism; TypeScript packaging expectation follows the real dependency closure; server test state reuses test support | Recorded focused server test PASS; final `mise run gate` exit 0; `git diff --check` clean | PASS |
| Prohibited scope remains absent: no new Python/TypeScript server lifecycle, MCP Workflow API, persistent queue/recovery/affinity implementation, credential administration, compatibility layer, duplicate executor/transport/config owner, or deferred TASK-004 r6 remediation | Complete base-to-candidate name-status and source diff | Source inspection; task evidence records the same exclusions | PASS |

## Proposed findings

### INVREV-001 — INCORRECT: invalid execution choices perform input-file IO before refusal

- Violated obligation: TASK-005 Scenario 1 requires a server file source and
  local `--detach` to fail before side effects. The implementation's own
  `RunArgs::run` contract says invocation choices are checked before a file is
  read.
- Location: `crates/wyrd/wyrd-cli/src/workflow.rs:181-199`, especially the
  unconditional `self.input()?` at line 183 before the mode/source match;
  `RunArgs::input` reads `--input-file` at lines 325-338.
- Evidence and reachability: both
  `wyrd workflow run --file workflow.yaml --execution server --input-file missing.json`
  and
  `wyrd workflow run --file workflow.yaml --detach --input-file missing.json`
  enter `RunArgs::input` first. The current result is
  `WYRD_CLI_500_IO`, not the required invalid-mode error. A readable special
  file can also be opened or block even though the command can never execute.
  The changed test covers these invalid modes only without an input file, so it
  cannot detect the ordering defect.
- Observable consequence: an invalid invocation performs avoidable filesystem
  work and exposes the wrong stable failure. Mode validation is no longer the
  producer invariant that protects later input IO.
- Required testable correction: validate the selected source's compatibility
  with `execution` and `detach` before reading or parsing input, while retaining
  the existing shared owners and error codes. Extend the compiled CLI contract
  proof with invalid server-file and local-detach invocations that also name a
  nonexistent input file; both must return
  `WYRD_CLI_400_INVALID_ARGUMENT` for the execution choice rather than an IO
  error.

### INVREV-002 — DRIFT: authored-file runs reject the established `--server` endpoint override

- Violated obligation: REQ-026 and TASK-005's packet-local CLI seam preserve
  existing `--server URL` as the Wyrd connection endpoint; it is not a source
  or execution-mode flag. Authored Workflows may lazily resolve external Cards
  and local `wyrd_gateway` steps call the public Wyrd endpoint.
- Location: `crates/wyrd/wyrd-cli/src/workflow.rs:105-107` declares
  `conflicts_with = "file"`; `RunArgs::run_local` at lines 208-226 consequently
  never passes the endpoint override into authored loading or route
  preparation. The new test codifies the extra rejection at
  `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:193-243`.
- Evidence and reachability: `--server` reaches `crate::client::from_global`
  for registered-local and server-run paths, but a file source is rejected by
  Clap before `Workflow::from_path`. The successful file/public-gateway CLI
  journey instead injects `WYRD_SERVER_URL` into the child process in
  `Journey::command` (`workflow_journey.rs:662-668`), so it does not prove the
  advertised CLI override. A user with a different or absent ambient endpoint
  cannot run that same authored file against `--server URL`.
- Observable consequence: one shipped source cannot use the CLI's normal
  endpoint-selection mechanism for its lazy registry reads or public gateway
  calls. The unapproved conflict is a surface-specific restriction and makes
  the option's meaning depend on source type.
- Required testable correction: delete the file/endpoint conflict and carry the
  explicit endpoint through the existing shared-client loading and local-route
  composition; do not add a CLI parser, transport, credential owner, global
  registry, or environment-mutation workaround. Prove the compiled
  `--file ... --server <real test server>` path with the ambient endpoint
  absent or deliberately wrong, using an authored external ref or
  `wyrd_gateway` route to demonstrate that the explicit endpoint is the one
  consumed.

## Review result

**FAIL**

The recorded integrated verification is broad and credible for the paths it
selects, and the candidate preserves the required ownership and non-goals.
However, the two reachable CLI contract defects above leave Scenario 1 and
REQ-026 incomplete. Both corrections are bounded to the approved CLI/shared
client behavior and require no new product, security, durability, or
cross-service decision.

## Verification notes

- Reviewed recorded evidence only, as required. No command that builds, tests,
  formats, lints, generates, or mutates implementation state was run.
- Recorded evidence includes the four focused CLI selectors, the Rust/Python/
  TypeScript SDK journeys, the Python gateway fixture regression lane, the
  focused server run-tool test, `docs:check`, a final successful `mise run
  gate`, and clean `git diff --check`.
- The TypeScript evidence uses the corrected full Vitest name documented in the
  task. The three ignored CLI journeys were recorded as passing after the
  aggregate gate because the aggregate does not select them.
- Candidate identity remained
  `f2f4b87dc783df790b831b6c818c6273106d661b` throughout this review.
