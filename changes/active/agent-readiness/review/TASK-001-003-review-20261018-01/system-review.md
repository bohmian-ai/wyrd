# Independent system-resilience review

Result: **FAIL**. Proposed findings: **SYS-001**, **SYS-002**.

## Subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
Base: `1b962bb00`; candidate: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3`.
HEAD remained the candidate while reviewing. The subject is the cumulative TASK-001–003 change, not TASK-004 or a latest-fix-only comparison. Source was not edited. No other review report or implementation completion message was read.

Governing sources: supplied AGENTS.md, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/operations/deployment-and-release.md`, spec-driven-development and maintainer references, approved `changes/active/agent-readiness/spec.md` revision 2 and its three tasks. Relevant obligations are REQ-002–008, INV-001/002/003/006, AC-002–010. No CodeGraph directory exists. The review examined the cumulative changed-surface inventory and expanded the material runtime paths below; it does not independently establish every generated documentation line or every language's complete journey result.

## Actual deployment and process topology

| Path | Owners and process/dependency boundary | Capability and failure boundary |
|---|---|---|
| Local setup | `server/dev.rs::DevArgs::run` owns `DevServer`; `DevServer::command` creates kill-on-drop children for installed `wyrd-server migrate`, `setup`, and the foreground serving process. PostgreSQL is external; local storage/Bifrost roots and protected client configuration are persisted. | One person/harness-owned CLI owns one local server. Ending setup settles that child; this is deliberately not a production supervisor or detached daemon. |
| Serving | `wyrd-server/src/app/{server,supervise,shutdown}.rs`, boot, HTTP router and MCP handler share one server process; role owners serve HTTP/gRPC and Bifrost capabilities. `supervise.rs` distinguishes process-terminal transport failures from restartable capability workers. | The candidate does not replace that supervision or turn a Card validation/transport error into a shared-server panic. A server-process exit loses local serving capacity until the owner reruns setup. |
| Writes | CLI/three SDKs → `Cards::declare` → saga prepare/submit/upload/complete/ensure_active → authenticated `/v1/cards` → server service, tenant transaction, object storage, lifecycle reconciliation. | Request/saga errors propagate to the caller. Committed Cards/artifacts can survive cancellation; this is not a transaction over the entire external workflow. Authorization audit remains on the non-blocking process outbox. |
| Reads and bundles | CLI get/pull and SDK equivalents → shared Cards owner. MCP merged get dispatches either exact ref or kind/UID to existing audited reads. Pull graph reads → sibling staging directory → bundle workspace publication → local WyrdState validation. | Read/refusal errors stay at the request. A failed materialization does not publish a partial runtime bundle. The old bundle remains until promotion; cancellation during awaited transfer can leave unpublished staging, as documented by `CardGraphPuller::pull`. |
| Agent connection | Selected host file launches `wyrd mcp proxy`; `proxy.rs::run` resolves saved client configuration, acquires its access token, uses `WyrdMcpHttpClient`, and bridges host stdio to `/mcp`. | Host owns proxy lifetime; proxy owns no server lifetime. Authentication/refusal/replay remains shared-client behavior. Instructions are copied from the upstream initialize result; calls forward the server result or a transport/protocol error. |
| Application evidence | Example app loads the complete bundle, starts buffered Bifrost/telemetry, opens Runs, and flushes on app shutdown; server outbox/Scribe owns retained evidence. | App death can lose client buffers; abrupt server death can lose unacknowledged outbox writes. Quickstart explicitly explains buffering and app-death loss and asks for retained evidence rather than treating an execution ID as a verdict. |

## Failure and recovery assessment

| Selected credible failure | Source evidence and affected capabilities | State, recovery and proof assessment |
|---|---|---|
| Missing input / occupied HTTP endpoint | `DevArgs::resolve` runs before install/migrate/provision; `DevServer::start` reserves the listener before migration. `Diagnostics::locate` attaches log location to stage errors. | Refuses setup without replacing the saved deployment. TASK-002 evidence and `pg_tests::server_dev_first_run_is_client_ready` cover JSON missing-input and listener refusals; the latter uses a real occupied listener. |
| Release network unavailable | `ServerInstaller::install` returns its catalog error before publishing a new verified installation; `run_cli_code` prints it through `CliBoundaryError`. | Prior installation stays intact. Existing signed-release tests cover preserved installation on failure. Independent installed-CLI offline-proxy probe below reveals machine-stream contamination, SYS-001. No production-server availability change follows an installer request failure. |
| Database / migration or setup interruption | `DevServer::migrate`, `provision`, `save_settings`, `save_tenant`; outer `tokio::select!` observes interruption and then calls `stop`. Setup output containing credentials is captured, not attached to the diagnostic file. | Committed database operations remain; rerun uses the same database/config and never silently rotates an active tenant's key. The existing partial-failure/repeat journey exercises saved-state recovery, including lost-key refusal. JSON changes preserve this owner rather than adding another bootstrap state machine. |
| Foreground server exits after readiness | `run` prints `DevReady` only after saved-client proof and gateway/MCP steps, then awaits `Self::exited(child)`. Every unsolicited exit returns a ServerDev error; `DevArgs::run` settles the child and `run_cli_code` emits a nonzero error. | The single prior ready result is retained; a later exit becomes a separate stderr failure. Saved config/database/storage remain for an explicit rerun. Source path is sound, but the changed JSON journey only exercises orderly interruption and pre-ready refusal, not forced post-ready child death. This is a verification limit, not a separately inferred runtime defect. |
| SIGINT/SIGTERM and drain overrun | `interruption`, `DevArgs::run`, `DevServer::stop`; child command uses `kill_on_drop(true)`. `stop` signals TERM, waits up to 45 seconds, then kills/reaps and reports shutdown failure. | Intentional interruption returns 130; the new JSON path logs its notice instead of appending prose. The test checks listener closure and no extra machine output after normal interrupt. Hard parent SIGKILL is not a graceful-shutdown guarantee; no stronger guarantee is required or proposed. |
| MCP host config refusal / upstream outage | `McpInstallReport::into_result`, `offer_local`, `DevServer::run`; `proxy.rs::{run,upstream_error}`. | Independent install returns a report containing every selected outcome and does not modify refused host files. Setup's selected-host failure terminates its owned local child, matching the pre-existing offer-local refusal boundary. A later proxy transport error is forwarded to that host request, not used to restart or crash the shared server. Saved credentials and existing installed host entries remain reusable. |
| Declaration upload/completion failure or cancellation | `cards/saga/{mod,upload,complete}.rs`; four bounded upload transfers settle before returning the first failure; completion merges only the matching returned outcome; ensure_active refuses non-active success. Server service retains lifecycle/reconciliation ownership. | No success receipt is fabricated. Earlier commits/partially uploaded Cards can survive. Server reconciliation and existing keyed transport retries remain the recovery mechanism; the candidate renames those durable owners in place under the explicit no-mixed-version authority. No client abort/rollback mechanism or service-wide crash was introduced. |
| Pull dependency failure / corrupted artifact / publication failure | `CardGraphPuller::pull`, `BundleWriter`, `BundleWorkspace::{publish,restore_after_failure}`, `WyrdState::from_path`. | Resolution precedes staging; a returned write error aborts unpublished staging. Failed promotion restores the backup or returns an explicit rollback failure with backup location. Unit tests cover promotion/rollback/cleanup failure; CLI/SDK journeys cover complete and metadata-only results and local runtime refusal. A process kill between two filesystem renames is not promoted into a new crash-atomic replacement promise; this publication mechanism is inherited. SYS-002 concerns presentation during transfer, not integrity. |

No changed retry or readiness loop introduces unbounded fault amplification: ready polling keeps the existing two-minute total budget with two-second requests, artifact upload concurrency remains four, and production worker recovery is unchanged.

## Material proposed findings

### SYS-001 — Configured tracing breaks JSON error streams on dependency failure

Classification: **INCORRECT**. Severity: medium. Violated obligations: REQ-006, AC-007, TASK-002 scenario 1 and its diagnostic-routing constraint.

Location: `crates/wyrd/wyrd-cli/src/main.rs:9–21` (especially `try_from_default_env` and the unconditional stderr writer).

The changed entry point sets only the *fallback* filter to `off` for JSON. Any valid `RUST_LOG` wins and all CLI/dependency diagnostics still target stderr. `server/dev.rs::Diagnostics` redirects explicit notices and the serving child's output, but does not own this already-installed parent subscriber. The same entry point serves install, dev, declare, get and pull, so fixing individual command error renderers cannot correct the producer.

Reachable evidence: ran the existing local compiled CLI with the release request forced through a closed loopback proxy:

```sh
RUST_LOG=debug HTTPS_PROXY=http://127.0.0.1:1 target/debug/wyrd server install --format json
```

It exited 69 and emitted rustls certificate diagnostics and reqwest/hyper connection diagnostics followed by the `WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE` JSON document. Representative preceding line: `DEBUG reqwest::connect: proxy(http://127.0.0.1:1/) intercepts 'Some("api.github.com")'`. Whole stderr therefore cannot deserialize as the promised single JSON document. No database or release-server availability was needed. This local binary probe was not a rebuild; the immutable source independently proves the same writer/filter path.

Observable system consequence: a machine caller recovering from a genuine dependency outage gets an unparseable error stream instead of its actionable catalog failure. For `server dev`, early parent debug logging can also make the documented background readiness loop see nonempty error.json before readiness, despite setup still running. The journey explicitly removes RUST_LOG in its fixture (`tests/pg_tests.rs:114`), so its clean capture does not cover this path.

Required correction: select diagnostic output at the shared CLI entry/presentation boundary so a valid RUST_LOG never sends diagnostics to either machine stream in JSON mode. Reuse/extend the existing diagnostic-log destination policy, retaining requested diagnostics in that destination and preserving command error documents, readiness timing, saved state, and human-mode logging. Do not solve this by discarding requested logs, stripping lines in consumers, adding a second startup owner, or weakening the missing-input contract.

Focused closure proof: run the compiled CLI with RUST_LOG=debug and a deterministic closed proxy for `server install --format json`; assert nonzero exit, empty stdout, whole-stderr JSON with the existing release-unavailable code, and diagnostics in its designated log. Retain the normal `server dev` JSON readiness/interrupt assertions with parent tracing enabled as a regression check.

### SYS-002 — Complete JSON pulls still draw artifact progress onto stdout on a terminal

Classification: **INCORRECT**. Severity: medium. Violated obligations: REQ-006, AC-007, TASK-002 scenario 1 (no progress corruption).

Changed locations: `crates/wyrd/wyrd-cli/src/card.rs:498–506`; `crates/shared/wyrd-client/src/cards/pull/bundle.rs:68–75`. Producer: `cards/download_progress.rs:24–25`.

`dispatch_pull` uses args.format only after awaiting pull_bundle. The latter reaches Cards::pull with only BundleMode, and BundleWriter unconditionally creates DownloadProgressDisplay::stdout for complete mode. Each authorized artifact transfer calls display.start/set_position, drawing human text and terminal control bytes before the final JSON result. The existing indicatif 0.18.6 implementation (`draw_target.rs::stdout/term`) draws to buffered stdout when it is a terminal, and hides drawing for non-terminal captures. Thus piped journey output being valid JSON does not prove the terminal-backed invocation used by a coding-agent PTY is clean. Metadata-only and artifact-free cases do not expose the defect; a complete pull with a nontrivial artifact does.

Observable consequence: `wyrd pull ... --format json` can publish a correct bundle while returning stdout that is not one JSON document; a failed transfer can leave only progress bytes on stdout where no success output is permitted. This affects machine result/error interpretation, without corrupting the published bundle or requiring a shared-server restart.

Required correction: stop unconditional human progress rendering inside the shared pull workflow for the machine-facing path. Reuse the existing download/display mechanism with a non-stream diagnostic/hidden target; removing automatic terminal progress from this path is sufficient if no format-aware presentation is retained. Keep verified transfers, staging, rollback and the final text/JSON summaries unchanged. No new transport, storage engine or public workflow is needed.

Focused closure proof: capture stdout through a PTY for a complete JSON pull with a real nonempty artifact and assert the raw output contains only the JSON summary (allowing normal terminal newline translation), without bar text/control sequences. Repeat with an artifact-transfer refusal to assert no stdout success/progress and one stderr catalog error; preserve the prior destination. Existing piped complete/metadata journeys remain relevant but cannot alone close this case.

## Verification limits and disposition

The independent execution was the small installed-CLI network-failure probe; no Cargo suite, database environment, live agent, new synthetic host load or source mutation was run. Task evidence reports Linux x86_64 execution; Linux arm64/macOS arm64 recovery behavior was not executed here. The complete deterministic gate and final integrated journeys belong to change review, as the orchestrator specified. Source review supports preserved foreground ownership, request-level declaration/pull errors, shared-client MCP authentication and restart semantics; it cannot substitute for unexecuted fault journeys or other-target execution.

**FAIL** for SYS-001 and SYS-002. Both corrections belong to output/presentation ownership and must preserve the existing foreground lifecycle and diagnostic-log contract.
