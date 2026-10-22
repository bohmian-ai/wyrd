Review ID: guided-revision-6-readiness-2026-10-21
Target: changes/active/guided-setup/spec.md revision 6; TASK-001-R1 human-directed addition and scenarios 6–7
Gate: Task handoff
Verdict: Ready
Architecture axis: PASS
Executability axis: PASS
Critical: 0
Major: 0
Task coverage: TASK-001-R1 revision-6 delta only
Rehearsal: PASS

## Evidence and scope

This bounded review covers REQ-037–038, AC-020 and the corresponding INV-008 change: explicit `wyrd init guided` launches one selected installed coding agent with the shared prompt and normal approvals, supervises that child, and retains the ready server after agent completion. The maintainer approved this addition. Revision-5 FIND-TASK-001-1 through FIND-TASK-001-5 and their validated corrections are unchanged inputs, not reopened findings. TASK-002 deployment identity and ASGI behavior are outside this review.

Authorities and decisions governed:

- The current user direction, specification revision 6, and remediation packet settle opt-in launch, selection, prompt arguments, approval boundaries, failure handling and process ownership. The remediation explicitly supersedes the historical parent task's passive-only instruction for guided mode; ordinary init remains passive.
- Root `AGENTS.md`, `architecture/README.md`, and `architecture/wyrd-design.md` Doctrine govern existing owners, portability and the prohibition on becoming an application or agent runtime. The bounded selected child is an approved setup action, not a new durable execution service.
- `architecture/operations/deployment-and-release.md` Local server bundles governs installed bootstrap, persistence, Node UI and recovery. The packet explicitly requires updating that owner for the opt-in child lifecycle.
- `architecture/wyrd-security-posture.md` Security principles governs secret handling and unchanged server authentication; no new auth path or approval bypass is authorized.
- `agent_docs/index.md`, `rust.md`, `errors.md`, `maintainer-style.md` and `agent-surfaces.md` govern struct ownership, existing error patterns, literal inputs and discoverability. `docs/AGENTS.md` governs source examples and generated projections.
- All mandatory readiness references were read: plan-review rubric, execution readiness, simplicity, production risk, public workflow, and review format. Production risk applies to local child lifecycle and credentials; public workflow applies to CLI selection, invocation, diagnosis and recovery.

CodeGraph was used before source searches. Source evidence then located the concrete bootstrap and proof owners listed below. No builds, Cargo commands, runtime tests or paid-agent invocations were executed.

## Findings

No blocking findings. No required artifact edit.

## Architecture and execution readiness

The deployed path is the invoking terminal → installed `wyrd` CLI (`InitArgs::run`) → existing `DevServer` → installed Rust server and Node UI, with the selected MCP host configured by `HostInstaller`; guided mode adds one host executable child after readiness and connection. PostgreSQL, saved configuration, credentials and data retain their existing owners. `McpHost` owns invocation syntax; `wyrd_spec::mcp::setup_prompt` owns prompt text. No SDK, HTTP wire shape, schema, database migration or durable agent state is added.

The independent `system-rev` pass (`/root/guided_revision_readiness/system_rev`) and reviewer source tracing establish the failure boundaries. Agent successful/nonzero exit or spawn failure affects the agent action and its visible status; it must not return from the ready foreground server loop. Interruption or unexpected server/UI exit stops and reaps only Wyrd's launched child before existing bounded server/UI settlement. The local service is unavailable after that settlement; saved setup and committed data survive for a normal rerun. An unrelated editor process or detached host session is outside Wyrd ownership. No retry loop or health-check amplification is introduced.

The existing source supports this extension: `init.rs:330` already owns interruption and final settlement; `DevServer::run` at `808` performs readiness, persistence, gateway and host setup while observing server/UI exits; `exited` at `1087` and `stop` at `1134` own failure and cleanup. `settle` at `1144` supplies bounded child termination/reaping. `terminal.rs` already owns cancellable terminal questions and echo restoration. The packet requires usable terminal IO during the agent and after cleanup, without selecting private process-group or terminal implementation mechanics.

Scenario 6 defines real readiness/host connection before a literal argv/cwd/terminal observation, explicit selection and negative modes, no substitute agent, and ordinary-init nonlaunch. Scenario 7 defines healthy/nonzero completion and failed spawn containment, interruption/server failure cleanup, terminal usability, preserved settings and no false verification-success claim. These are meaningful public-boundary assertions with controlled executables; exit status alone cannot establish persisted verification.

## Adversarial analysis

Independent simplicity reviewer: `/root/guided_revision_readiness/simplicity_rev`. No material simplicity finding survived evidence review.

- Retaining only printed handoff is smaller but fails the approved launch requirement. A new setup executor or second supervisor adds ownership without serving a requirement: existing `InitArgs`/`DevServer` already own the lifecycle.
- Executing `McpHost::launch`'s existing display string would avoid a representation change but introduce shell evaluation. One host-owned executable/arguments representation projected into display and launch preserves prompt bytes with less synchronization burden than two host matches. The packet requires this bounded consolidation.
- First-detected host selection would save a question but violate explicit selection, and config detection does not prove executable presence. Reusing `--host` and checking its executable is necessary; a registry or version-probe framework is not.
- A live paid agent would add account and model nondeterminism without strengthening argv, readiness or cleanup proof. Controlled executables plus real Wyrd/Postgres and terminal observations cover the claimed boundary.
- Extending child ownership into pre-existing editor processes would create a broader termination hazard. The packet explicitly limits ownership to the started process and excludes detached host sessions.

Remaining choices—private argument representation, command parser wiring, executable lookup helper placement, terminal guard mechanics and fixture organization—are reversible within fixed behavior. They do not transfer a material product or architecture decision.

## Traceability and task coverage

Only the material revision-6 addition in TASK-001-R1 is claimed ready here.

| Compilation dimension | Result | Evidence |
| --- | --- | --- |
| Outcome and scope | PASS | Human-directed addition fixes one selected agent, approved prompt and server-retention outcome; preserves ordinary init, offline reads, saved inputs and TASK-002 boundaries. |
| Owners and impact | PASS | `InitArgs`, `DevServer`, `McpHost`, `HostInstaller`, CLI help, Setup tab, generated docs and local operations authority are explicitly reached. |
| Interfaces | PASS | Public command, existing `--host`, multiple-host rejection, interactive IO requirement and incompatible output modes are specified. Private launch representation stays adaptable. |
| Control flow | PASS | Reject conflicting modes before bootstrap effects; select/validate host; await server/connection; launch literally; retain server after agent exit; reap owned agent before server/UI cleanup. |
| Acceptance | PASS | REQ-037–038/INV-008 map to AC-020 and scenarios 6–7, including failure and nonlaunch outcomes. |
| Tests | PASS | Extend named installed CLI C2 with controlled executables, real readiness, argv/cwd/terminal and lifecycle observations; preserve C1/offline and host literal proof. |
| Verification | PASS | Exact C1/C2/literal selectors and parent preparation are specified; `mise.toml` defines real-server/UI/Postgres preparation and docs projections. |
| Adaptation | PASS | Existing ownership, no shell execution/new registry/daemon/approval bypass, unchanged identity/native state, and bounded local mechanics distinguish permitted choices from material changes. |
| Rehearsal | PASS | Reviewer rehearsal and independent system/cold rehearsal locate owners, fixtures, commands and first changes without an unresolved material choice. |

Cold rehearsal identity and inputs: this reviewer used only the remediation, minimal approved spec/parent preparation, repository authorities and current source. The independent `system-rev` reviewer repeated the runtime/public-contract rehearsal with the same bounded inputs, without prior verdict guidance.

Located implementation and consumer seams: `src/cli.rs` dispatches `Command::Init`; `src/init.rs` owns parsing/resolution, ready report and foreground supervision; `src/mcp/hosts.rs:93` owns current shell-display syntax; `src/mcp/mod.rs:239` prints connected handoff and `:286` offers connection. The shared prompt owner is `wyrd-spec::mcp::setup_prompt`. Setup docs and the local operations section are named update targets. Generated docs come from `mise.toml:1173` source-driven generation.

Located proof/setup: `tests/cli.rs:37` owns C1 offline/config/network purity; `tests/pg_tests.rs:1389` owns C2 and its real installed-server/UI/Postgres `LocalDev` fixture. `tests/card_lifecycle.rs:2519` supplies a rustix PTY precedent, and the CLI manifest already enables termios plus dev PTY support. `mise.toml:160` supplies CLI/server and production UI preparation; the packet's C2 runs its exact nextest selector through repository Postgres and migrations. The host literal test is `mcp::hosts::tests::launch_preserves_the_setup_prompt_literally`. Docs generation/command/link checks have existing task definitions. No new SDK projection or generated wire contract is needed.

First walked changes: extend C1 for guided conflicts and ordinary/offline preservation; extend C2's installed fixture with a controlled executable and bidirectional terminal IO, assert readiness and host connection before invocation, then observe completion/failure/interruption and server/UI failure; run the specified behavioral RED before changing existing parser/host/bootstrap owners. Add the host-owned safe argv projection, selection/availability checks, and one retained child to existing supervision, then update help/docs/operations and rerun the named proof. A terminal-backed fixture must observe input and restoration, not merely captured stdout. Choosing its mechanics does not change the proof obligation.

Rehearsal result: PASS. No failed pass or revision was required. Runtime correctness remains to be established by implementation evidence.

## Static limitations

This is a task-handoff readiness review, not an implementation approval. FIND-1 through FIND-5 were not re-audited; no source/tests/manifests were edited. No runtime, cross-platform, terminal restoration, real host compatibility or child-cleanup result is claimed. Independent simplicity and system/cold-rehearsal passes were available and completed. Controlled-host tests will prove Wyrd's boundary, while real host behavior still depends on the installed host's own CLI and approval controls.
