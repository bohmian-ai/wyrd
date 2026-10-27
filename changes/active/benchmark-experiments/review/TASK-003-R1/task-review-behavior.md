# Behavior review — TASK-003

Overall: **FAIL**. The focused behavior proofs pass; two explicit repository/task gates remain unmet.

## Immutable subject and coverage

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Base: `7f422c53ad54e370c24cb971a343ec5e06651109`.
- Candidate: `45641bd5eadf8c86111d82fda6afd9605e218f84`.
- First full review: `git diff <base>...<candidate>`; no prior implementation acceptance carried forward.
- Authority: externally approved working revision 8 of `changes/active/benchmark-experiments/spec.md`, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`. Explicit maintainer approval resolves stale draft wording. Original task: `tasks/TASK-003-production-tracing.md` at candidate.
- Read root rules, task-review skill, spec-driven workflow, agent-doc index, Rust/error/agent-surface and maintainer guides, docs rules, trace-context architecture, relevant reliability/recovery and security audit/privacy authority, and applicable testing guidance. Used CodeGraph before code navigation. Checked HEAD and spec hash remained the recorded subject.
- Inspected the complete changed-file diff, including lockfile/manifests, configuration validation, both subscriber initializers and shared composition, sampler/provider/operator/tenant routing and scrubbing, exporter adapter, new unit fixtures/tests, SDK journey extraction and quiet policy, operational documentation, and task evidence. Followed server config/environment initialization and existing remote-parent/linked-work and authenticated route owners; inspected actual Gateway attempt and Verification block ancestry for filtering risks.
- Existing owners remain appropriate: configuration owns ratio validation; `subscriber` shares composition instead of duplicating initialization; `ServerSampler`, `TenantRoutes`/`TenantExport`, and `OperatorExport` retain destination decisions; the installed SDK retains worker/queue/lifecycle ownership. `FailedBatches` adds only the approved exporter result observation. No concrete duplicate workflow or unearned abstraction was found.
- No source edits, commits, benchmarks, runtime builds, or other reviewers' reports. Orchestrator executed focused verification sequentially; results are in `verification.md` in this directory. Task evidence supplies reported fmt/lints/boundary/docs/config checks; those were not independently rerun here.

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-017: independent log/operator policies; required normal catalog copies survive quiet logs and trace filters | `lib.rs:384–403, 431–468, 531–571`: logs filter applies only to fmt; OTel receives operator selection OR normal catalog/events; both production and capture share composition | V1 passes; production-initialized quiet SDK story passes | PASS |
| REQ-017: unset 0.01, explicit 0/1/intermediate, reject invalid/NaN; one default/validation path | `TelemetryConfig::operator_sample_ratio`, `telemetry_sampler`, server `config.rs:3523–3526`; production calls validation even without endpoint | V2 passes; configuration checks reported in task evidence | PASS |
| REQ-017/INV-010: caller cannot increase operator sampling; retain W3C parent/link identity and children decisions | Unchanged `ServerSampler::should_sample` draws a server id for remote-root operator judgment and follows local decisions; `OperatorExport` admits sampled spans only and clears tracestate | V4 four-way matrix, remote-policy and linked-work coverage; V5/V6 actual identity/link readback pass | PASS |
| REQ-017/INV-010: no endpoint and ratio zero preserve authenticated tenant path, catalog, scrubbing and nonblocking sink | `init` always constructs provider; unchanged `TenantRoutes`, `TenantExport`, safe-field reduction and Scribe outbox remain owners | Quiet V5 and V6 rejection/cap proofs pass; existing V4 scrub/routing/unauthenticated coverage passes | PASS |
| REQ-018: failed counter increments once per error result, never success/queue drop, low cardinality; preserve result and delegation | `export.rs:242–303`, production wraps installed exporter only; counter has no request/tenant labels; flush/shutdown/resource delegate | V3 passes direct failed/successful calls and stock-processor use | PASS |
| REQ-018: retain stock finite nonblocking queue and SDK loss logs, no live-drop metric/parser/replacement/retry | `init` still uses stock `BatchSpanProcessor`; narrow wrapper does not own admission, batching, retries or queue; architecture/operations distinguish failed calls from losses | V3 passes SDK first-drop warning and shutdown total; source confirms no custom processor/queue | PASS |
| REQ-018: bounded failure paths and independent request/tenant results | Production ordering stages tenant copy before operator processor; stock processor admission remains nonblocking and lifecycle delegates; no new request failure propagation | V3 controlled stall/saturation and timed shutdown; V6 outbox cap and real tenant continuation pass. Detailed failure/lifecycle coverage belongs also to selected system reviewer | PASS |
| REQ-018: unsampled errors retain correlated logs subject to logging filters | fmt selection independent of trace sampling; V1 produces an unsampled request and logs its error within request/verification context | V1 passes and explicitly checks correlation and intentional log suppression | PASS |
| REQ-017/019: preserve meaningful engine boundaries and document retained/debug inventory | Full runtime span implementations remain unchanged; task inventory identifies request, queue/admission, stages, dependencies, retries/streaming, background and existing debug spans; inspected Gateway attempt, Verification block and dispatch implementation | V5/V6 representative trace trees pass; inventory is recorded. No arbitrary span-count rule introduced | PASS |
| REQ-019/INV-006: normal/debug/restore policy, SDK warnings, failed-batch limitations, head-sampling limits and scrubbed diagnostic data | Owning trace-context and reliability/recovery sections and existing deploy settings table are updated; no payload/credential/tenant metric labels added | `docs:check` reported passed; docs rules/source review | PASS |
| INV-001/003, AC-010/011: real SDK→Gateway→timeline and other-tenant denial, existing permission/audit paths remain | `act_and_read` preserves original connected-tree, IDs, links and attribution assertions; original operator readback remains in original story; new production quiet story invokes same shared journey; tenant-negative story unchanged | V5 three SDK stories and V6 real-server journeys/cap test pass | PASS |
| No second provider/backend, tail sampler, dynamic service, custom retry/queue/worker, new runtime feature or bypassed SDK/server work | Diff and unchanged provider/processor ownership; only existing workspace `metrics` runtime dependency and test telemetry dependency added | Static full diff and manifest/lockfile review; dependency/tenant checks reported passed | PASS |
| Every new/changed Rust item documents intent, errors and possible panics, including private fixtures/tests | Missing error/panic/field documentation in new telemetry items | Source audit; BEH-001 | FAIL |
| Task V5 explicitly permits existing lifecycle ignores but says add no new ignore; root prohibits gate circumvention through ignores | New quiet journey receives a new `#[ignore]` at SDK test line 191 | Full diff and lane semantics in `TESTING.md:225–234`; BEH-002 | FAIL |

## Material findings

### BEH-001 — VIOLATION: complete changed-item rustdoc gate

**Obligation:** Root `AGENTS.md` says every new or changed Rust item needs rustdoc and missing rustdoc blocks merge. `agent_docs/rust.md` explicitly includes fields, private helpers and tests, requires `# Errors` for fallible functions and `# Panics` for possible panics. This is a mandatory gate independent of runtime probability.

**Locations/evidence:**

- `crates/shared/wyrd-telemetry/src/export.rs:255–272`: `FailedBatches`' wrapped tuple field has no rustdoc; `SpanExporter::export` returns the wrapped export failure but documents no `# Errors`. The adjacent shutdown/flush methods do document errors, so the missing contract is source-visible rather than an inherited documentation policy.
- `crates/shared/wyrd-telemetry/src/lib.rs:683`: new `serve` uses fixture `.expect` calls without `# Panics`. New `production_filters_preserve_tenant_traces` (745), `operator_export_loss_is_observable` (922), and `operator_sampling_defaults_are_explicit` (1033) contain asserts/panics/expect but lack `# Panics`.
- New `Logs::write`/`flush` (789/798) and `Gated::export` (817) are fallible trait implementations without `# Errors` (the no-failure cases can say so). New tuple storage fields in `Staged` (659), `Logs` (777), and `FailedBatchCount` (855) are undocumented.

**Consequence/materiality:** Candidate cannot meet the repository's explicit merge requirement even though focused tests pass. These are newly added items, not an attempt to remediate unrelated old documentation.

**Smallest correction:** Complete rustdoc for the changed/new items in these existing files, documenting their actual errors, fixture/test panic conditions, and field role. Audit the same changed-item set once for remaining omissions (including materially changed methods); no new check, runtime code, fixture or style refactor is needed. Source review and existing format/lint checks provide sufficient proof; do not add documentation tests that mirror wording.

### BEH-002 — VIOLATION: new quiet-policy journey is lifecycle-ignored

**Obligation:** Original task V5 explicitly states, “The existing tests are lifecycle-ignored, hence run-ignored; add no new ignore.” Root rules prohibit using `#[ignore]` to circumvent a gate. `TESTING.md:225–234` explains that ignore excludes tests from default family execution and that family lanes provision Postgres.

**Location/evidence:** `sdks/wyrd-sdk-rust/tests/integration/action_timeline.rs:191` adds `#[ignore = "requires the repository-managed Postgres journey lifecycle"]` to the new `service_action_timeline_survives_a_quiet_operator_policy` story. This attribute is new in the full base-to-candidate diff; it is not the preserved existing story's ignore. V5's checked-in exact command still selects only the two original names, although implementation/review evidence separately ran the new test explicitly.

**Consequence/materiality:** The new regression proof is skipped by default and the task's exact V5 command omits it. A future executor following that recorded command can pass without exercising production initialization under the quiet policy, the central newly fixed failure. Passing an explicitly expanded review command does not repair the delivered proof contract or the express no-new-ignore constraint.

**Smallest correction:** Remove the new ignore, retain the repository-managed Postgres lifecycle for the journey, and update the task's V5 exact selector to include the quiet-policy test. Keep both original stories and their existing lifecycle behavior unchanged. Run that focused three-story command once. This requires no new binary, harness, runtime option, or full language-suite repetition.

## Challenged and non-retained hypotheses

- **Catalog ancestors under filtering:** OTel metadata filtering can elide noncatalog parents (for example `gateway.attempt`) and a later catalog child could then attach to a catalog ancestor. Existing direct-SDK export tests intentionally show `gateway.translate` beneath such a parent, but bypass the subscriber; that fixture alone does not prove a current supported runtime failure. Reviewed actual attempt/provider and block paths and did not establish a required normal catalog descendant under a removed noncatalog parent. Likewise existing debug `verification.block` detail changes with diagnostic verbosity, but it is already explicitly debug detail rather than proof that a required normal behavior boundary is missing. Not retained as a material finding; no speculative routing machinery prescribed.
- **Trace filter cannot suppress normal catalog operator output completely:** Shared OTel selection deliberately unions normal catalog recording with operator diagnostics. New owning architecture states this policy. A preference for destination-specific post-recording filtering is not enough to impose a second filter owner or change the approved behavior.
- **Sampling test capture default differs:** Capture intentionally preserves prior complete default traces while explicit ratios use production validation. It shares subscriber/provider/sampler ownership and documents this test-only default. No production fallback to full export exists.
- **No `WYRD_TRACE_FILTER` mutation in `WyrdServerConfig` environment merge:** Production `resolve_trace_filter` reads the environment directly at initialization, the same owner used by both initializers. Its omission from the config merge does not make the documented override ineffective.
- **Existing lifecycle ignores:** Do not remove the original two ignores or report them as newly introduced debt. BEH-002 concerns the new attribute plus the omitted delivered selector only.

## Verification limits and handoff

Focused V1–V6 tests and whitespace check were rerun by the orchestrator and passed; see `verification.md`. Format/lints, configuration, boundary and docs checks remain implementation-reported evidence. Full language-surface journeys belong to integrated change review and were not requested here. No performance benchmark or broad gate ran.

Desired remediation proof: source audit of changed-item rustdoc, existing narrow owner format/lints, and the corrected three-story V5 selector through the existing Postgres wrapper. No runtime implementation correction or benchmark is justified by these findings.
