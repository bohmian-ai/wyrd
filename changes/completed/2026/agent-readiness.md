# Make Wyrd ready for coding agents

- Change: `agent-readiness`
- Specification: `SPEC-agent-readiness`, approved revision 2
- Completed: 2026-10-19
- Reviewed target: `a7feddd16`. The human owner closed the change without a
  final change review. TASK-001–003 last received `FIX_REQUIRED` from task
  review (round 2, candidate `d6b7c75b8`), and the owner closed with that
  round's three code findings and two evidence blockers unresolved. TASK-004
  was implemented and its live run passed, but it was never task-reviewed.
  See [Open at completion](#open-at-completion).
- Delivery reference: not supplied

A person points a coding agent with shell and file access at the Wyrd docs or
MCP. The agent should then run Declare -> Observe -> Verify on that person's
application without being taught Wyrd: declare a Service with its components
and Verifier bindings, pull and load the bundle through an SDK, emit
correlated telemetry and custom rows, and read back synchronous and
asynchronous judgments from the server. Humans use the same documented
contracts.

## Shipped behavior

- Card declaration vocabulary is replaced in place on every surface.
  `wyrd declare PATH` replaces `apply`. `wyrd declare PATH --check` replaces
  `plan` and validates purely locally. SDK `register*` becomes `declare*`, the
  wire and receipt types become `Declaration*`, the outcome is `"declared"`,
  and the SQL table is `card_declaration_operations`. No aliases.
- `get` returns one Card envelope and downloads no artifacts. MCP
  `cards.get_by_ref` is merged into `cards.get`. `wyrd pull`/`cards.pull`
  replace graph-download `hydrate` and `get --output-dir`, producing a
  `wyrd/bundle/v1` bundle with `BundleManifest` types. Public `wyrd load` is
  removed, and the retired bundle header is refused.
- `server install`, `server dev`, `mcp install`, `declare` (including
  `--check`), `get` and `pull` accept `--format json`: one typed result on
  stdout, one structured error document on stderr, and usage failures as
  `WYRD_CLI_400_USAGE`. `server dev` prints one readiness line, then waits in
  the foreground. JSON-mode diagnostics go to a log file. Unattended runs
  never prompt.
- MCP instructions point new agents to `wyrd.guide {"topic":"start"}`, a
  static onboarding topic for the full workflow. MCP still only inspects and
  runs verification.
- Docs teach the workflow end to end with consistent names. `llms-full.txt`
  holds generated page bodies and embedded code. The quickstart's outputs
  were captured from a real local-provider run. The Rust and TypeScript
  reference apps are compiled or type-checked.
- `AGENTS.md` names exactly Linux x86_64, Linux arm64 and macOS arm64.
- The opt-in `mise run test:agent-readiness:smoke:live` lane runs Claude Code
  and Codex against a harness-owned server and docs built from the commit
  under test. It judges each agent only from server evidence.

## Invariants and constraints

- MCP teaches, inspects and verifies. The CLI declares, pulls and runs local
  development. SDKs instrument applications. The server and Vala own durable
  behavior. No MCP declare, check or upload tool exists.
- The person or live harness owns the foreground dev server, and agents
  connect to it. There is no detached mode and no stop API.
- Declaring grants no authority, and no secret enters host config, argv or
  live-lane artifacts. Audit stays non-blocking on the Scribe outbox.
- No new durability, acknowledgment or exactly-once promise. Empty or partial
  evidence is never success.
- The live lane is excluded from `gate` and CI. Deterministic journeys stay
  credential-free.

## Decisions and deviations

- Revision 2 removed revision 1's detached lifetime and stop requirements. It
  settled DEC-001 as a local-only check with no MCP check tool, and required
  the live agents to use docs built from the commit under test.
- The owner replaced the four-target platform statement with three targets.
  Intel macOS installers stay out of scope.
- Declaration migrations were edited in place (pre-release schema), not
  migrated.
- TASK-001–003 review round 1 found six code findings, all closed by
  remediation R1. Round 2 confirmed those closures and found that R1 had
  introduced one new cause (FIND-TASK-003-4).
- TASK-004 known limits: the agent objective fixes Python, because the
  current-build SDK ships as a wheel. MCP-run and app-run verifications look
  the same on the server. Codex can still fetch the published site (only
  Claude Code blocks web search). One stochastic pass per agent is not a
  reliability claim.

## Acceptance closure

| Criterion | Status | Evidence or gap |
|---|---|---|
| AC-001 discoverable workflow | Partial | MCP `start` discovery journey; real-server published-quickstart journey (local provider). Installed-artifact acquisition unproven (FIND-TASK-003-3) |
| AC-002 local check, authoritative declare | Closed | Offline check and diagnostics CLI tests; Postgres declaration/replay tests |
| AC-003 declaration names, no aliases | Closed | SQL/route/served-OpenAPI tests, three-SDK declare/pull journeys, `codegen:check` |
| AC-004 envelope get, merged MCP get | Closed | CLI, SDK and MCP get, selector and refusal tests |
| AC-005 pull bundles | Closed | Pull, metadata-only and retired-format refusal tests; PTY corrupt-transfer retention test |
| AC-007 machine streams | **OPEN** | Healthy-log paths pass, but a failed diagnostic write still reaches JSON stderr (FIND-TASK-002-1) |
| AC-008 SDK observe/verify recipes | **OPEN** | Three-SDK support-desk journeys pass, but the published Rust `serve` skips the row drain on telemetry failure (FIND-TASK-003-4) |
| AC-009 agent references | Closed | Generated `llms-full` docs tests, `docs:check` |
| AC-010 supported targets | **OPEN** | Source conformance only; Linux x86_64 execution only (FIND-TASK-002-4) |
| AC-011 deterministic aggregate | **OPEN** | Scoped lanes passed; no final `mise run gate` and no final change review recorded |
| AC-012 live agent readiness | Unreviewed | Live run on `47f5db244` passed for both agents with server evidence IDs; TASK-004 never task-reviewed |

## Open at completion

The owner closed this change with these items unresolved. They are carried
forward, not accepted:

- **FIND-TASK-002-1** (code): in JSON mode, `tracing-subscriber`'s default
  internal-error fallback prints raw prose to stderr when a write to the
  opened diagnostic log fails. Owner: `wyrd-cli` `src/lib.rs`
  (`install_tracing`).
- **FIND-TASK-002-3** (code, rustdoc gate): the `run_cli_json` docs are
  attached to `run_cli_logged` in `tests/card_lifecycle.rs`, and
  `install_tracing` does not document its `.init()` panic.
- **FIND-TASK-003-4** (code): `SupportDesk::serve` in
  `examples/support-desk-app/rust/lib.rs` returns on `telemetry.shutdown()?`
  before `state.shutdown()`. The published Rust recipe can lose buffered
  ticket rows that the independent writer could still drain.
- **FIND-TASK-002-4** (evidence): no Linux arm64 or macOS arm64 execution.
- **FIND-TASK-003-3** (evidence): no compatible published-artifact install
  run. Published `wyrd-ai` artifacts were 0.1.1 as of 2026-10-18, and the
  quickstart stays `status: partial`.
- TASK-004 was not task-reviewed, and no final integrated change review or
  `gate` run took place.

The bounded, unimplemented fix for the three code findings is
`changes/active/agent-readiness/review/TASK-001-003-review-20261019-02/TASK-001-003-R2-machine-stream-and-shutdown-closure.md`.
It is preserved, with the full spec, task and review packet, in git history at
commit `a7feddd16`.

## Links

- [AGENTS.md](../../../AGENTS.md) (supported targets),
  [Wyrd design](../../../architecture/wyrd-design.md),
  [doctrine](../../../architecture/wyrd-doctrine.mdx)
- Docs: [setup](../../../docs/src/content/docs/get-started/setup.md),
  [quickstart](../../../docs/src/content/docs/get-started/quickstart.md),
  [declare and inspect](../../../docs/src/content/docs/build-and-verify/declare/declare-and-inspect.md),
  [record a Run](../../../docs/src/content/docs/build-and-verify/observe/record-a-run-and-observations.md),
  [connect through MCP](../../../docs/src/content/docs/connect/connect-an-agent-through-mcp.md),
  [MCP tools](../../../docs/src/content/docs/reference/mcp-tools.md),
  [errors](../../../docs/src/content/docs/api/errors.md),
  [llms generator](../../../docs/scripts/generate_llms_txt.py)
- Code owners: [CLI boundary](../../../crates/wyrd/wyrd-cli/src/lib.rs),
  [Card commands](../../../crates/wyrd/wyrd-cli/src/card.rs),
  [dev server](../../../crates/wyrd/wyrd-cli/src/server/dev.rs),
  [MCP install](../../../crates/wyrd/wyrd-cli/src/mcp/mod.rs),
  [MCP guide](../../../crates/wyrd/wyrd-server/src/mcp/guide.rs),
  [Cards handle](../../../crates/shared/wyrd-client/src/cards/handle.rs),
  [bundle loading](../../../crates/shared/wyrd-client/src/state.rs),
  [error catalog](../../../crates/wyrd-spec/src/error.rs),
  [declaration migration](../../../crates/wyrd/wyrd-sql/migrations/20260601000014_card_declaration_operations.sql),
  [support-desk app](../../../examples/support-desk-app)
- Tests: [CLI lifecycle](../../../crates/wyrd/wyrd-cli/tests/card_lifecycle.rs),
  declare/pull journeys in [Rust](../../../sdks/wyrd-sdk-rust/tests/integration/declare_and_pull.rs),
  [Python](../../../sdks/wyrd-sdk-python/tests/integration/test_declare_and_pull.py) and
  [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/declare-and-pull.test.ts),
  [published quickstart journey](../../../sdks/wyrd-sdk-python/tests/integration/test_support_desk.py)
- Live lane: [`mise.toml`](../../../mise.toml) task
  `test:agent-readiness:smoke:live`,
  [harness](../../../sdks/wyrd-sdk-python/tests/agent_readiness/live.py),
  [observer](../../../sdks/wyrd-sdk-python/tests/agent_readiness/observe.py),
  [unit tests](../../../sdks/wyrd-sdk-python/tests/unit/test_agent_readiness.py)
