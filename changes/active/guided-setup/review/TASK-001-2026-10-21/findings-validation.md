# TASK-001 independent findings validation

Subject: base `ebd75dc224854413d17cb06b70da8181b78cde6b`, candidate `32fbc264adccaf1160894769d5501fbe441259a5`. Approved authority: `changes/active/guided-setup/spec.md`, revision 5; original task: `changes/active/guided-setup/tasks/TASK-001-guided-setup-and-asgi.md`. This is the first review, with no prior FIND IDs. HEAD remained the candidate. This report selects findings and corrections, not the orchestrator's verdict.

## Independent scope and evidence

Read the task-review skill in full, immutable subject, claim comparison, both task reports, repository, maintainer, system, concurrency, protocol and focused follow-up reports. Read the complete cumulative change in bounded surface segments, including rename-aware bootstrap diff, source/example/test changes, generated documentation and dependency-lock changes. CodeGraph preceded source navigation. Read applicable root/Python/TypeScript/docs rules and relevant change-workflow, Rust, errors, maintainer, agent-surface, architecture-map/client/framework, Bifrost system-boundary, credential-persistence and local-release authorities. The candidate's implementation evidence is a claim, not fresh validation.

Expanded source tracing included the full adapter and its lifecycle/diagnostic callers, native Python state startup/shutdown, native Run enter/exit and existing correlation owner; shared BifrostLifecycle shutdown; complete host launch match, host config detection/installation, connected outcomes, handoff and init caller; Setup/Quickstart/shared prompt and manual journey/helper; schema implementation and sibling projection; all rustdoc correction-target bodies. Existing worker and telemetry tests were read. The CLI, proxy and server reuse existing bootstrap, credential and transport owners; no replacement workflow is necessary.

Two fresh read-only probes imported the actual public middleware through `mise exec -- uv run --no-sync --directory sdks/wyrd-sdk-python python`. A generic app sending startup.complete then receiving shutdown and raising recorded `start, final` without `drain`, while preserving its RuntimeError. A post-receive startup exception containing synthetic DSN/token material appeared verbatim in the adapter-generated startup.failed message. These establish local control-flow/projection failures, not real-server persistence. No Cargo, formatter, source mutation, dependency install or live agent was run.

## Every proposal's disposition

| Source proposal | Disposition | Independent evidence and correction adjudication |
| --- | --- | --- |
| BREV-001 | REVISED → FIND-TASK-001-1 | `_lifespan:143–147` skips the only final drain when an established app lifecycle raises; independently reproduced. Retain existing state shutdown. Limit cancellation proof to a controlled established-lifecycle interruption and a cleanup attempt; no guaranteed delivery after deadline or cancellation during an in-flight startup thread. |
| INVREV-001 | REJECTED | `_lifespan:153–162` does suppress a pre-receive exception, but its cause is indistinguishable from unsupported lifespan under standard ASGI. Turning all such exceptions into startup.failed removes supported generic-ASGI compatibility. No bounded correction is established; details below. |
| INVREV-002 | REVISED → FIND-TASK-001-1 | Same producer and skipped lifecycle owner as BREV-001. Reuse one terminal cleanup boundary; do not create per-consumer findings. Preserve the primary app failure and any already emitted terminal message. |
| SYSTEM-001 | CONFIRMED → FIND-TASK-001-1 | A normal graceful shutdown can complete the server's app-error path without attempting SDK drain. The correction stays in the ASGI worker, never the shared Wyrd server or its supervisor. |
| CONC-001 | REVISED → FIND-TASK-001-1 | Same established-lifecycle gap. A single controlled cancellation can also bypass `except Exception`; preserve propagation and attempt existing cleanup where the task can still execute. Reject implied hard-timeout delivery, uncancellable startup, new retry/supervision or new timeout policy. |
| BREV-002 | REVISED → FIND-TASK-001-2 | `_diagnostic:25–29` creates unsanitized Wyrd-owned output. Reject its bundled requirement to rewrite application-owned startup.failed/shutdown.failed messages. The application owns those diagnostics; REQ-023 requires preserving its failure. |
| INVREV-003 | CONFIRMED → FIND-TASK-001-2 | All newly generated lifecycle messages/logs use the same unrestricted fallback. Safe canonical Wyrd fields and the established missing-extra diagnostic suffice; a secret-string blacklist does not. |
| BREV-003 | CONFIRMED → FIND-TASK-001-3 | Manual tab only links Quickstart; neither page publishes the durable query/Run-identification operation. `test_guided_manual_setup` obtains SQL from the agent prompt and supplies the operation through `stored_verdicts`, so its passing result does not close published manual instructions. |
| PROTOCOL-001 | REVISED → FIND-TASK-001-4 | Copilot/Pi omissions are real. Tracing every sibling and current primary docs also establishes VS Code, Cursor and Hermes prompt launches. All are one incomplete match, not independent new features. Details below. |
| REPO-001 | CONFIRMED → FIND-TASK-001-5 | Changed `Spec::json_schema` method lacks item docs, new `mcp::tests` module lacks item docs, and new `walk` omits panic docs. Explicit changed-item rustdoc gate applies; batch corrections without runtime changes. |
| MAINT-001 | CONFIRMED → FIND-TASK-001-5 | Full helper has filesystem `expect` calls and only one summary line; same `walk` omission. |
| INVREV-004 | CONFIRMED → FIND-TASK-001-5 | Same `walk` omission, not another finding. |
| Focused follow-up diagnostic narrowing | CONFIRMED | New Wyrd diagnostic projection is owned here; merely forwarded application output is not. Preserve the existing application-refusal assertion. |
| Focused follow-up pre-receive uncertainty | RESOLVED; no finding | Primary ASGI protocol establishes compatibility and provides no reliable marker distinguishing those exceptions. Reject universal refusal remediation; no new policy needs choosing to close retained findings. |

The reports propose no other material findings. General standards PASS rows do not defeat the specific demonstrated diagnostic defect. Broad Manual and handoff PASS rows are revised by the actual published path and all host branches. The unchanged utoipa `Spec` oneOf projection is not a new defect established by this task; do not bundle an OpenAPI refactor into these corrections. Placement/wording preferences carry no separate blocker. Missing rustdoc remains an explicit exception under root rules.

### Unsupported lifespan and failure ownership

The primary [ASGI lifespan specification](https://asgi.readthedocs.io/en/stable/specs/lifespan.html), checked 2026-10-21, defines exception-based unsupported-lifespan compatibility and explicit startup.failed for deliberate refusal. It has no capability marker distinguishing an HTTP-only callable raising on a lifespan scope from a broken lazy app before its first receive. REQ-021 supports generic ASGI wrapping; REQ-022/023 must be read with that conventional protocol. An intentionally emitted application failure and an adapter-owned state-startup refusal remain failures. The early arbitrary-exception probe proves suppression, not that the middleware can infer an application startup refusal.

Accordingly INVREV-001 does not require `SPEC_REVISION_REQUIRED` for this task: its demanded universal exception interpretation is outside the established standard behavior, rather than a decision necessary to implement an approved feature. If a future product asks for a stricter compatibility policy, that would need explicit authority. Do not invent an option, exception taxonomy or capability probe now. This disposition does not excuse skipping cleanup after an established lifecycle's abnormal exit.

## Final deduplicated ledger

### FIND-TASK-001-1 — REVISED / INCORRECT: established application lifespan can exit without final drain

Sources: BREV-001, INVREV-002, SYSTEM-001, CONC-001. Obligations: REQ-021/023, AC-014 and original task scenario 4.

Location: `sdks/wyrd-sdk-python/python/wyrd/integrations/asgi/_middleware.py:119–162`, specifically the immediate reraise at 145–146; terminal-message-only drain at 133–141.

Producer → consumer: `_start` starts the state's writer/provider; the app sends startup.complete, setting `replied`; on shutdown it can queue final evidence and raise before any terminal message. The handler reraises without `_drain`. Native `PyWyrdState::shutdown` (`src/state/mod.rs:337`) is the sole relevant telemetry-flush/native-drain projection; shared `BifrostLifecycle::shutdown` (`crates/shared/wyrd-client/src/observe/lifecycle.rs:132`) closes on success and retains the same writer after ambiguous drain failure. `Run.__exit__` only restores correlation and cannot substitute for shutdown. Explicit application failure/complete messages reach drain already; the unsupported-lifespan fallback is another existing sibling path. The adapter's full caller is its lifespan branch, used both by add_middleware and direct wrapping.

Consequence: ordinary application cleanup failure can abandon queued request/final-app evidence and spans with no drain attempt. Primary exception propagation alone is insufficient.

Smallest complete correction: retain the existing off-loop `_drain`/`state.shutdown` owner and give a successfully started, established lifespan terminal cleanup on abnormal app exit too. Keep app teardown before drain; do not drain again after a terminal-message path already attempted it. Preserve the primary exception or cancellation, preserve existing explicit application failure messages, and do not report completion for an actual failed phase. If a secondary drain fails, expose/log its safe Wyrd diagnostic without replacing the primary app failure. An appropriate phase failure response may accompany an otherwise unanswered protocol phase; no duplicate terminal reply after one was already sent. This is adapter protocol adaptation, not new native lifecycle behavior.

Cancellation scope: a controlled cancellation while awaiting an established app can attempt the same bounded cleanup and then propagate. There is no requirement to shield forever, wait beyond the host budget, stop a Python worker thread, guarantee completion on a repeated cancellation, or guarantee delivery after timeout/hard kill. Do not broaden correction to cancellation during `_start` or add a supervisor/retry policy without evidence and authority.

Closure: extend existing lifespan proof with a plain ASGI app that sends startup.complete, consumes shutdown, queues real final evidence, then raises before replying. Assert original failure observable, successful state closure, final evidence queryable and loop responsive. Cover secondary drain failure preserving the original error and a small controlled established-lifecycle cancellation cleanup attempt. Existing successful and explicit-message cases still drain once, in order. The existing test driver must observe task termination as well as messages so the new negative case cannot hang; extend it minimally, without a new harness. Run C5 below and the directly affected existing telemetry/provider proof C7; cancellation can be a small Python-only case within the existing module.

### FIND-TASK-001-2 — REVISED / VIOLATION: adapter-created diagnostics publish arbitrary exception bodies

Sources: BREV-002, INVREV-003, focused follow-up. Obligations: REQ-022, AC-014; errors guide public-boundary rule.

Location: `_middleware.py:25–29`; all consumers at 107, 115–116, 125 and 151.

Producer → consumer: native writer/telemetry or raw post-receive app exceptions enter `_diagnostic`; cleanup/drain uses it in logging and failure responses too. The fallback interpolates arbitrary exception text, including multiline DSNs/tokens. The direct probe confirms this. Existing WyrdError structured projection is the trusted source for code/canonical public fields; the arbitrary fallback is not. Existing `wyrd.otel._EXPORTER_EXTRA` at 199–202 is the owner of known missing-extra guidance and is raised by its import guards. Structured field-redaction helpers cannot safely scrub arbitrary text and are not needed.

Consequence: the newly promised scrubbed startup boundary can expose secret material through Wyrd-generated ASGI messages and logs.

Smallest complete correction: change the existing diagnostic projection once: canonical Wyrd code plus safe public diagnostic information; static phase/classification text for unexpected exception bodies; preserve the specific existing missing-otel-extra remedy by recognizing/reusing that known diagnostic. Do not treat every ImportError as a missing extra, publish arbitrary `str(error)`/traceback text, add regex blacklists, add a new error catalog or reimplement SDK exception projection. Preserve the original app exception where already propagated. Explicit app-owned startup.failed/shutdown.failed messages remain their original messages and failures; altering them is rejected adjacent scope.

Closure: small fault cases with synthetic DSN, token and newline across startup and cleanup/drain show none enters adapter-created messages/logs; Wyrd refusals retain their code; the actual missing-extra path retains `pip install 'wyrd-ai[otel]'`; explicit app failure text still passes through. Use existing public middleware test module, C5, `py:lints`, `py:typecheck`; no real secrets or corpus/redaction framework.

### FIND-TASK-001-3 — CONFIRMED / MISSING: published Manual path omits durable readback and Run identification

Source: BREV-003. Obligations: REQ-027, AC-003/015; shared prompt steps 6–7.

Locations: `docs/src/content/docs/get-started/setup.md:137–139`; linked `get-started/quickstart.md` full procedure; `sdks/wyrd-sdk-python/tests/integration/test_local_development.py:1229–1300` (`stored_verdicts`, `test_guided_manual_setup`). Generated `docs/public/llms-full.txt` reproduces the omission.

Producer → consumer: new Manual tab promises equivalent steps but directs the reader only to Quickstart. Quickstart's examples print exact Card identities and synchronous verdicts, then teach version drift; they neither identify the Run nor read durable results. The parameterized test gets SQL from the agent prompt and does MCP reads in a helper invisible in the manual path. All three language runners share that helper, so this is one omitted published completion, not three SDK defects.

Consequence: following the Manual instructions cannot yield the documented durable confirmation and Run report; the green journey silently completes an unpublished step.

Smallest complete correction: complete the existing published manual path with the equivalent existing Card lookup/result read and same-run passed/failed confirmation, including Run identification and publication delay/failure guidance. Reuse existing MCP read/query surfaces and the prompt's SQL semantics; no setup executor, new SDK loader, writer or new CLI contract. Keep one coherent language-neutral readback after the existing three language examples, accessible directly from Manual. Update the focused journey to execute/check the published completion operation instead of depending only on an unpublished helper. Generate docs from source. Do not expand into unrelated tutorial redesign or TASK-002 deployment content.

Closure: C4 runs each language and the actual documented completion against the local server, retrieving the matching pair under one Run ID; source/readability inspection establishes the reader can perform that operation. Run docs generation plus command/link/content checks for edited pages; no full docs aggregate required at remediation.

### FIND-TASK-001-4 — REVISED / MISSING: host launch match omits supported initial-prompt commands

Source: PROTOCOL-001, expanded by independent sibling tracing. Obligation: REQ-002 and task scenario 2.

Location: `crates/wyrd/wyrd-cli/src/mcp/hosts.rs:93–101`; consumer `mcp/mod.rs:239–260`; init caller `init.rs:854–865`. Current tests at `hosts.rs:781` and `tests/pg_tests.rs:1389` cover only Codex/Claude and incorrectly use Cursor as a known unsupported fallback.

Producer → consumer: all seven McpHost variants are installable through the same existing installer. Success becomes connected Added/Updated/Unchanged; `print_handoff` then calls `launch`, whose single categorical None arm includes five hosts that accept a command-line prompt. Nothing in the enum/install contract pins old host versions or limits handoff to terminal-only interactive clients. Installation paths are ordinary Codex/Claude, Copilot `mcp-config.json`, VS Code user `mcp.json`, Cursor `~/.cursor/mcp.json`, Pi agent `mcp.json`, Hermes active-profile `config.yaml`; handoff needs no credential/config change.

Current primary references independently checked on 2026-10-21:

| Existing variant | Required existing syntax/mechanism | Primary evidence |
| --- | --- | --- |
| Codex | `codex '<prompt>'` | Existing implemented branch and literal-shell proof; retained. |
| ClaudeCode | `claude '<prompt>'` | Existing implemented branch and literal-shell proof; retained. |
| CopilotCli | `copilot -i '<prompt>'` | [GitHub command reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference) documents an initial interactive prompt. |
| Pi | `pi '<prompt>'` | [Pi CLI reference](https://raw.githubusercontent.com/earendil-works/pi/main/packages/coding-agent/docs/cli.md) documents positional initial messages and default terminal UI. |
| Vscode | `code chat '<prompt>'` | [VS Code CLI](https://code.visualstudio.com/docs/configure/command-line) documents chat startup in the current working directory with a supplied prompt and default agent mode. This is the configured VS Code host itself. |
| Cursor | `agent '<prompt>'` | [Cursor parameters](https://prod.cursor.com/docs/cli/reference/parameters) and [Cursor MCP](https://prod.cursor.com/docs/cli/mcp) document prompt invocation and shared editor/CLI MCP configuration. No separate Wyrd host/config is needed. |
| Hermes | `hermes chat -q '<prompt>'` | [Hermes CLI](https://hermes-agent.nousresearch.com/docs/user-guide/cli) documents the supplied-query invocation. REQ-002 does not limit command handoff to interactive-only sessions; one-shot query mode is not grounds for categorical None. It does not require bypassing permissions or changing tools. |

Consequence: connected users on these supported hosts receive the paste fallback instead of the approved exact launch command. Paste remains possible but does not meet the required supported-command branch.

Smallest complete correction: extend the existing match with these documented prefixes and its existing literal single-argument quoting. Keep output-only behavior, current-directory semantics, existing host installation and foreground supervisor. No executable/version probe, capability registry, option, abstraction, new host kind or automatic agent launch. Retain fallback behavior for genuinely unsupported commands; do not invent one by treating a supported host as unsupported. Current old-installed-version compatibility is a documented verification limit, not grounds to omit the current supported syntax wholesale.

Closure: extend the existing parameterized/literal launch proof to every branch, including apostrophe, newline, dollar and backtick bytes; update the existing focused connected-host journey's selected outcomes and assertions so its Cursor fallback expectation no longer pins the omission. No paid agent needs launching. Run the exact host-unit selector and C2 below; do not run all bootstrap journeys for this bounded change.

### FIND-TASK-001-5 — CONFIRMED / VIOLATION: changed Rust items miss required rustdoc

Sources: REPO-001, MAINT-001, INVREV-004. Obligation: root AGENTS changed-item rustdoc gate and `agent_docs/rust.md`, explicitly retained as blocking by the task-review skill.

Exact sites and scope:

- `crates/wyrd-spec/src/envelope.rs:333`, changed `Spec::json_schema`: no method rustdoc; impl-level description at 324–327 does not document that item. Explain the variant-union schema contract at the method; leave behavior unchanged.
- `crates/wyrd-spec/src/mcp.rs:81`, new `mod tests`: no module rustdoc. Its documented child test does not document the module.
- `crates/wyrd/wyrd-cli/tests/cli.rs:111`, new `walk`: only a summary despite directory, entry and file-read `expect` calls. Add its panic contract and correct the same item's relative-path description (it currently returns full joined paths).

Full method/module/helper and caller bodies were inspected. `walk` serves only the offline snapshot closure and itself; no runtime design change is warranted. The consequence is failure of an explicit merge gate, even though this is otherwise documentation-only. Smallest correction is accurate rustdoc at these three changed sites, batched with the behavioral fixes. No source-wide sweep, new check or new runtime test. Source inspection and narrow Rust formatting/lint checks close it.

## Reconciled acceptance matrix

PASS means source and available evidence support this task-stage obligation, not that this validator reran the recorded journeys. C1–C8 refer to the exact original task commands; focused remediation closure is listed below.

| Obligation | Source/verification evidence | Reconciled result |
| --- | --- | --- |
| REQ-001; INV-008; AC-001 bootstrap rename, no alias, preserved inputs/settings/credential/gateway/supervision | Rename-aware init diff, parser/dispatcher, unchanged orchestration, TS signal wrapper; C1/C2 and retained renamed regressions | PASS, with full combined regressions reserved to change review |
| REQ-002 host handoff, literal args, no automatic launch | Existing print-only connected-host owner and quoting are correct; five supported launch branches omitted | FAIL, FIND-TASK-001-4 |
| REQ-003 offline prompt/conflicts | Exclusive flag, return before installer/config; C1 invalid-config/file/listener assertions | PASS |
| REQ-004 JSON one readiness object/foreground lifetime | DevReady setup_prompt; existing JSON output/lifetime unchanged; C2 | PASS |
| REQ-005 argument-free authenticated read prompt | Existing authenticated MCP edge, static descriptor/getter, proxy forwarding; C2/C3 refusal assertions | PASS |
| REQ-006; INV-007 release/single-source prompt | Shared Markdown plus release projection, CLI/JSON/MCP parity and docs embed; C2/C3/generated projection evidence | PASS |
| REQ-007/008 agent prompt supported-language/exact-release/install/file safety/verification/readback/report | Full prompt steps 1–7, existing examples and fixed-UID query; C4 proves APIs; deterministic live-agent execution excluded | PASS for prompt; manual publication separately fails |
| REQ-021 generic public pure-ASGI adapter | Public exports/stub, direct callable/add_middleware; ASGI/example journeys | PASS; standard unsupported-lifespan compatibility preserved |
| REQ-022 startup ordering/partial cleanup/provider safety/safe diagnostics | Existing state methods via thread; C5 writer/provider refusal checks; unrestricted new diagnostic fallback | FAIL, FIND-TASK-001-2; INVREV-001 rejected |
| REQ-023 final off-loop app-before-state shutdown/failure preservation | Normal/explicit-message tests and existing state owner work; raw established-app failure skips drain | FAIL, FIND-TASK-001-1 |
| REQ-024; AC-012 per-exchange isolated Run/alias/stream/exception/cancellation/passthrough | Full call context and native enter/exit, C6 negative/concurrent/storage assertions | PASS |
| REQ-025 instrumentation correlation/provider ownership | No HTTP span creation; existing OTel owner extended with ContextVar; C7 and existing observation cases | PASS |
| REQ-026; INV-009; AC-013 workers/portable lifecycle | Worker-local lifespan starts resources, actual two-worker test, enabled-lifespan and hard-limit docs | PASS by source and recorded Linux runtime evidence; default telemetry multiprocess/platform limits remain |
| REQ-027; AC-003 manual equivalent published completion | Setup Manual only links Quickstart; test performs unpublished query/Run readback | FAIL, FIND-TASK-001-3 |
| REQ-028 audience paths ≤5 ordered pages | Choose your path gives exactly three with 5/4/4 pages | PASS for task portion; deployment guide reserved TASK-002 |
| REQ-030/031; AC-015 integration shape/tested example | Docs AGENTS shape, FastAPI source embedding and test_fastapi_example, recorded docs subchecks | PASS for integration shape/example; learning completion fails under FIND-TASK-001-3 |
| REQ-032; AC-016 task portion | Public command rename, source/docs/example/wrapper changes; intentional historical/removed-command references only | PASS; unused trace constant belongs TASK-002 |
| INV-005 registration/load separation | Adapter accepts loaded state, example from_path, declaration only in setup/test | PASS |
| AC-014 combined startup/shutdown failures | C5 omits retained failure/projection gaps | FAIL, FIND-TASK-001-1/2 |
| Changed-item documentation | Three precise item omissions | FAIL, FIND-TASK-001-5 |
| Non-goals and required owner reuse | No second bootstrap, transport, registry, native lifecycle, writer/provider, HTTP instrumentation, auth redesign, framework discovery or hard-kill guarantee | PASS; retained corrections preserve these boundaries |
| TASK-002 requirements/combined integration | REQ-009–020/029/033–036 and related identities/refusals intentionally outside this task | Out of this task, neither acceptance nor failure inferred |

## Focused closure commands and limits

Use existing mise lanes and the original focused selectors; no new validation script or broad aggregate is needed. The remediation task should include only the affected commands, with Cargo coordinated by its caller.

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=mcp::hosts::tests::launch_preserves_the_setup_prompt_literally)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E "test(=pg_tests::guided_init_lifecycle_and_prompt)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_local_development.py::test_guided_manual_setup'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_asgi_integration.py::test_asgi_lifespan tests/integration/test_asgi_integration.py::test_asgi_telemetry_ownership'
mise run py:lints
mise run py:typecheck
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-cli --all-targets --all-features -- -D warnings
mise run docs:generate
mise run docs:check:commands
mise run docs:linkcheck
```

New narrowly scoped fault tests require their exact selectors as well if placed outside the existing lifespan test. For docs, source/projection inspection must verify the published operation and generated equality; existing docs content/unit checks can be selected for changed content. Codegen checks are required only if remediation changes contract-generation inputs; these findings require no wire change. Do not rerun full bootstrap/all-SDK/full-docs aggregates in remediation; those belong once at integrated change review.

I did not rerun supplied C1–C8, real Postgres journeys, Cargo/lints, docs generation or worker processes. Other review evidence records Python lints/stubs, docs command/generated-equality checks and 68 existing observation tests; this report does not claim those runs as its own. Existing runtime evidence remains Linux x86_64 only; macOS arm64/Linux arm64 and installed agent-version compatibility are unexercised. C8 uses telemetry=False; C7 supplies single-process managed-provider proof. No candidate mutation, missing authority or unresolved source-access blocker was found. No retained correction needs a new material decision or spec revision. Five bounded findings remain; ordinary lifespan teardown can currently lose queued evidence, and generated diagnostics can disclose synthetic-equivalent user secrets.
