# Protocol and setup-prompt boundary review

Result: **FAIL** — one proposed material finding, `PROTOCOL-001`.

Subject: base `ebd75dc224854413d17cb06b70da8181b78cde6b`, candidate
`32fbc264adccaf1160894769d5501fbe441259a5`. HEAD still identifies the candidate.
Only this report was written. No other discovery reports were read.

## Boundary and authority coverage

Reviewed the cumulative diff, including the CLI move and callers, MCP server
and proxy, shared schema and prompt, SDK/ASGI changes, docs and generated text,
examples, tests, and dependency/task wiring. Detailed domain tracing covers
the following boundary; this is not an independent ASGI lifecycle verdict.
CodeGraph was consulted before source navigation.

| Boundary | Governing authority | Source and consumer coverage |
| --- | --- | --- |
| Offline CLI/readiness projection | Approved spec revision 5 REQ-001–004, REQ-006–008; original TASK-001 scenarios 1–3; root AGENTS; agent_docs/agent-surfaces.md | `cli.rs`, `lib.rs`, `init.rs::InitArgs::run`, `DevServer::serve` readiness/handoff, `DevReady`, moved bootstrap diff, CLI and installed-bootstrap tests, TS CLI signal wrapper |
| Authenticated MCP prompt and forwarding | REQ-005–006, INV-007; Wyrd design Client model; security posture authenticated routes; agent-surfaces guide | `http/router.rs:99–113`, existing `require_authenticated` edge; `mcp/mod.rs::WyrdMcpHandler::{get_info,list_prompts,get_prompt}`; `guide.rs::{prompt_descriptor,prompt}`; CLI proxy::{run,list_prompts,get_prompt,upstream_error}; Python and Rust proxy journeys |
| Literal launch handoff | REQ-002, credential protection in REQ-008 and security posture local-persistence paragraph | `mcp/hosts.rs::McpHost::{ALL,launch,label}`, all seven enum branches, `McpInstallReport::print_handoff`, init connected-report flow, host quoting test and guided bootstrap journey |
| Shared prompt and docs | REQ-006–008; docs/AGENTS.md and nested Python/TS rules; architecture client model | `wyrd-spec/src/mcp.rs`, full `setup-prompt.md`, Setup CodeFromFile import and generated `llms-full.txt`, quickstart YAML/program consumers, `test_guided_manual_setup` and `stored_verdicts` |
| Schema correction | Root pure-contract/wire rules; agent-surfaces generated-schema rules; Wyrd design Client model | `envelope.rs::Spec` and both schemars/utoipa projections, `Card`, MCP `cards::descriptors` → `tool<GetCardArgs,GetCardResponse>` → `schema_of`, HTTP Card routes/GetCardResponse and served OpenAPI contract assertion |
| Preserved security boundary | Approved task bootstrap regression boundary; security posture local credential persistence and gateway protection; operations deployment/release local bundles | Complete move diff preserves installer verification, settings/key persistence, tenant provisioning, gateway and supervision; new handoff uses only static release text; no credential or host configuration is interpolated into launch text |

Also read the repository authority map, change workflow, agent_docs index,
Rust/errors/maintainer guides, applicable nested AGENTS, and task verification
claims. Existing owners are reused: host metadata owns host-specific launch
syntax, the existing proxy owns forwarding, and the existing HTTP edge owns
authentication. No second bootstrap, credential, prompt-execution, or registry
service was introduced.

## Boundary assessment

| Obligation | Evidence | Assessment |
| --- | --- | --- |
| Prompt reads require authentication, are available independently of write scope, and execute nothing | `/mcp` remains behind `require_authenticated`; new hooks return static text without caller-derived data or domain calls | PASS by source |
| MCP advertises one argument-free setup prompt; wrong names and supplied arguments are refused | `guide.rs:52–91`; absent/empty argument map is accepted and nonempty map receives protocol invalid params; proxy forwards the result/error | PASS by source and inspected C2/C3 test assertions |
| CLI and MCP use a release-labelled shared body | Both call `wyrd_spec::mcp::setup_prompt(env!("CARGO_PKG_VERSION"))`; readiness stores the same String; docs imports the shared body and identifies Wyrd 0.3.0 in the page's release/install context | PASS by source; docs embeds the body rather than the executable heading |
| Print-only mode avoids setup side effects and conflicts with setup options | Exclusive clap argument and early return before installer/configuration; C1 checks config, filesystem and listener purity | PASS by source; no test rerun here |
| Launch arguments preserve literal prompt and never execute hosts | Static executable prefix and POSIX single-quote escaping; printed by connected outcomes only; unit test uses hostile shell text and compares bytes | PASS for implemented branches |
| Every connected host accepting an initial CLI prompt receives its launch command | Host match excludes supported Copilot and Pi initial-prompt invocation | FAIL: PROTOCOL-001 |
| Setup instructions preserve file/language choices and confirm evidence with MCP reads | Manifest ambiguity stop, exact SDK version, conflicting-file stop, minimal Service/Verifier, fixed-UID SQL and same-run passed/failed check | PASS by source; inspected C4 exercises repository examples rather than external package installation |
| Schema correction reaches strict MCP Card-output validation | `Spec` schemars uses anyOf and every MCP GetCardResponse schema derives from it; runtime kind-directed deserialization remains unchanged | PASS for the changed MCP path |

The sibling HTTP OpenAPI `Spec` projection still uses `oneOf`
(`envelope.rs:263–283`), and `pg_openapi_contract.rs:1450–1458` still asserts
that existing shape. It was already so at the base; the candidate neither
changes the HTTP representation nor sends the new setup workflow through an
OpenAPI validator. This is not presented as a new regression or a requirement
to expand this task. The shared schemars correction also cannot bypass server
kind-directed validation: it changes an advertised union, not decoding,
permissions, or a write path.

## Material proposed finding

### PROTOCOL-001 — Supported initial-prompt hosts are sent to the paste fallback

- Classification: **MISSING**. Severity: medium.
- Violated obligation: approved REQ-002 requires an exact launch command for
  each connected host that accepts an initial prompt on its command line;
  TASK-001 scenario 2 retains that requirement.
- Exact changed location: `crates/wyrd/wyrd-cli/src/mcp/hosts.rs:97–98`.
- Producer and reachable consumer: `McpHost::launch` returns `None`
  unconditionally for `CopilotCli` and `Pi`. A successful explicit or
  interactive host installation produces a connected outcome; init invokes
  `McpInstallReport::print_handoff`, which maps that `None` to paste/retrieve
  instructions. Thus a person connecting these existing supported hosts never
  receives the promised launch command.
- Primary-source evidence, checked 2026-10-21: [GitHub's Copilot CLI command
  reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference)
  documents `-i PROMPT` / `--interactive=PROMPT` as starting an interactive
  session and executing that prompt (line 688 in the retrieved document).
  [Pi's own CLI reference](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/cli.md)
  documents positional messages as an initial prompt and the default terminal
  UI with terminal stdin/stdout (lines 3–31). These are current upstream
  capabilities, not a claim about every older installation. Wyrd's host
  variants impose no contrary release/version restriction and the new match
  does no version selection; its omission is categorical, including for a
  current supported host. No installed paid host was launched in this review.
- Sibling check: the same match owns Codex, Claude Code, Copilot CLI, VS Code,
  Cursor, Pi and Hermes. Codex/Claude receive commands; all other branches
  fall back. [Hermes' own CLI guide](https://hermes-agent.nousresearch.com/docs/user-guide/cli)
  also documents `hermes chat -q` for a single query, but this finding needs no
  new choice between interactive and one-shot Hermes behavior: Copilot and Pi
  already supply direct interactive counterparts to the implemented hosts.
  No claim about editor launch support is made.
- Observable consequence: this is an omitted approved handoff for existing
  supported host users, not a shell-injection claim. The paste fallback remains
  usable, but it does not satisfy the branch explicitly required for these
  hosts.
- Smallest testable correction: extend the existing `McpHost::launch` match
  for confirmed supported initial-prompt hosts, using their documented CLI
  syntax (`copilot -i` and `pi`) and the already implemented literal argument
  quoting. Preserve unsupported-host paste guidance, no automatic launch,
  foreground supervision, and secret-free output. No capability probe, new
  configuration, dependency or host abstraction is needed. Extend the current
  host launch assertion and existing guided handoff journey to cover these
  connected outcomes and literal argument bytes. The present tests exercise
  only Codex/Claude commands and Cursor fallback, so they cannot falsify this
  omission.

## Verification limits

This pass is read-only static review plus current official host CLI references.
No Cargo command, runtime server, paid coding agent, package installation,
Postgres journey, formatter or code generator was run. C1–C8 and format/lint/
codegen results were inspected as supplied claims and their relevant test
bodies were checked; this report does not claim they were independently rerun.
The recorded runtime platform coverage is Linux x86_64 only. macOS arm64,
Linux arm64 and installed host-version compatibility remain unexercised.
ASGI lifecycle correctness belongs to the separate scoped domain review.

No speculative protocol hardening or unrelated schema refactor is requested.
Risk retained by the candidate: supported Copilot/Pi users receive a manual
paste handoff where the approved change promises a launch command.
