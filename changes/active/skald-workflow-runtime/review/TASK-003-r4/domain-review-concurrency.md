# Concurrency, cancellation, timeout, and auth-renewal domain review

**Immutable subject:** base `58d07d7260df1f022a721e720a28ea48e5096e35`, candidate `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`.

**Overall result: FAIL.** The candidate closes the pending-native-`401` renewal ordering defect and keeps model submission send-once, cancellation local to the caller IO, request context immutable per call, and blocking filesystem work off Tokio's polling threads. One bounded R3 defect remains: a client-less mixed `ext_gateway` plus `wyrd_gateway` run reads `config.toml` twice instead of deriving both local bindings and the gateway client from the one approved run-start snapshot.

## Reviewed boundary and authority

I reviewed the complete base-to-candidate source and diff against `AGENTS.md` sections 5, 6, 7, 11, and 16; `architecture/agent-rules.md`; the applicable client and Workflow sections of `architecture/wyrd-design.md` and `architecture/wyrd-doctrine.mdx`; approved Revision 12; original TASK-003; all three prior verdicts, validated ledgers, and remediation tasks; and the implementer's recorded evidence. The human-approved `WyrdGatewayCall.model` amendment, the R1 corrected native-`401` wording, and the human approval of `spawn_blocking` for run-start config/client/secret reads were treated as authority. Run-start preparation was not treated as drift: loading must continue to read no execution secrets.

The trace covered:

- `WyrdGatewayProvider::send` through `PublicWyrdGatewayCaller::call`, `HttpTransport::post_native`, and `AuthMiddleware::{bearer,force_refresh}`;
- immutable model, fallback, remaining timeout, cancellation token, and correlation propagation through Skald route planning and the public client adapter;
- pre-dispatch and post-dispatch cancellation, caller deadline/drop behavior, complete/truncated/pending native `401` bodies, renewal failure, and the no-replay boundary;
- `Workflows::{create,get,cancel,wait}` idempotency, polling, terminal values, and dropped-future ownership;
- `Workflow::run_with`, `load_local_setup`, `SelectedRoutes`, selected secret resolution, retained Cards client context, ambient client construction, and Python's shared-runtime bridge;
- the R3 focused raw-HTTP fixture and recorded shared-client/Python evidence.

No build, compilation, test, lane, Cargo, mise, pnpm, pytest, formatter, linter, or other verification command was run. Conclusions use source, the cumulative diff, and recorded evidence only.

## Boundary coverage

| Boundary | Source and recorded evidence | Result |
|---|---|---|
| Immutable concurrent call context | `crates/skald/skald-workflow/src/route.rs:543-590` stores model, fallback, absolute deadline, cancellation, and correlation on an attempt-local adapter and constructs a fresh `WyrdGatewayCall`; `crates/shared/wyrd-client/src/workflow/gateway.rs:77-123` keeps its projected body and header vector local to that call. The recorded concurrent case distinguishes three models and fallback headers on one shared caller. | PASS |
| Caller cancellation and timeout | `workflow/gateway.rs:96-116` races the entire authenticated send/renew/body future against the call's remaining timeout and cancellation token with cancellation precedence. The recorded post-dispatch case observes the POST before cancellation and no resend. Dropping the client future does not claim to roll back gateway-owned work. | PASS |
| Native `401` send-once and pending-body renewal | `transport/http.rs:376-410` sends exactly one POST, branches on the known status, awaits `force_refresh` before body collection, gives renewal failure precedence, and never rebuilds or resends the request. The R3 `Reply::Hold` case records initial exchange, one model POST, and renewal while the response body remains pending. This closes prior `FIND-TASK-003-2`. | PASS |
| Auth cache ownership | `auth.rs:451-543` retains the existing shared cache/gate and refresh owner. TASK-003 adds no second cache, detached renewal worker, or credential mutation surface. The approved correction intentionally leaves cancellation of an in-progress refresh governed by the existing caller future; the prior proposal for cancellation-safe retirement was rejected as drift. | PASS |
| Remote lifecycle/drop semantics | `workflow/remote.rs:41-116` uses the shared idempotent submission owner, direct GET/cancel snapshots, and foreground one-second polling. Dropping `wait` owns no background task and sends no cancellation; create/cancel document possible server-side progress after caller drop. | PASS |
| Blocking-pool boundary | `workflow/mod.rs:77-95,130-149` puts authored bundle/client creation and run-start ambient setup on `spawn_blocking`; `workflow/local.rs:115-138` puts selected secret files on the blocking pool. Python releases the GIL and uses the shared runtime at `sdks/wyrd-sdk-python/src/workflow.rs:513-516,559-562`. The approved run-start timing is preserved. | PASS |
| Retained client context | `workflow/mod.rs:35-42,137-145,274-320` retains and reuses the Cards client; Python stores the complete `ClientWorkflow` and mutates only its Skald value at `sdks/wyrd-sdk-python/src/workflow.rs:204-215,305-435,559-579`. | PASS |
| One coherent run-start ambient snapshot | `workflow/mod.rs:195-208` reads `GlobalConfig` once for selected external bindings and then calls `WyrdClient::from_global`, which reads it again through `client.rs:48-54` and `config.rs:91-98`, whenever a client-less Workflow selects both route families. | **FAIL — CONC-R4-001** |

## Proposed finding

### CONC-R4-001 — mixed ambient routes use two configuration snapshots

- **Classification:** INCORRECT
- **Violated obligation:** TASK-003-R3's selected correction for `FIND-TASK-003-10` requires the shared run-start owner to load one ambient `GlobalConfig` snapshot when selected routes require it and reuse the existing `GlobalConfig`, `ClientConfig`, and `WyrdClient` construction behavior for both Workflow bindings and a client-less public-gateway client. This is the approved consistency boundary that accompanies moving the complete filesystem-bearing setup to `spawn_blocking`.
- **Exact locations:** `crates/shared/wyrd-client/src/workflow/mod.rs:190-210`, especially `GlobalConfig::load` at line 196 and `WyrdClient::from_global` at line 203; the second read is at `crates/shared/wyrd-client/src/client.rs:48-54` through `crates/shared/wyrd-client/src/config.rs:91-98`.
- **Evidence and reachability:** `SelectedRoutes` independently sets `needs_config` for any `ext_gateway` binding and `needs_gateway` for any `wyrd_gateway` step (`workflow/local.rs:39-70`). A wholly local/authored Workflow can contain both route families and has no retained Cards client. For that ordinary path, `load_local_setup(true, true, None)` first loads `config.toml` and extracts its `workflow` section, then calls `WyrdClient::from_global`, which loads the same file again before constructing the gateway client. A file replacement or edit between reads can therefore combine external bindings from snapshot A with endpoint, tenant, and token-cache settings from snapshot B in one run. Even without a concurrent edit, it performs the duplicate filesystem read that R3 explicitly removed from the accepted design.
- **Recorded-proof contradiction:** The R3 evidence says `load_local_setup` covers config load plus gateway client build, but does not establish one snapshot. The rerun `selected_local_dependencies_use_shared_config` does not exercise this path: its external-route cases call `SelectedRoutes::dependencies` with a pre-parsed `LocalWorkflowConfig` (`workflow/mod.rs:549-565`), while its gateway case supplies a retained explicit client (`:683-724`). Thus the recorded green selectors cannot close the source-visible mixed-route gap.
- **Observable consequence:** One Workflow run can prepare its two selected route families from mutually inconsistent ambient configuration, producing nondeterministic endpoint/tenant/binding selection during a concurrent config update. This also defeats the remediation's single run-start snapshot contract.
- **Required testable correction:** In the existing `load_local_setup` owner, load at most one `GlobalConfig` whenever external bindings or a client-less public gateway require ambient configuration. Derive the selected `LocalWorkflowConfig` and, when necessary, the gateway `ClientConfig` from that same value using the existing `ClientConfig::from_global_with_env` and `WyrdClient::with_config` owners. Preserve retained-client precedence, selected-route laziness, run-start `spawn_blocking`, credential-file resolution, current public error mapping, and purely Native/no-ambient behavior. Do not add a cache, watcher, async config API, setting, option, dependency, new harness, or permanent check. Closure needs static source proof of the single snapshot plus rerun/recording of the existing selected-local-dependencies and retained-client/language evidence; the R3 authority expressly does not require a timing or synthetic slow-filesystem test.

## Cancellation, timeout, and recovery assessment

The R3 native renewal ordering now matches the approved contract: after response headers establish `401`, the existing auth owner is awaited before a pending body can block it. A successful refresh prepares only later calls; the current model POST is never replayed. Cancellation or timeout drops the local future, while Skald owns abort/drain of its step task and the gateway owns any already accepted settlement. No shared mutable fallback, model, deadline, correlation, header map, or Python-side client slot is introduced.

`spawn_blocking` work may finish after its awaiting run future is dropped, but it has only run-start reads/client construction and no dispatch or durable mutation. That is consistent with the human-approved boundary and does not justify detached cancellation machinery. Selected secret reads likewise remain run-start preparation so Workflow loading continues to resolve no execution secrets.

## Verification evidence and limits

The implementer records a six-test focused shared-client selector, Clippy, formatting, and diff checks for R3. Source supports the pending-body renewal, send-once, auth-doc, `into_skald` ownership-doc, blocking-pool, retained-client, and post-dispatch cancellation claims. Earlier records cover the Python retained-context journey and broader shared/client boundary lanes.

The evidence does not exercise client-less mixed `ext_gateway` plus `wyrd_gateway` ambient setup, and source contradicts R3's one-snapshot requirement, so that omission is part of `CONC-R4-001`, not a residual verification limit. I found no other reachable defect in this domain and no unestablished mechanism, check, file, setting, or option that qualifies as DRIFT. Candidate identity remained `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89` when this report was written.
