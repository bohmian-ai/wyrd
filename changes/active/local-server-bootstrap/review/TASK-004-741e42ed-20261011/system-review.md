# TASK-004 independent system-resilience review

Result: **FAIL** — proposed finding `SYS-001` requires independent validation.

## Immutable subject

Repository: `/Users/stevenforrester/Documents/GitHub/local-server-bootstrap-task-004`.
Base `cb5656b2b913dbd88a8f6736bf4a0baaf998960c`; candidate
`741e42ed97ac3ff9638217bbcd4f0c0583e6d464`. HEAD remains the candidate.
Approved authority: `changes/active/local-server-bootstrap/spec.md` revision 7;
original task `tasks/TASK-004-local-dev-journey.md`, including final corrected
verification evidence and the startup/test-fixture cleanup. No prior findings
were supplied. This report examines cumulative runtime changes, rather than
only the cleanup commit. No production or test source was changed.

## Authority and source coverage

| Boundary | Governing authority | Inspected source and consumers |
|---|---|---|
| Foreground lifecycle, readiness, failure and retry | Spec REQ-004/005/007/010/015; AGENTS §§2,5,6,9,11; agent rules; architecture constraints; deployment-and-release migration/configuration/shutdown contracts | CLI `server/{dev,install,mod}.rs`; server `main.rs` migration/setup/default serving dispatch; `app/{shutdown,supervise}.rs`, configuration drain default |
| Durable partial progress and credentials | Spec REQ-008/009/010 and INV-004; Wyrd design administration planes and outbox loss contract; operations reliability-and-recovery | Server setup/root credential/provisioning call path; CLI `ProvisionedTenant`, disclosure, save and fresh-client verification; shared `GlobalConfig`, `CredentialsFile`, saved-login selection and token-cache writers |
| MCP selection, cancellation and host editing | Spec REQ-011/012/013/014; shared-owner and async rules | `mcp/{mod,hosts}.rs`, existing `InstallArgs`/`HostInstaller`, synchronous standalone installation, async local prompt and shared parser |
| Installed language-runtime signal ownership | AGENTS §§7,8,11; PyO3 and TypeScript boundary references | Python `src/cli.rs` and console dispatcher; TypeScript `src/cli.ts` and native shared dispatcher; installed-package checker |
| Credible process and fresh-client proof | Spec TASK-004 acceptance; testing-workflows, TESTING, spec-driven-development and maintainer references | CLI `tests/pg_tests.rs` and CLI target wiring; Python existing local-development integration module; Rust and TypeScript saved-client tests; focused mise lane; corrected task evidence |

The separate draft docsite range has documentation-only effects. It introduces
no deployed process, dependency, migration or restart path; current and draft
instructions preserve external Postgres and the attached foreground command.
No Bifrost engine or publication algorithm is changed in this candidate; its
ordinary server shutdown owners remain authoritative.

## Deployment and failure-path assessment

The deployed topology is one installed CLI process (native, CPython, or Node),
one retained normal `wyrd-server` serving child, transient server-owned
`migrate`/`setup` subprocesses, external PostgreSQL, and the ordinary local
storage/Bifrost roots. Clients and MCP proxies are independent processes using
the saved files. The CLI is not a database daemon or durable supervisor.

| Credible event | What stops, what survives, and recovery | Proof assessment |
|---|---|---|
| Interrupted/stalled first installation | Pinned interruption is polled alongside installation; dropping installer future removes private staging. Existing active installation remains authoritative. No serving child exists. | `server_dev_repeat_and_partial_failure_preserve_state` intercepts HTTPS through a stalled local proxy, sends SIGINT, and asserts exit 130 and no credential file. Live official release transfer remains untested. |
| Migration refusal or database failure | Server-owned one-off migration fails; CLI does not start serving and reports redacted guidance. Completed migration transactions remain, and retry goes through the same migration owner. | CLI scenario uses an underprivileged login and asserts migration refusal without DSN disclosure. Normal migration/startup recovery is additionally recorded in the Linux startup evidence. |
| Interruption during migration/setup | Commands have `kill_on_drop(true)`; cancellation drops the transient output future and then runs the retained serving child's stop path if it exists. Committed root/tenant/database state is preserved, with explicit recovery guidance rather than automatic key reissuance. | Source evidence. Dedicated migration/setup interruption tests were not recorded; no claim of transactional cancellation or key recovery through a rerun is inferred. |
| Serving child fails before readiness | `ready` checks `try_wait` between bounded HTTP probes; readiness budget is two minutes, requests have two-second ceilings. Failure settles the owned child. | Hardened missing-signing-key scenario asserts readiness failure and listener release. |
| Tenant key cannot be saved after provisioning | Server state survives; command explains explicit tenant-admin recovery. Retry sees active tenant and does not mint a replacement. The saved-key-before-endpoint write ordering retains a recoverable key if the later config write fails. | CLI partial-persistence scenario makes directory unwritable, asserts recovery guidance/no secret output, retries, asserts no key is reissued and listener is released. Shared-owner test preserves comments/key/token cache under retry. |
| Root cannot be disclosed to a terminal | Secret-free recover-root guidance is emitted, and tenant-key persistence continues. This avoids throwing away the tenant credential merely because the independent root disclosure failed. | No-terminal source path and successful controlling-terminal disclosure are exercised. Forced terminal-write failure is source-inspected only. |
| User interrupts an unanswered host prompt | Local AsyncFd terminal read yields, outer interruption cancels it, terminal descriptor drops, and retained child receives SIGTERM and is awaited. | Real controlling-terminal fixture asserts exit 130 and HTTP listener release. |
| User declines host setup | Existing config is untouched by host installation; server remains available to SDK callers. | Packed Node terminal-decline fixture performs fresh SDK write/read and graceful interruption. |
| Server exits while host prompt is unanswered | Server disappears, but CLI does not observe its exit until terminal input arrives; see SYS-001. | Source-reachable negative path is absent from recorded tests. |
| Interrupt after announced readiness | CLI forwards SIGTERM, permits 45 seconds (35-second normal server drain plus grace), and otherwise kills/reaps with failure guidance. Server's own signal watcher and supervisor own subsystem draining. | Native SIGINT/SIGTERM, installed Python, packed Node, and PTY scenarios assert process settlement; forced drain expiry is not exercised. |
| Normal serving child exits after startup | `wait` reports every exit as failure; stop observes/reaps the owned child. Saved credentials/config and committed state permit rerun. | Source evidence. An unsolicited exit during the optional setup prompt is the exception below. |
| CPython pending SIGINT after Rust cleanup | Rust dispatcher settles server and returns 130; wrapper consumes only pending KeyboardInterrupt, propagating other exceptions. | Recorded rebuilt production-wheel command outside checkout plus focused Python fixture asserts exact exit 130. No Python lifetime is emulated in Rust tests. |
| Node SIGINT/SIGTERM | Listeners are installed only for literal `server dev`, preventing Node's default re-raise from conflicting with Tokio. Other commands retain default signal behavior. | Packed executable is rebuilt through emitting build config; fixture selects actual packaged CLI, then checks fresh SDK work and exit 130. Source verifies listeners do not affect MCP proxy/install or ordinary commands. |
| Postgres unavailable after serving | Ordinary server capability/error boundaries remain in place; this CLI adds neither unbounded restart nor retry traffic. A process exit is reported by foreground wait and operator reruns the same command after restoring the dependency. | Runtime server behavior is reused. No production dependency-failover/RPO claim is added or required by this local bootstrap task. |

Existing download, migration, provisioning, file-lock/persistence, MCP editing,
token exchange and server drain owners are reused; the new DevServer state is
earned by owning its installed binary and child lifetime. No new durable retry
queue, service restart policy, database management or storage lifecycle is
required or proposed.

## Proposed finding

### SYS-001 — serving child exit is not supervised during the interactive host offer

- Classification: **INCORRECT**.
- Obligation: spec REQ-005 requires the attached command to report server exit;
  REQ-010 requires reporting success only with a ready usable server. TASK-004
  requires a foreground usable local deployment and explicit failure recovery.
- Exact changed location: `crates/wyrd/wyrd-cli/src/server/dev.rs:180`
  (`mcp::offer_local(...).await`), with child observation deferred to line 185.
  The outer select at lines 85–92 observes only CLI interruption and the
  overall run result. The reachable terminal wait is
  `mcp/hosts.rs:706`; `mcp/mod.rs:236` invokes it for detected hosts.
- Producer-to-consumer evidence: after readiness/provisioning/saved-client
  verification, DevServer retains its live child and awaits host selection.
  A normal server crash, transport-failure shutdown, or termination then makes
  the serving child exit. No child wait or try_wait is polled in this phase.
  The terminal wait has no timer or child dependency and continues indefinitely
  if the user does not answer. After a later answer, `run` prints "Local Wyrd
  ready" before calling `wait`, even though that child has already exited.
- Observable consequence: the foreground CLI remains at the host prompt while
  the HTTP/MCP/SDK deployment is down, fails to report that exit, and can briefly
  announce readiness for the dead deployment. Saved state remains intact, but
  the required failure-and-rerun guidance is withheld until user input.
- Reachability: interactive installed CLI with at least one detected host and
  no explicit `--host`, exactly the approved first-use journey. The existing
  PTY fixture already reaches this phase; independent server termination while
  waiting is a concrete process-loss event, not optional availability work.
- Existing owners and siblings: DevServer already owns `child`, `ready`,
  `wait`, `stop`, and recovery errors. MCP's standalone synchronous selection
  is also a caller of the shared menu/parser and has no server child to watch.
  Python and Node both call the same Rust `ServerCommand::Dev` dispatcher.
  Server `app::supervise` supervises internal server tasks, not its parent CLI.
- Smallest testable correction: keep child-exit observation active through the
  optional interactive host offer inside the existing DevServer lifecycle,
  cancel pending selection on observed exit, and return its existing redacted
  server-exit failure through the existing cleanup path. Do not add an MCP
  supervisor, restart the server, change key issuance, change standalone host
  selection, or defer the fix to SDK projections.
- Focused closure proof: extend the existing terminal/process fixture to leave
  the host offer unanswered and terminate only the owned serving child. Assert
  the CLI settles without terminal input, reports server-exit failure, emits no
  final readiness announcement or secrets, preserves saved credentials/config,
  and allows the ordinary safe retry. Existing CLI-interruption and decline
  cases protect adjacent behavior. A new harness/dependency is unnecessary.

## Verification limits and conclusion

Recorded final corrected lane includes three native CLI scenarios and four
direct pytest outer scenarios, with nested fresh Rust/Python/TypeScript callers.
Fixtures copy/select an installed compatible server but never migrate or issue
credentials on the CLI's behalf. The rebuilt production Python wheel and packed
Node executable are separately credible installed-runtime evidence. The
orchestrator's fresh focused journey completed PASS (exit 0, 140.92 seconds),
including all three exact CLI and four direct pytest cases; its independent
execution evidence is `verification.md` in this review directory. This
reviewer deliberately did not start a duplicate Cargo/Postgres lane. That
green lane does not exercise server exit during the unanswered prompt.

Skipped live release downloads, forced terminal-write failure and forced drain
expiry; source inspection establishes their bounded handling but not runtime
qualification. SYS-001 remains a required local lifecycle correction, subject
to independent finding validation.
