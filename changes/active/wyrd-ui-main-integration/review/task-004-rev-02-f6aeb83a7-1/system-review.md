# System resilience review — TASK-004-REV-02

Result: **FAIL**.

Subject: base `4a76331113864f9bddc43d3cc8df01080a696937`, candidate `f6aeb83a77179ac4450110671df6f564adc1914a`, first review of this task. Source remained unchanged. This report covers lifecycle, failure propagation and telemetry completion, not the visual acceptance audit.

## Authority and deployed paths

Read the original task (including its full behavior/proof sections), approved spec revision 18 REQ-038/039, root AGENTS.md, task-review skill, maintainer guide, applicable testing rules, operations authority map/readiness contract, and Bifrost lifecycle/public query requirements. No nested AGENTS govern the scoped shell/UI paths. CodeGraph was consulted before source navigation.

| Path | Owner and dependency/effect | Evidence assessed |
|---|---|---|
| `dev:full` → canonical prepare | Existing `scripts/dev/example-app.sh`: pinned base, compare actual Git tree to base+patch, refuse dirty checkout, current production wheel and installed native-module digest, refuse testing import | Full existing launcher and task-range diff; no duplicate environment service introduced |
| `dev:full` → MockServer + server/Postgres + Vite | Existing mise/process group ownership and ephemeral `scripts/postgres/dev-server.sh`; server setup replaces tenant credential; startup reports reset/credential replacement | `mise.toml:1288–1340`, dev-server source |
| setup → app → batch → inspect | Setup must deploy tenant gateway/Cards, then ordinary uvicorn app serves; driver uses HTTP and public execution/Bifrost APIs | Canonical patch and existing launcher; failure case SYS-01 |
| interruption → app drain → server cleanup | New `stop` sends TERM to app group and waits before stopping server/mock/UI groups. Existing lifespan logs then invokes public state shutdown; SDK-installed OTEL providers remain the only exporter path | `mise.toml:1296–1303`, patch `src/app.py` and `src/desk.py`; canonical test sends SIGTERM and observes post-stop log readback |
| appended batch → retained telemetry | Batch has actual bounded HTTP calls and execution waits, but changed readback no longer knows each call's trace | Patch `Call/send/readback/missing`; SYS-02 |
| Observe → BFF → shared executor | Existing remote loader passes AbortSignal; `/ui/load` authenticates and runs shared loader with request.signal; executor abort returns query stream and rejects post-abort results; WyrdView generation prevents stale settling | `view.ts::remoteLoader`, `/ui/load/+server.ts`, `node.ts::nodeExecutor`, `WyrdView.svelte:248–303`, Observe page/server load |
| Query rerun/cancel | Server action uses browser session, bounded max_rows/deadline and request.signal; client enhanced form aborts request | Query `+page.server.ts`, `+page.svelte`; no separate runtime owner introduced |

Initial WyrdView load resolves one range for its panels. Existing refresh implementation independently resolves relative ranges per panel; this is within the behavioral review's shared-state scope rather than a separate resilience finding. Browser-only initial panel loading/SSR suitability is likewise left to the frontend/behavior reviewers; credential and cancellation propagation itself uses existing BFF owners.

## Findings

### SYS-01 — Setup failure destroys the healthy environment needed for diagnosis

Classification: **MISSING**. Obligation: approved spec REQ-038 explicitly says “keep healthy services available after setup/batch failure”; task Scenario 1 requires setup failures to remain visible with healthy services inspectable.

Exact location: `mise.toml:1323`, with `set -Eeuo pipefail` at 1288 and EXIT cleanup at 1296–1303.

The launcher has already started MockServer, the real server/Postgres and Vite when it invokes `scripts/dev/example-app.sh setup` as an unguarded foreground command. That script runs the gateway deployment command and canonical `scripts.deploy` under `set -e`. A refused deployment, failed Card declaration or transient setup read fails the command; the enclosing shell immediately runs the EXIT trap. `stop` terminates the healthy server/mock/UI, and the ephemeral Postgres owner tears down its database. The diagnostic environment and any partial setup state disappear. By contrast batch failure is explicitly caught at 1330–1334 and leaves services running. The failure boundary is therefore wrong for setup only, not a reason to redesign process supervision.

Materiality: this is an explicit supported recovery requirement, with a normal reachable upstream setup failure. It prevents examining the failed provisioning against the same server and ephemeral database. The happy-path lifecycle test never induces setup failure and cannot prove this branch.

Smallest correction: extend the existing `dev:full` setup/batch status handling so a failed canonical setup is reported and leaves the already healthy server/UI/MockServer alive. Do not start the dependent app/batch after setup failed, do not print app/batch readiness as success, and retain the same interruption cleanup owner. Startup failures before useful dependencies exist may retain their ordinary fail-fast behavior. No controller, retry policy or persistence change is needed; reusing the existing batch failure/status pattern is sufficient.

Focused closure proof: induce a canonical setup command failure after server readiness; assert truthful setup failure, no app/batch launch, healthy server/UI/MockServer still answer, and Ctrl-C then removes only the launcher's owned children. This is one targeted lifecycle negative case, not another full default batch replay.

### SYS-02 — The final batch repair replaced request correlation with ambient service counts

Classification: **INCORRECT**. Obligation: original task “Finite batch, complete evidence” requires recording actual request/standard trace context and correlating evidence by existing trace/Run IDs; missing telemetry must fail. REQ-038 requires retained readback to determine batch completion rather than an ambient observation count.

Exact locations in the reviewed patch: `scripts/dev/wyrd-example-py.patch:88–98` (`Call`), 112–133 (`send`), 155–167 (SPANS/LOGS/JUDGE) and 172–198 (`readback`/`missing`). These line numbers refer to the committed reviewable patch, whose effective source is canonical `scripts/batch.py`.

In candidate commit f6aeb83a7 the call's trace ID and request propagation were removed. Sending plain requests correctly permits real application HTTP roots, but neither `Call` nor `send` now captures the actual application trace context. `readback` counts **every** support-desk trace holding any HTTP server span and every support-desk log trace since batch start; it uses `len(spans)` and `len(logs)` rather than the calls' identities. `missing` accepts counts greater than or equal to workload denominators. The judge count is also ambient since-start/model-name classification, while app calls/verdicts still use returned Run IDs. Successful Run-correlated evaluation rows do not establish that the requested HTTP span/log records were retained.

Reachable failure: while a batch runs in the deliberately live app, another ask or HTTP request (including another `dev:batch`) produces evidence inside that window. If a workload request's span or log is absent, unrelated evidence replaces its count. The report can state success although the requested telemetry is missing, and the report cannot identify the missing HTTP-only/error request because it no longer records its trace. This is not a demand for adversarial isolation: live investigation and appended batches are supported, and exact correlation is an explicit task requirement.

Verification: an in-memory assert-based probe extracted candidate `missing` through Python AST without importing SDKs or editing source. One workload call with no retained HTTP trace/log identity, aggregate counts of one from unrelated requests, and the expected Run/model/metric totals produces `[]` gaps. The producer SQL and `readback` body establish that unrelated records actually reach those aggregate counters. Existing canonical test exercises a quiet 20-request run and equality of totals; it does not exercise substitution by unrelated traffic.

Smallest correction boundary: keep plain app-owned HTTP roots and restore actual per-call trace identity at the canonical application/driver seam, then reuse the current public Bifrost readback to require each workload trace's retained evidence. For the HTTP-only/error calls with no Run ID, capture the real active OTEL trace context from the existing ASGI response hook/standard context propagation and retain it in the driver; do not restore fabricated parent spans, introduce a second Run/exporter, add grouping metadata, or treat service/time counts as exact membership. Existing returned Run/execution IDs remain the authority for app/judge/verdict membership; constrain judge attempts to those Runs as well. Keep metric observations as separately reported time-window measurements, since points are not one-per-request evidence. This is bounded counterpart app/driver work, not a new Wyrd query/schema/SDK contract.

The corrective cost is a small context bridge and scoped membership checks in existing owners. A threshold/count adjustment alone cannot recover identities already discarded by `send`; adding consumer guards to `missing` cannot infer membership. Scope the closure to the required evidence, retaining existing report/accounting behavior and no new lifecycle owner.

Focused closure proof: run a small real canonical batch with unrelated app traffic in the same window; deliberately omit one workload HTTP span/log from the readback being checked and verify the driver reports that specific workload gap instead of accepting the unrelated row. Verify app-owned HTTP roots remain roots and all four workload cases preserve actual trace correlation. Reuse the existing canonical test/patch and Bifrost mechanism; no new runner or broad suite.

## Verification limits and scoped result

I inspected source and exact diffs; I did not restart Postgres/Docker or rerun the expensive live lifecycle. Existing V1 evidence supports successful startup, appended data and cleanup; V2 asserts ordinary export while serving and a final shutdown log; V9 is recorded human/implementation evidence, not independently rerun here. Neither includes setup-failure recovery or unrelated-traffic substitution. The in-memory candidate `missing` probe passed its assertion demonstrating the false-success path.

No separate source-proven defect was found in the changed shutdown ordering or existing query cancellation propagation within this scope. These accepted paths retain their recorded evidence and do not require replacement machinery. The two findings above are bounded original obligations; they do not add persistence, automatic recovery, a new provider, or a general supervisor.
