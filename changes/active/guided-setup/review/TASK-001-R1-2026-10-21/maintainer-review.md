# TASK-001 cumulative maintainer review

Result: **FAIL** — one new field misses the explicit Rust-item documentation gate. No material workflow duplication or complexity finding.

## Subject and method

Reviewed base `ebd75dc224854413d17cb06b70da8181b78cde6b` through candidate `796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`, including the latest fixes from `00b6148919b9ccd5f958f3b621ab6a89c6372001`. HEAD still equalled the candidate at the final source check; the only untracked content was this review directory. Source stayed immutable.

Read the approved revision-6 specification, original task, prior verdict and final validation ledger, remediation packet and implementation evidence, task-review maintainer instructions, root and applicable Python/TypeScript/docs rules, maintainer/Rust/error/agent-surface guides, change workflow, testing guidance and relevant architecture owners. Used CodeGraph before subsequent source navigation. Did not read other current discovery reports.

This is a static maintainer audit. Submitted test results were inspected as claims; no Cargo, test, formatter, generator, paid agent, or runtime journey was run.

## Changed-surface coverage

| Surface | Source, callers and proof inspected | Assessment |
|---|---|---|
| CLI rename and bootstrap | `cli.rs`, renamed `init.rs`, `server/mod.rs`, `server/install.rs`, library registration, gateway/shared-config references; installed CLI and TypeScript wrapper dispatch; C1 and existing local bootstrap tests and recipe | Existing foreground bootstrap moved and extended rather than replaced. Existing installer, persistence, readiness, provisioning, gateway, diagnostics and interruption owners remain discoverable. |
| Explicit guided orchestration | `InitArgs::{run,agent}`, `DevServer::{run,guide,exited,stop}`, `Agent`, `SavedTerminal`, existing `Terminal`/`EchoGuard` and `settle`; C2 `Guided` PTY and fake-host fixture, selection/refusal/exit/server-failure assertions | Dependency ownership stays with `DevServer`. `Agent` adds only the launched child's retained process/terminal state. Existing bounded `settle` is reused, with agent settlement before server/UI settlement. New saved-terminal tuple field has the finding below. |
| Host handoff | Complete seven-branch `McpHost` launcher/display/command/installed methods; `HostInstaller::{from_process,detected,config_path}`, existing host-selection functions, `offer_local`, report handoff and `into_result`; literal host test and C2 | One argv representation feeds both literal shell display and direct spawn. Configuration detection remains separate from launch-program availability, which is necessary for guided selection. No new host registry or parallel connection workflow. |
| Prompt and MCP | Shared Markdown and release formatter; server guide prompt descriptor/reader and authenticated handler; CLI proxy prompt forwarding; CLI readiness/offline consumers and Python MCP prompt journey | One text owner, thin projections through existing authenticated transport/handler. No executor or prompt registration machinery. |
| Schema correction | Complete schemars `Spec` implementation and unchanged utoipa sibling, MCP Card result consumer and strict-client diagnosis | `anyOf` fixes overlapping untagged schemas at the existing producer. Changed method rustdoc now explains the relevant consumer contract. No unrelated OpenAPI rewrite. |
| Public Python adapter | Complete `_middleware.py`, package initializers and authored re-export stub; native Python state lifecycle and Run context hooks, shared lifecycle shutdown, ASGI example and worker app | ASGI adaptation delegates writer/provider/run behavior to existing owners. `_start` orders startup and partial cleanup; `_drain` invokes the native finalizer off-loop; local `drain` tracks only protocol cleanup attempts. It creates no competing lifecycle, queue, registry or provider. Public inline types and stub exports agree. |
| Correlation | Changed `_enter_run`, `_exit_run`, `_RunCorrelationProcessor.on_start` in surrounding `otel.py`; native Run callers, application-owned provider path and C6/C7 | Existing correlation owner extended for instrumentation that replaces OTel context. The plain context-variable fallback has a specific missing-context purpose and remains execution-local; no request registry was added. |
| Tests and fixtures | Full new ASGI lifespan/request/provider/example/worker tests; Python manual helper/runners and MCP reader; CLI offline and full C2 PTY/host test additions; modified existing bootstrap callers | Outcomes include real stored evidence, exact prompt bytes, process/terminal state, provider ownership and negative flows. Helpers have bounded local responsibilities and extend existing journey owners. Fault injection does not introduce product test switches. |
| Documentation and examples | Setup, Quickstart readback SQL, audience paths, FastAPI guide/example; renamed connect/operate pages and example references; architecture/changelog changes; generated `llms*.txt` diff | Published Manual path now includes the durable query and Run grouping used by its journey. Instructions use existing Card/query surfaces; embedded code retains a tested source. Authority describes opt-in child ownership without claiming an agent runtime or process-tree ownership. |
| Docs publication/build | Landing raw-source import and highlighter export removal, shell publication fixture, command-checker fixture; docs contributing/product rules | Vite captures the same existing example sources at build time, fixing bundled-path IO at its owner. Fixture mirrors the new source dependency; no second example copy or loading framework. |
| Packaging and lanes | Python dev dependency and lock changes, ASGI journey membership, local-CLI journey membership, live-readiness and renamed example/test consumers, TS CLI wrapper | New dependencies are confined to proof of the requested real worker/instrumentation boundaries. No runtime dependency or feature abstraction added. Generated declaration parity is unchanged outside the authored pure-Python re-export. |

## Complete-workflow reuse assessment

The alternative to new setup orchestration is the existing installed-server workflow, and the candidate extends that complete workflow: `InitArgs::run` retains signal/input/installer composition, while `DevServer::run` still owns migration/readiness, credential/config persistence, provisioning, gateway, host setup and waiting. Guided launch happens after the same readiness and host result, and cleanup runs through the same caller-owned stop boundary. The added `guide` stage does not replicate bootstrap or server supervision. `McpHost` holds both the display and process projections rather than making a second host capability table.

For ASGI, native `PyWyrdState` already coordinates managed telemetry flushing and shared-client shutdown; shared `BifrostLifecycle` already owns writer state and drain outcome. The adapter calls that whole workflow. It adds only protocol message ordering, off-loop execution and cleanup-once tracking; moving these into the native state would mix an ASGI lifetime into a language-independent owner. Existing Run context entry/exit is reused for request lifetime, and existing OTel correlation remains the single enrichment owner. No replacement lifecycle is justified or present.

C2's latest fixture correction matches the stated race: the earlier announcement could precede `stty` completing, whereas the candidate announces after changing terminal mode and uses `exec sleep`, preserving the recorded process identity. That is a fixture synchronization correction within the approved one-child boundary. I did not independently rerun the race, and the separate diagnostician conclusion in task evidence remains inspected evidence rather than my own runtime proof.

## Prior finding closure

| Prior ID | Source assessment within this lens |
|---|---|
| FIND-TASK-001-1 | `_lifespan` now attempts the existing guarded drain before rethrowing an established application's exception or cancellation. It preserves primary failure and avoids repeating terminal-message drain. New negative tests inspect state closure and retained final evidence. |
| FIND-TASK-001-2 | One `_diagnostic` projection withholds arbitrary unexpected exception text while retaining canonical Wyrd code/message and exact missing-extra remedy. Application-created protocol messages continue unchanged. |
| FIND-TASK-001-3 | Setup Manual links Quickstart's published `get`/SQL/query readback, and the helper executes those published commands. |
| FIND-TASK-001-4 | All seven hosts now have launch prefixes in the same owner; shell display and argv spawn share them. The literal test covers each branch. |
| FIND-TASK-001-5 | All three previously retained sites close: changed `Spec::json_schema` has method docs, the new `mcp` tests module has item docs, and `walk` explains joined paths and filesystem panic conditions. The separate new-field omission below is not a claim that those corrections failed. |

The earlier rejected pre-receive exception proposal and application-owned diagnostic rewriting are not reopened. Neither is necessary to correct the material owner or maintenance properties assessed here.

## Material proposal

### MAINT-R1-001 — VIOLATION: new saved-terminal field has no rustdoc

- **Changed location:** `crates/wyrd/wyrd-cli/src/terminal.rs:107`, the `Termios` tuple field in `pub(crate) struct SavedTerminal(Termios);`.
- **Governing rule:** root `AGENTS.md` makes missing changed-item rustdoc a merge blocker; `agent_docs/rust.md`, Rustdoc, explicitly includes fields regardless of visibility. The task-review skill preserves this exception to the normal behavioral-consequence threshold.
- **Evidence and caller path:** the struct has type-level prose but its new tuple field has no documentation attribute. `SavedTerminal::save` captures it, `restore` applies `self.0`, and `Agent` retains it across the guided process lifetime. Nearby `EchoGuard` documents its saved `Termios` field separately.
- **Concrete cost:** the new retained terminal-state field does not meet the repository's explicit documentation gate. Its struct documentation already explains most of the intent; this is a narrowly scoped gate failure, not a claim of runtime risk or a request to redesign the wrapper.
- **Smallest testable correction:** document the existing tuple field as the standard-input attributes captured before the owned agent starts and later restored after it is reaped. Preserve its representation and behavior. Static source inspection is sufficient; add no test or abstraction for this documentation correction.

No other blocking maintainer finding, optional refactor, or preference-only proposal is retained.

## Limits

No runtime checks were rerun. Submitted evidence is Linux x86_64 only; macOS arm64/Linux arm64 and real paid-host runtime behavior remain unexercised. Combined docs/SDK/bootstrap aggregates and TASK-002 seams remain at integrated change review. Only this report was written.
