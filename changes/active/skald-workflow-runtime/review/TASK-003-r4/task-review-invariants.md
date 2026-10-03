# TASK-003 r4 invariant review

**Overall result: FAIL.** The cumulative candidate closes the prior native
`401`, documentation, retained-client, error-redaction, and source-contract
findings. One bounded defect remains in the R3 correction: a client-less run
that selects both `ext_gateway` and `wyrd_gateway` reads the ambient global
configuration twice instead of deriving both dependency sets from the one
required snapshot. The recorded R3 evidence also omits the required
retained-client/language rerun after changing the shared `run_with` path.

## Immutable subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Remediations: TASK-003 R1, R2, and R3 under the corresponding review directories

The human-approved 2026-10-03 `WyrdGatewayCall.model` amendment, the R1
same-spec native-`401` wording correction, and the approved use of
`spawn_blocking` for run-start configuration/client/secret reads were treated
as authority. This review inspected the complete base-to-candidate source and
diff, applicable repository authority, prior verdicts and ledgers, and the
implementer's recorded evidence. It ran no build, compile, test, lane, Cargo,
mise, pnpm, pytest, formatter, linter, or other verification command.

## Producer-to-sink invariant trace

- `Workflow::run_with` computes the selected routes, clones any retained
  loading client, and moves all ambient setup into one blocking-pool task
  (`crates/shared/wyrd-client/src/workflow/mod.rs:135-142`). This closes the
  async-executor blocking half of prior `FIND-TASK-003-10`.
- `load_local_setup` reads `GlobalConfig` for selected external bindings at
  `workflow/mod.rs:195-196`. If the same Workflow also needs a public gateway
  and has no retained client, it then calls `WyrdClient::from_global` at
  `:200-208`.
- `WyrdClient::from_global` calls `ClientConfig::from_global`
  (`crates/shared/wyrd-client/src/client.rs:48-53`), which calls
  `GlobalConfig::load` again (`crates/shared/wyrd-client/src/config.rs:91-103`).
  The second snapshot supplies client endpoint, tenant, and token-cache
  settings while the first supplied the external bindings. A configuration
  replacement between those ordinary filesystem reads therefore composes one
  run from two file states.
- The existing constructors already provide the required reuse seam:
  `ClientConfig::from_global_with_env(&GlobalConfig)` derives client settings
  from a caller-owned snapshot, and `WyrdClient::with_config` assembles the
  client. No new API or mechanism is needed.
- `SelectedRoutes::dependencies` resolves only selected bindings and installs
  the retained or ambient gateway client (`workflow/local.rs:79-103`); the
  execution plan then validates every selected route before dispatch. The
  snapshot split is produced before these consumers and is not corrected by
  them.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Recorded verification evidence | Result |
|---|---|---|---|
| REQ-024/031/045/046 and AC-019: one shared create/get/cancel/wait handle, stable idempotent submission, direct run snapshots, fixed polling, and drop-without-cancel | `workflow/remote.rs` delegates to the shared `WyrdClient` transport and returns native run DTOs | Original and R1/R2 records include `shared_workflow_client_contract`; R3 reran it | PASS |
| Rust SDK projects shared handles without enabling Python | Shared exports and `sdks/wyrd-sdk-rust/src/lib.rs` re-exports | Original recorded SDK and boundary lanes; later changes do not alter exports/manifests | PASS |
| REQ-036A/038/039/043 and INV-020: model, fallback, remaining deadline, cancellation, and correlation remain immutable per call | Skald attempt adapter constructs a fresh `WyrdGatewayCall`; `PublicWyrdGatewayCaller` keeps all call values local | Recorded focused transport cases cover concurrent isolation, supported dialects, Vertex refusal, timeout, and post-dispatch cancellation | PASS |
| Corrected native-`401` contract: one model POST, renewal starts after status and before body collection, no replay, renewal error precedence | `transport/http.rs:382-412` sends once and calls `force_refresh` before `response.bytes()` | R3 recorded the pending-body, cut-off-body, complete-body, and failed-renewal cases in the existing focused selector | PASS — prior `FIND-TASK-003-2` closed |
| Native refusal normalization retains only approved safe fields and catalog/category text | `workflow/gateway.rs::Ingress::problem`, `wyrd_problem`, and `provider_remediation` | Prior recorded three-dialect/canary/status proof remains source-aligned | PASS — prior `FIND-TASK-003-3` closed |
| AC-011A and INV-020: authenticated ingress consumes, validates, and does not forward the exact fallback header | `gateway/policy.rs::from_header_value`; `gateway/ingress.rs::requested_fallback`; route assembly | Prior recorded policy, PG ingress, and served OpenAPI evidence remains source-aligned | PASS |
| REQ-058/AC-031: one shared local dependency owner, lazy selection, selected-secret-only resolution, retained Cards client, and explicit native injection | `Workflow::run_with`, `SelectedRoutes`, shared secret reader, and Skald dependency owner | Prior focused/shared/Python evidence covers these behaviors before the R3 `run_with` boundary change | PASS for runtime ownership and selection |
| R3 `FIND-TASK-003-10`: load one ambient `GlobalConfig` snapshot for selected route preparation and derive both Workflow bindings and a client-less gateway client from it | `load_local_setup` reads once for bindings, then `WyrdClient::from_global` reads again for a client-less mixed-route run | R3 records only separate external-route and retained-client cases; neither exercises client-less mixed routes or proves one snapshot | **FAIL — INV-R4-001** |
| R3 closure proof: rerun selected-local-dependency proof and retained-client/language coverage after changing shared `Workflow::run_with` | Rust selected-dependency source exists; the Python retained-context integration reaches the shared sync/async bridge and `run_with` | R3 command record names Rust Workflow/transport/auth tests only; it does not record the existing Python integration or another language-boundary run | **FAIL — proof gap included in INV-R4-001** |
| FIND-TASK-003-11: auth owner accurately distinguishes replay-safe retries from native send-once renewal | `auth.rs:503-514` | Static source plus recorded auth selectors | PASS |
| FIND-TASK-003-12: consuming the facade documents discarded client/composition context | `workflow/mod.rs:165-174` | Static source and recorded lint evidence | PASS |
| INV-004/007/011/012: local route semantics, cross-language facade, Prompt ownership, and gateway credential isolation remain intact | Shared Workflow facade, Skald route owner, private gateway caller transport, redacted secret types | Prior focused, integration, language, codegen, and boundary records remain source-aligned except for the expressly missing R3 language rerun above | PASS for source behavior |
| Non-goals: no new ingress, Vertex endpoint, credential mutation surface, provider-body Workflow context, polling knob, arbitrary-header public API, duplicate executor/graph/transport, cache/watcher/check/option, or unrelated refactor | Complete cumulative diff and live callers | Static inspection | PASS |

## Proposed finding

### INV-R4-001 — INCORRECT / MISSING: mixed client-less route preparation does not use the required single configuration snapshot

- **Violated obligation:** TASK-003-R3 lines 63-69 require the shared Workflow
  preparation owner to load one ambient `GlobalConfig` snapshot when selected
  routes require it and reuse the existing `GlobalConfig`, `ClientConfig`, and
  `WyrdClient` construction behavior for both Workflow bindings and a
  client-less gateway client. Its acceptance criterion at lines 120-131 also
  requires the retained-client/language coverage to be rerun and recorded.
  This preserves REQ-058's one shared local configuration path and the retained
  client invariant repaired by `FIND-TASK-003-1`.
- **Exact locations:**
  `crates/shared/wyrd-client/src/workflow/mod.rs:190-210`, with the second read
  through `crates/shared/wyrd-client/src/client.rs:48-53` and
  `crates/shared/wyrd-client/src/config.rs:91-103`; incomplete evidence at
  `changes/active/skald-workflow-runtime/review/TASK-003-r3/TASK-003-R3-close-pending-renewal-and-async-context-contracts.md:148-161`.
- **Evidence and reachability:** A file-loaded Workflow may contain at least one
  `ext_gateway` step and one `wyrd_gateway` step, making both route flags true,
  while owning no retained client. `load_local_setup` first moves
  `GlobalConfig::load()?.workflow` into `config`, then the `(None, true)` arm
  calls `WyrdClient::from_global`, whose existing constructor loads the same
  global file again. Atomic config replacement between reads is ordinary and
  can yield external bindings from snapshot A with gateway endpoint, tenant,
  or token-cache settings from snapshot B. The focused Rust test covers an
  external-only run and a gateway-only run with an injected retained client
  (`workflow/mod.rs:567-725`), so it cannot detect the mixed client-less path.
  The R3 record claims closure but lists no rerun of the existing Python
  retained-client integration after moving its shared execution preparation
  through `spawn_blocking`.
- **Observable consequence:** One run can dispatch its external-gateway steps
  under one coherent config version and its public-gateway steps under another,
  contrary to the selected single-snapshot correction. Separately, the changed
  shared async boundary lacks the task-mandated language-runtime regression
  evidence, so the retained Python client/context projection is not proved for
  this candidate.
- **Required testable correction:** In the existing `load_local_setup` owner,
  load at most one `GlobalConfig` when either selected external bindings need
  it or a client-less selected public gateway needs it. Derive the local
  Workflow config and, when needed, the client configuration from that same
  value using the existing `ClientConfig::from_global_with_env` and
  `WyrdClient::with_config` owners. Preserve no-read behavior for purely native
  runs and gateway runs with a retained loading client, selected-secret-only
  resolution, existing error mapping, and the approved blocking boundary. Add
  no cache, watcher, API, setting, option, dependency, checker, or new harness.
- **Focused closure proof:** Extend the existing selected-local-dependencies
  proof with the reachable mixed, client-less route shape and show that both
  dependency families work from the same configured values without dispatch
  during loading. Static source inspection must show one `GlobalConfig::load`
  feeding both consumers; do not add a timing fixture or configuration hook.
  Rerun and record that existing selector and the already-existing
  retained-client Python integration named in R1, plus the applicable current
  language-boundary lane required by the remediation. Missing evidence must be
  produced by the implementer; this review did not run it.

## Prior-finding closure

| Stable finding | Result |
|---|---|
| `FIND-TASK-003-1` | CLOSED in source: the shared/Python Workflow retains the loading client through mutation and execution. Its R3-mandated regression rerun is missing and is accounted for in `INV-R4-001`. |
| `FIND-TASK-003-2` | CLOSED: status-triggered renewal precedes pending body collection, sends no replay, and has recorded direct proof. |
| `FIND-TASK-003-3` | CLOSED: recognized error codes use trusted catalog text and remediation. |
| `FIND-TASK-003-4` | CLOSED: plaintext secret-file reading remains private behind the redacted shared reader. |
| `FIND-TASK-003-5` | CLOSED: function-local imports remain removed. |
| `FIND-TASK-003-6` | CLOSED: changed declarations use module-imported bare or role-specific aliases. |
| `FIND-TASK-003-7` | CLOSED: prior identified panic-capable items retain substantive panic contracts. |
| `FIND-TASK-003-8` | CLOSED: durable operations document cancellation and partial progress. |
| `FIND-TASK-003-9` | CLOSED: the existing focused selector records post-dispatch cancellation with one model POST. |
| `FIND-TASK-003-10` | PARTIALLY CLOSED: ambient setup is now off the async polling thread, but the required one-snapshot invariant and closure evidence fail as `INV-R4-001`. |
| `FIND-TASK-003-11` | CLOSED: `force_refresh` documentation distinguishes native send-once behavior. |
| `FIND-TASK-003-12` | CLOSED: `into_skald` documents discarded client/composition context. |

## Verification notes

The implementer records green R3 Rust focused tests, package clippy, package
formatting, and `git diff --check`. Source supports the native-`401` and
documentation claims those checks cover. The evidence does not include the
R3-required retained-client/language rerun and does not exercise the
source-visible double configuration read. Those are part of `INV-R4-001`, not
residual uncertainty. Candidate identity remained unchanged while this report
was written.
