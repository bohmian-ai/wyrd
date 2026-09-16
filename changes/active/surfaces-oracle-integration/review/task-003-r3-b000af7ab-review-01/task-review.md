# TASK-003-R3 task implementation review

## Immutable subject and scope

- Base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; candidate `b000af7ab704f077a8a4ba3e29c2b968d47d344d` (tree `f087f7d395cd53386e4e2c6f011b31d8604fd791`).
- Source was last changed at `f8e887b3038651d2ba82091d642915d6b856d4d6` (tree `eb98062dc854277a248f37d86afe55beb9c3db26`); the candidate commit changes only the R2 review and R3 evidence packet.
- Authorities: approved `spec.md` revision 9, original TASK-002/TASK-004/TASK-003, prior R1/R2 reviews and remediation packets, TASK-003-R3, `AGENTS.md`, agent rules, and Bifrost query/test authorities. `.codegraph/` is absent. Unrelated dirty `verified-change-contract` files were excluded. The cumulative base-to-candidate diff, not only the latest patch, is the reviewed subject.
- Read-only review; no broad test rerun. `git diff --check 861f8d86..b000af7ab` exits 0.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Preserve cumulative TASK-002/TASK-004/TASK-003 and R1/R2 corrections, with one shared client, single data root, retained Forge recovery assertion, approved storage URL, three SDKs, and no legacy owner | Cumulative diff and prior accepted closure; R3 source diff touches only Oracle forwarding, edge rustdoc, and test-only journey hooks | Prior R2 verdict; R3 same-tree `test:shared`, storage matrix, cards/CLI/WyrdState/Python/TypeScript journeys and codegen checks recorded | PASS on available evidence |
| A connected selected remote peer cannot hold pre-header delivery past the captured Oracle deadline; typed timeout, cancellation, one delivery, no successor | `forward()` captures one `Instant`; `route_remote_once()` reuses it for connection and `tokio::time::timeout` around the whole `forward_remote()` future, including credentials and initial gRPC response. Timeout drops that future and maps to `BifrostError::QueryTimeout` | Exact paused-time unit test (2 selected/passed) and role-separated HTTP journey (2 selected/passed); negative control reportedly fails without the timeout; server journey lane 12 passed | PASS |
| Retain protected edge, auth, tenant, request-ID, local Oracle, peer-error, and terminal-stream behavior | R3 production diff changes only selected remote pre-header wait; `EdgeTimeout` and query handoff unchanged, `forward_remote()` still returns the forwarded terminal stream and maps peer errors | Local edge journey and remote journey recorded; Bifrost server journey 12 passed | PASS |
| The silent-peer test hook is justified and absent from ordinary production builds | `SilentForwardPeer`, state accessor, and peer-service call are all `#[cfg(feature = "test-support")]`; it parks before `accept` and counts arrival/abandonment. No new feature/dependency/production branch | Role-separated HTTP journey observes one parked handler abandoned, then successful follow-up | PASS |
| New edge-layer associated types and fallible methods have required rustdoc, with no behavioral modification | `edge_timeout.rs` adds docs on `Layer::Service`, `Service::{Response,Error,Future}` and `# Errors` on `poll_ready`/`call`; diff is documentation only | Source audit; recorded `fmt:check` and `lints` pass | PASS |
| Six previously omitted owner lanes run on the final source tree | R3 packet records identity 18; Postgres contract, concurrency, roles, inventory; Forge production geometry 1 (469 s) | All six recorded at `f8e887b30` / `eb98062d`; no zero-test result reported | PASS on recorded evidence |
| Complete Bifrost capability gate on the final source tree | `mise.toml`: `verify:bifrost` is only `check:bifrost` + `test:bifrost`; the latter runs nine lanes from `run-bifrost-tests.sh` | R3 packet maps `check:bifrost` checks and all nine Bifrost lanes with nonzero passing counts; R3 explicitly permits this exact-child alternative | PASS on recorded evidence |
| Broad final repository gate and every focused lane pass on the corrected tree, without baseline waiver or empty selection (REQ-064, INV-025, AC-022; TASK-003 and R3) | `mise.toml` defines `gate` as a pure dependency aggregate of 49 tasks; the packet claims `test:bifrost` plus all other 48 dependencies passed separately. This is substantial test coverage, not a lower-tier substitution | No passing `mise run gate` on `f8e887b30`: attempts were SIGTERM-killed. Unlike `verify:bifrost`, neither the approved spec nor R3 explicitly accepts child equivalence for `gate`. The packet also summarizes the 48 children rather than recording every command/count individually as R3 directs | FAIL — TASK-R3-1 |
| Three local real-cloud owners pass on the final source tree; merge-to-main cloud workflow retained | Approved revision-9 contract and unchanged workflow; ignored `mise.local.toml` invokes S3/GCS/Azure cloud lanes | Packet records three local tasks, each with CRUD and multipart e2e 1/1 passing at `f8e887b30` | PASS on recorded evidence |
| Non-goals: no public query deadline/schema change, second configured timer, global HTTP timeout increase, terminal-frame change, Azure/LocalFS duration claim, skipped tests, push/merge/deploy | R3 production diff keeps one captured Oracle deadline and existing public wire; evidence notes storage ceilings; candidate remains local | Source diff and `git show` of evidence-only final commit | PASS |

## Proposed finding

### TASK-R3-1 — final-tree aggregate proof is incomplete

- **Classification:** MISSING.
- **Violated obligation:** Approved REQ-064/INV-025/AC-022 and TASK-003 require a passing broad local `gate` on the final candidate; R3 explicitly repeats `mise run gate` and requires each command/result to be recorded. It allows exact-child substitution for `verify:bifrost` only.
- **Location and evidence:** R3 evidence packet, lines 163–194, reports killed `mise run gate` attempts and a grouped claim that its 49 declared dependencies passed separately. `mise.toml` lines 1361–1415 confirms `gate` has dependencies only, so this is strong functional coverage; nevertheless there is no passing aggregate invocation or individually auditable 49-child command/result map. The earlier passing `gate` at `117f668f6` predates R3 source changes.
- **Reachable consequence:** The tested source may be correct, but the immutable candidate does not yet satisfy the approved *evidence* condition as written. This is not a new product bug or evidence that a test assertion fails: the apparent aggregate failure was host memory pressure, and the direct children reportedly all passed.
- **Required testable correction:** Run `mise run gate` to successful completion at the final source tree with resource contention removed, preserving all selections and checks. If that remains impossible, obtain an explicit approval that a complete, individually recorded same-tree execution of every declared child constitutes the broad-gate acceptance result; do not silently infer that exception from R3's separate `verify:bifrost` allowance. Record exact child commands, exit statuses, selected test counts where applicable, and source commit/tree.

The documented child runs cover the gate's substantive tests; no additional test harness, code path, checker, or refactor is proposed by this finding. The remote timeout fix and edge rustdoc close R2-3 and R2-2 respectively. R2-1's six specific omitted lanes are closed, but its broad final-tree gate proof remains formally open.

## Overall result

**FAIL** — one evidence/authority gap, `TASK-R3-1`. No source-correction finding is proposed.
