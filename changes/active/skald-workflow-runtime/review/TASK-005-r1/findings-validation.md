# TASK-005 findings validation

## Immutable subject and inputs

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded-evidence inspection only; no build, test,
  formatter, linter, generator, or executable verification was run.

Revision 14 supersedes the task's Revision 12 wording where they conflict. The
approved allocation of Rust, Python, TypeScript, and compiled-CLI registration
proof; TypeScript's Skald packaging cone; the dev-only wiremock allowlist; the
five deferred TASK-004 r6 findings; and the Revision 13/14 provider decisions
were treated as fixed authority and were not reopened.

All required discovery reports were present and read:
`task-review-behavior.md`, `task-review-invariants.md`,
`standards-review.md`, `maintainer-review.md`, `system-review.md`,
`domain-review-security.md`, `domain-review-lifecycle.md`, and
`domain-review-multiruntime.md`. No follow-up report was required: the
scope-specific PASS reports do not contradict the four proposed source claims,
and the overlapping claims resolve directly from the approved CLI contract and
candidate source.

## Source trace and claim validation

### Endpoint override for authored files

**Discovery claims:** `BEHAVIOR-001`, `INVREV-002`, `STD-001`, `SYS-001`  
**Validation:** **REVISED**

The source claim is reachable and material. `RunArgs::server` declares
`conflicts_with = "file"` at
`crates/wyrd/wyrd-cli/src/workflow.rs:105-107`, and the compiled-CLI contract
asserts that rejection at
`crates/wyrd/wyrd-cli/tests/workflow_journey.rs:217-253`. Clap therefore exits
64 before `RunArgs::run`, even though Revision 14 REQ-026 fixes `--server` as
the endpoint selector rather than an execution-mode flag. File execution stays
local because `--execution server --file` is separately rejected by
`RunArgs::run`; allowing an endpoint does not weaken that boundary.

The endpoint is not optional plumbing for every authored Workflow. The shared
`Workflow::from_path` path builds `Cards::new(None, None)` on the first external
Card ref (`crates/shared/wyrd-client/src/workflow/mod.rs:79-98`), and a loaded
client is retained for a later `wyrd_gateway` route. When no external ref loaded
a client, `Workflow::run_with` builds the gateway client from ambient
configuration (`workflow/mod.rs:143-162,191-235`). The CLI's explicit endpoint
currently reaches neither path. The successful CLI journey instead injects
`WYRD_SERVER_URL` in `Journey::command`
(`workflow_journey.rs:662-668`), so it does not close the explicit-option
contract.

Deleting only the Clap conflict would accept and then ignore the option. The
source trace also revises discovery's claim that this is a bounded
implementation correction. The approved Rust public surface exposes only
`Workflow::from_path(path)` for authored loading, and that method owns both
lazy Cards construction and the client retained for later gateway execution.
The CLI is a separate crate, so it cannot supply an endpoint to that owner
through a private seam. Every safe root-cause correction therefore needs a new
public shared-client composition decision: an endpoint- or client-aware
authored-loading surface that preserves lazy no-client behavior for wholly
local Workflows and carries the same client from external-ref hydration into
`wyrd_gateway` execution. Changing ambient process state would instead create
the prohibited environment-mutation workaround and is not an acceptable
private escape hatch.

The approved specification must choose that public surface before remediation;
this validator does not prescribe its signature. The decision must retain the
existing `Workflow::from_path(path)` behavior for SDK callers, explicit endpoint
precedence with ambient authentication for the CLI, the existing Cards
hydrator, and one retained gateway client, without adding a CLI transport,
parser, global registry, or second configuration owner. Narrowing REQ-026 so
the option is accepted but ignored for authored files would change the approved
observable behavior and is not a valid remediation.

Focused closure proof is one compiled-CLI authored-file journey whose bundle
contains a registered external dependency and a `wyrd_gateway` route, whose
ambient endpoint is absent or deliberately wrong, and whose explicit
`--server` endpoint is observed for both hydration and gateway dispatch. Keep
the separate negative proof that `--execution server --file` is refused. No new
harness, setting, or check is warranted.

### Invalid modes read input before refusal

**Discovery claims:** `BEHAVIOR-002`, `INVREV-001`  
**Validation:** **REVISED**

`RunArgs::run` resolves the source and immediately calls `self.input()` at
`crates/wyrd/wyrd-cli/src/workflow.rs:181-184`; only afterward does its match
reject local `--detach` and server execution of a file at lines 185-198.
`RunArgs::input` calls `std::fs::read_to_string` for `--input-file` at lines
325-332. Both invalid invocations are public CLI paths, so a missing input file
changes the result to `WYRD_CLI_500_IO`, and a special file can block before the
required `WYRD_CLI_400_INVALID_ARGUMENT` refusal. The current contract test
uses no `--input-file` for either invalid mode
(`workflow_journey.rs:256-264`) and cannot prove ordering.

The smallest correction is to finish source/execution/detach compatibility
validation on the already-produced `Source` before calling `RunArgs::input`,
then continue through the unchanged local and server owners. Do not add another
validator type or input abstraction. Focused closure is the existing compiled
CLI contract extended with both invalid combinations naming a nonexistent
`--input-file`; each must report the execution-choice
`WYRD_CLI_400_INVALID_ARGUMENT`, proving the input path was not opened.

### Function-scoped Unix trait imports

**Discovery claim:** `MAIN-001`  
**Validation:** **CONFIRMED**

`crates/wyrd/wyrd-cli/tests/workflow_journey.rs:148-153` and
`sdks/wyrd-sdk-rust/tests/workflow_loading.rs:465-470` each place
`use std::os::unix::fs::PermissionsExt as _;` inside an ordinary helper
function. Neither is a generic function needing the repository's narrow local
trait-import exception. `architecture/agent-rules.md` explicitly requires the
module import block, and nearby external-test modules use a top-level
`#[cfg(unix)]` import (`crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:12`,
`crates/wyrd/wyrd-testing/src/server.rs:8`).

Move the same trait import, guarded by `#[cfg(unix)]`, to each module's existing
top-level import block. Leave permission setting and fixture behavior
unchanged; no shared helper, allowlist, lint, or new check is justified.
Closure is source inspection plus the existing format/lint lane and the two
affected Workflow journey commands.

### Fallible Ctrl-C listener is treated as an interrupt

**Discovery claim:** `STD-002`  
**Validation:** **REVISED**

The server-wait branch at `crates/wyrd/wyrd-cli/src/workflow.rs:262-271` binds
the output of `tokio::signal::ctrl_c()` to `_`. That future returns
`std::io::Result<()>`; both `Ok(())` and listener-registration `Err` therefore
print that the user interrupted the CLI and return 130. The path is reachable
after the server has accepted a run. On error, dropping `Workflows::wait` still
leaves the accepted run active, but the CLI reports a user action that did not
occur and discards the actionable failure, contrary to REQ-027's stable-error
boundary and the repository rule against swallowing errors.

Handle the signal result inside the existing `tokio::select!`. Only `Ok(())`
may print the interruption message and return 130. Map `Err` to the existing
derive-backed `WyrdError::WorkflowInternal` boundary (and therefore the
existing `WyrdCliError::Server` projection), with a safe message identifying
signal-listener initialization; do not add a new public error code, signal
abstraction, retry, setting, injection framework, or checker. This preserves
the accepted run and exposes an established stable failure without expanding
the contract.

The existing compiled SIGINT journey remains the focused behavioral proof for
the successful signal branch. Source inspection and the normal compile/lint
lane prove that the error result is no longer discarded. A bespoke mechanism
to force OS signal-registration failure would cost more than the static
invariant and is not required.

## Deduplicated validated finding ledger

### FIND-TASK-005-1

- **Discovery sources:** `BEHAVIOR-001`, `INVREV-002`, `STD-001`, `SYS-001`
- **Status:** `REVISED`
- **Classification:** `DRIFT`
- **Violated obligation:** Revision 14 REQ-026 and TASK-005 Scenario 1 preserve
  `--server` as the ordinary Wyrd endpoint selector while file execution
  remains local.
- **Location:** `crates/wyrd/wyrd-cli/src/workflow.rs:105-107,208-226`;
  `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:217-253,662-668`;
  shared consumer path in
  `crates/shared/wyrd-client/src/workflow/mod.rs:79-98,143-162,191-235`.
- **Evidence:** the parser forbids `--file` with `--server`; authored external
  refs and public-gateway runs otherwise resolve only ambient configuration;
  the journey proves the ambient variable, not the explicit option.
- **Observable consequence:** an authored local Workflow cannot select an
  alternate Wyrd endpoint for external-Card hydration or a public-gateway call
  and exits with usage code 64 before loading.
- **Required authority:** `SPEC_REVISION_REQUIRED`. Approve the public
  shared-client composition surface by which the CLI supplies its endpoint to
  authored loading while preserving the existing one-argument
  `Workflow::from_path` contract, lazy no-client behavior, existing Cards
  hydration, one retained client for `wyrd_gateway`, and ambient
  authentication. Do not use CLI transport, parser, environment mutation,
  another config owner, compatibility behavior, or a new check as a substitute
  for that decision.
- **Focused closure proof:** run one authored bundle with an external Card ref
  and `wyrd_gateway` through compiled
  `wyrd workflow run --file ... --server <test-server>` while ambient endpoint
  selection cannot succeed; observe both
  hydration and dispatch at the explicit endpoint. Retain the server-execution
  file refusal.

### FIND-TASK-005-2

- **Discovery sources:** `BEHAVIOR-002`, `INVREV-001`
- **Status:** `REVISED`
- **Classification:** `VIOLATION`
- **Violated obligation:** TASK-005 Scenario 1 requires invalid source,
  execution, and detach choices to fail before side effects with the stable
  invalid-argument error.
- **Location:** `crates/wyrd/wyrd-cli/src/workflow.rs:181-199,319-338` and
  `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:256-264`.
- **Evidence:** `self.input()` reads `--input-file` before either invalid mode is
  rejected, and current tests omit the input-file ordering case.
- **Observable consequence:** an invalid invocation can read or block on a
  file and report `WYRD_CLI_500_IO` instead of the required mode error.
- **Decision-complete correction:** validate source/mode/detach compatibility
  before `RunArgs::input`, then reuse the unchanged local or server execution
  path. Add no validation abstraction or second input owner.
- **Focused closure proof:** compiled-CLI cases for local `--detach` and server
  execution of a file, each with a nonexistent `--input-file`, both return
  `WYRD_CLI_400_INVALID_ARGUMENT` for the incompatible execution choice.

### FIND-TASK-005-3

- **Discovery source:** `MAIN-001`
- **Status:** `CONFIRMED`
- **Classification:** `VIOLATION`
- **Violated obligation:** `architecture/agent-rules.md` requires all ordinary
  Rust imports at module scope; neither narrow exception applies.
- **Location:** `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:148-153` and
  `sdks/wyrd-sdk-rust/tests/workflow_loading.rs:465-470`.
- **Evidence:** both helper bodies import `PermissionsExt`; their module import
  blocks conceal the Unix dependency.
- **Observable consequence:** the test modules' platform dependency is hidden
  from the repository-standard dependency manifest and duplicated at the use
  sites.
- **Decision-complete correction:** move each existing import to its module
  import block under `#[cfg(unix)]`; change no fixture logic and add no helper,
  allowlist, lint, or check.
- **Focused closure proof:** source inspection finds no function-scoped import;
  the existing formatting/lint lane and affected CLI/Rust SDK journey commands
  remain green.

### FIND-TASK-005-4

- **Discovery source:** `STD-002`
- **Status:** `REVISED`
- **Classification:** `VIOLATION`
- **Violated obligation:** REQ-027 requires stable CLI failures, while the
  repository implementation rules prohibit swallowing errors and changing
  error semantics.
- **Location:** `crates/wyrd/wyrd-cli/src/workflow.rs:262-271`.
- **Evidence:** the `tokio::signal::ctrl_c()` `std::io::Result<()>` is bound to
  `_`, so listener failure takes the same 130/user-interrupt path as SIGINT.
- **Observable consequence:** after acceptance, listener failure silently stops
  polling and falsely reports user interruption while hiding its cause; the
  server run remains active but the CLI exposes the wrong outcome.
- **Decision-complete correction:** distinguish `Ok(())` from `Err` in the
  existing select arm. Keep 130 only for `Ok(())`; project `Err` through the
  existing derive-backed `WyrdError::WorkflowInternal` and
  `WyrdCliError::Server` path with safe signal-listener context. Add no new
  error code, abstraction, retry, setting, injection harness, or check.
- **Focused closure proof:** the existing compiled SIGINT journey still exits
  130 and leaves one accepted run active; source inspection plus normal
  compile/lint verification establishes that the error result is handled.

## Validation outcome

Four findings survive validation: `FIND-TASK-005-1` through
`FIND-TASK-005-4`. Findings 2-4 are bounded corrections within approved
behavior and existing owners. Finding 1 cannot be corrected safely through a
private implementation choice: the approved one-argument authored-loading API
offers the separate CLI crate no endpoint-injection seam, while the approved
CLI contract requires the endpoint to govern authored external-ref and public-
gateway IO. `SPEC_REVISION_REQUIRED` is therefore indicated for the shared
client public composition decision. No security, concurrency,
resource-ownership, or persistent-data decision is required.
