# Process and terminal domain review

Result: **PASS**. No material proposed findings in the approved one-child boundary.

## Subject, authority and coverage

Reviewed cumulative `ebd75dc224854413d17cb06b70da8181b78cde6b...796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`, including rename-aware comparison of `server/dev.rs` to `init.rs`, and remediation `00b6148919b9ccd5f958f3b621ab6a89c6372001...796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`. HEAD remained the candidate. Source was not changed.

Boundary: explicit guided selection, executable/config distinction, invocation arguments, terminal ownership, server/UI supervision, child exit/spawn failure, interruption/cleanup, ordinary init and saved bootstrap state. ASGI cancellation and deployment identity have separate domain owners.

Authority read: supplied root AGENTS; task-review skill; revision-6 spec REQ-001–004/037–038 and non-goals; original TASK-001; prior verdict and FIND-TASK-001-4 validation; R1 remediation scenarios 4/6/7 and implementation diagnosis; spec-driven development; agent-surface and maintainer guidance; operations deployment-and-release Local server bundles; TESTING/mise local-development journey ownership. No nested AGENTS owns the CLI directory. CodeGraph preceded code source/location inspection.

| Obligation / path | Source evidence | Verification assessment | Result |
|---|---|---|---|
| Guided selection before bootstrap, terminal required, no silent choice | `init.rs:368–490`: offline print returns first; interruption races agent/input resolution; agent checks stdin/stdout terminal, refuses multiple hosts, menus installed candidates and validates explicit choice. Clap conflicts/exclusive print at `45–90`. | C1 checks offline/no-terminal and conflicts against invalid saved config and a listening socket; C2 checks missing/multiple hosts and explicit interactive choice. | PASS |
| Configuration alone does not establish executable presence | `hosts.rs:114–119` checks PATH files independently from `HostInstaller::detected/config_path` at `325–370`; `agent()` does not use config detection as executable detection. | C2 has a configured missing Hermes and installed fake agents. | PASS |
| Selected connection/readiness precede launch | `init.rs:918–1004`: server/UI readiness, credential proof/save, gateway, selected one-element host slice, offer, report `into_result`, then guide. `mcp/mod.rs:258–317` refuses failed configuration. | C2 checks real readiness before child announcement, connected configuration visible at child start, and failed Pi connection launching nothing. Existing connection is the established configuration workflow, not a new live paid-host handshake. | PASS |
| Shared literal prompt, host's ordinary safeguards | `hosts.rs:90–130` owns all seven program/argument branches; display quotes apostrophes; actual command uses direct argv. `guide()` adds only kill-on-drop; no shell evaluation, cwd override, credential argument, approval or bypass flag. | Literal display test covers all seven prefixes and quote/newline/dollar/backtick payloads. C2 observes exact Codex argv, cwd and terminal IO. | PASS |
| Normal/nonzero exit and failed spawn leave ready Wyrd serving, without claiming verification | `init.rs:1028–1056`: failed spawn returns to supervision; child wait restores terminal and reports status; `run` then waits on server/UI. | C2 observes nonzero exit and failed interpreter spawn, with readiness after each. Success exit shares the wait/restore branch. | PASS |
| Interruption and server/UI exit stop/reap agent before existing settlement | Outer select at `410–427` cancels run then calls stop; agent retained before wait at `1038–1045`; server/UI concurrently observed there. `stop:1297–1309` awaits agent settle, restores termios, then joins existing UI/server settlement. `settle:1319–1367` signals recorded PID, waits within existing 45-second budget, kills/reaps on deadline. `interruption:1824` observes SIGINT/SIGTERM. | C2 checks SIGINT, unexpected server kill, agent PID absence, reusable listeners and usable terminal. UI exit uses the same select and cleanup owner. | PASS |
| Terminal IO and restoration | Child inherits terminal IO. `terminal.rs:104–123` snapshots/restores stdin termios; completion restores at `1047`; interrupt/server failure restores after settlement at `1303`. | Fake agent enters raw/no-echo; C2 verifies ECHO and ICANON after cleanup. | PASS |
| Ordinary init, saved state, existing supervisor retained | Rename-aware full diff preserves resolve, credential proof/save, migration, provisioning, UI command, readiness and settlement. Agent returns None outside Guided; log mode adds guided only. `run:967` prints ordinary handoff. TS wrapper selector renames its foreground signal exception to init. | C2 ordinary run checks no agent PID, exact handoff, JSON prompt parity and serving lifetime. Saved-state regressions remain in local-development lane; no new persistence/key reissue owner. | PASS |

## Existing alternatives and sibling consumers

The complete existing bootstrap owner is DevServer, moved from server/dev.rs; this extends retained children/final settlement rather than introducing another supervisor. Existing settle owns signal/wait/deadline behavior and now settles agent first. Terminal/EchoGuard own cancellable question IO and concealment restoration; SavedTerminal retains pre-agent attributes across a child lifetime, a different retained resource.

All launch consumers were enumerated: printed handoff calls McpHost::launch; actual guide calls McpHost::command; both project launcher. Executable detection has two agent consumers (menu and explicit/revalidation). HostInstaller::from_process separately locates the Wyrd proxy, not an alternate agent launch owner. Server/UI keep their commands and null stdin. Python/TS installed CLI projections keep the CLI owner; changed TS signal matching preserves existing foreground delegation for both init modes.

## C2 terminal diagnosis

R1 diagnosis is source-consistent. FAKE_AGENT now executes stty raw/no-echo before its announcement, then execs sleep (`pg_tests.rs:1381–1393`). The announcement is a synchronization boundary: the termios change completed, and recorded shell PID becomes the waiting executable. Announcing before stty allowed the test to signal/reap the shell while its stty child could still mutate the shared terminal after restoration. That is a fixture race under the approved direct-child boundary; it does not justify process-tree or group signalling. Updated fixture still exercises direct child stop/reap and raw-terminal restoration, without relaxing assertions, timeouts or selection.

The remediation evidence describes a separate diagnostician; its original transcript/report was not supplied. This audit independently checked source mechanism, without claiming reproduction of the historical failure.

## Considered counterexamples and verification limits

- PATH file presence cannot guarantee successful spawn (permissions, missing interpreter or replacement). REQ-038 explicitly defines failed-start behavior; C2 intentionally exercises a missing interpreter. I do not strengthen it into a new version/executable-probe contract. Configuration alone remains insufficient and correctly rejected.
- Only Wyrd's retained child PID is signalled. Detached sessions/descendants and their later terminal mutations are not claimed owned; requiring process-tree termination would exceed approved scope and risk pre-existing editor sessions.
- Restoration is best-effort tcsetattr, consistent with existing concealment cleanup; terminal disappearance is not solved by another restoration abstraction.
- C2 uses explicit SIGINT to CLI PID and a PTY, not a human typing Ctrl-C into each real host's raw-mode handler. No paid host or detached editor was run.
- No Cargo/build/test command rerun by this reviewer, avoiding shared concurrent builds. Green runtime results are inspected submitted evidence, not independent reruns. C1/C2 source assertions provide credible bounded proof; combined journeys remain change-review work.
- Submitted runtime evidence is Linux x86_64; macOS arm64/Linux arm64 and paid hosts remain unexercised. Parent hard kill is outside graceful interruption guarantees.

## Proposed findings

None. FIND-TASK-001-4 closes for this domain: every supported host shares literal display/direct invocation syntax. No material process/terminal defect was established. Prior ASGI/manual/rustdoc findings are outside this report's closure scope.
