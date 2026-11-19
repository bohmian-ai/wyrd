---
id: TASK-002B
kind: implementation
status: proposed
spec: SPEC-verifier-code-mode
spec_revision: 4
requirements: [REQ-001, REQ-006, REQ-007, REQ-009, REQ-012, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, INV-001, INV-004, INV-006, AC-001, AC-009, AC-012, AC-013, AC-016, AC-017, AC-018, REQ-025, AC-026]
depends_on: [TASK-002A]
parent_task:
remediates: []
---

# Bring the latest resource-scaling runtime and benchmark backend into code-mode

## Outcome and Value

Update the existing resource-scaling integration to the latest committed source
before TASK-003 implements the canonical verification benchmark. Keep both the
code-mode runtime and the newer optimized client, Gateway and sole benchmark
recorder. TASK-003 must extend the current benchmark owner rather than recreate
the timing backend that the optimization branch removed.

This is the maintainer-requested follow-on import. TASK-002A already records the
earlier merge's resolutions and passing checks; preserve that work and its
evidence instead of rewriting its acceptance scope. This task adds no new
product behavior or performance objective.

## What Changes and What Must Stay True

### Integration inputs and current state

Planning inspected these identities on 2026-11-11:

| Input | Identity |
| --- | --- |
| Destination | `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`, branch `change/verifier-code-mode`, HEAD `bcf61769e9c7b9b6632ed93521a695ff72dc36f4` |
| Earlier integration | Active `MERGE_HEAD` is `8d25b13a1ededa23b9eb91973a2b489fe9224557`; TASK-002A records resolved/staged files, verification and an uncommitted merge candidate |
| Latest source | `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`, branch `change/resource-scaling`, commit `2ce7caec7d5a7bd51fb0ba1512f552b0f425ec0f` |
| Source ancestry | Latest source is rebased/divergent from both the earlier imported `8d25b13a1` and prior TASK-002B pin `c9a2e27e6`; common ancestor is `5234961d51798581a026834cfffd5efaadc226b4`. It is not a simple incremental range. |
| Source scope | 247 source commits and 515 changed paths from the shared ancestor at planning. Establish patch correspondence before carrying evidence or resolving repeated changes. |
| Source authorities | Approved resource-scaling revision 5, benchmark-latency-diagnosis revision 7 (retaining TASK-005 revision-6 scope), card-authoring revision 6, and carried Variant/heap-profiling authorities |

### Refresh from the previous pin

The latest source is a direct descendant of the prior `4b37d47fc` pin:
15 additional commits / 70 changed paths. No manifest, lockfile, source packet
or source AGENTS change is present in that delta. Its architecture update changes
the tracing contract: one GenAI inference span per model call, detailed
in-process phases at debug, and tenant behavior copies independent of operator
verbosity, with nearest copied ancestry and the existing safe-field filter.
This delta is incremental from the previous pin; the overall source import is
still divergent from TASK-002A's earlier history as recorded above.

The new implementation also shares response schemas and named edits, forwards
Agent requests by reference with one serialization, avoids collecting validation
violations on valid answers, classifies a temporarily unavailable Gateway as
Busy, and decodes only the unverified token tenant before full verification.
These changes must compose with declared code-mode tools, cleanup and evidence;
none authorizes skipped validation, changed identity or dropped trace evidence.

First inventory the existing staged reconciliation, unstaged spec revision and
untracked tasks/review reports. Do not start a second merge while the earlier
merge is open. Finish TASK-002A's caller-owned integration action and independent
review, preserving its resolutions, before composing the latest source history.
Preserve its code-mode fixes while reconciling source patches that changed identity through rebase; record matching
and materially different patches rather than assuming ancestry equivalence. Do not
reset, abort or reconstruct that merge by discarding its staged work. Task execution
may complete reversible preparation and checks; commits, merge commits, push and
history rewrite still require caller authorization. Report pending Git actions
precisely; a verified index is not committed integrated ancestry.

Import committed history through the pinned latest source, including manifest,
lockfile and source packet changes. Do not cherry-pick only the last performance
commits or copy the source filesystem. Exclude the source's untracked
`changes/active/gateway-latency-capture/` and untracked benchmark review work.
Committed review/remediation packets at the pin are part of its evidence and
must not be mistaken for completed acceptance. The source TASK-005/006 original
review is FIX_REQUIRED; R1 implementation evidence is now committed, but source
closure and the recorded unresolved measurement-reference issue are not passes.
Recheck source/destination identities at execution; a newer source tip
requires recording and inspecting its additional range before refreshing this
pin. New material scope requires planning rather than silently extending it.

### Preserved behavior and limits

- Retain all TASK-001 cleanup/settlement fixes and TASK-002 records, citations
  and output-visibility guidance. Migrate descriptive records to the incoming
  explicit details contract on every shipped surface; preserve compact decisions
  without hidden details IO rather than restoring full default results. Settled
  host work still precedes retries/results; rejected execution never becomes a subject judgment.
- Preserve TASK-002A's shared evidence, borrowed comparison and capture hashing,
  single retained step output and admitted pinned metadata. Do not restore
  disposable payload copies to resolve a compile error.
- Import source client/server benchmark instrumentation and role-aware profiles.
  The Rust driver now already has benchmark recording and heap/CPU hooks; do
  not rebuild those in TASK-003. Native Python/TS drivers belong to TASK-003.
- In benchmark mode, `wyrd-benchmark` is the only application metrics backend,
  including timings, diagnostics, counters and gauges. Production retains its
  existing Prometheus backend. Do not revive `wyrd_telemetry::timing`, parse
  Prometheus in benchmark consumers or count one semantic observation twice.
- Retain source snapshot completeness, process identity, bounds/precision and
  invalid-evidence checks; preserve per-series recording concurrency and grouped
  snapshot coherence. Use its standard persisted diagnostic renderer.
- Preserve the latest shared response-schema and named-edit ownership across
  Prompt binding, Agent request building, structured answer validation and
  provider forwarding. Read-only use must not restore deep schema/body copies;
  malformed responses still follow existing validation/repair paths and exact
  authored edits, citation narrowing and provider formats remain correct.
- A Gateway `WYRD_SERVER_503_SERVICE_UNAVAILABLE` remains a Busy host outcome
  rather than an inconclusive subject judgment. Keep settled tool cleanup and
  queued deferral intact, and retain existing classifications of other codes.
- Import the owning architecture's updated trace catalog: one GenAI inference
  span per model call with integer token usage, debug-only in-process phases,
  and tenant behavior spans independent of operator verbosity. Keep trace/span
  identities, nearest copied ancestry, sampling and safe-field/tenant isolation;
  do not remove required visibility to improve benchmark results.
- `AccessTokenClaims::unverified_tenant` is only a lookup hint before the normal
  verifier checks signature, claims, audience and tenant. HTTP, Gateway and
  native Gate consumers must still reach full verification; parsing this hint
  never grants identity or data access, and malformed/foreign tokens remain
  refused through the existing public errors.
- Preserve SDK serialized-body sharing, per-request Verify deadlines, replay
  identity and token behavior; native raw/borrowed request handling, response
  validation/usage extraction and bounded answer-buffer ownership remain source
  owned. No new auth cache, unchecked parser, transport or fallback policy.
- Keep tenant separation, audit attribution, durability/ACK semantics, source
  dependency pins and production/benchmark activation boundaries intact. Import
  existing approved source changes; do not invent another dependency/feature,
  migration, wire contract or lifecycle owner during reconciliation.

TASK-003 owns the canonical workload and fine-grained provider/interpreter
exclusions. This import neither implements them nor claims its latency target.
Source qualification results retain their recorded workload/binary meanings.
The source also changes public Card authoring and uses local approximate
Gateway quotas under its own approved authority. Preserve that full contract,
its explicit overload/stale/crash behavior and R1 repairs; do not silently
restore strict cross-replica quotas or treat a quota-free benchmark as proof.
Source review findings affecting imported behavior must have closure evidence
or be reported as outstanding at integration review, never erased by green
unrelated checks. This task does not re-plan those source remediations.

The cost here is rebuild and focused regression verification across shared
native/runtime owners and all changed public projections; no new full
performance campaign belongs to integration.

## Approach

1. Preserve and finish the earlier integration under its existing caller-owned
   Git boundary; record its resulting immutable identity and review evidence.
2. Inspect the new source range and recheck authorities; compose its committed
   history into that integrated destination without importing untracked work.
3. Reconcile overlapping code-mode/server/benchmark consumers through their
   existing owners, preserving both branches' invariants and SDK semantics.
4. Run focused checks below, repairing merge-caused gaps at their owner with
   assertions and validity checks intact.
5. Record the integrated candidate, source range, conflict decisions and proof;
   confirm TASK-003's updated references match that candidate and obtain task
   review before beginning the benchmark implementation.

## Ordered Implementation Scenarios

### Scenario 1 — Code-mode records remain readable through explicit details

**Behavior.** Immediate/queued calls return compact judgments without hidden
network reads. An explicit authorized details read returns the same execution's
complete admitted direct/nested step/check records, honest recipients and
original citations, including valid custom data and failed/inconclusive work.
Missing/not-yet-visible details use the existing not-found outcome; permission
failures remain failures and neither case reruns judgment or rewrites a verdict.

**RED.** Extend the existing public code-mode journeys at the contract-composition
seam, using the focused Rust/Python/TS/MCP commands below. First expose missing
check records in details, a dropped recipient, incorrect typed transformation,
or an unintended details read. Add only missing assertions; compilation conflicts
are not RED. Already-correct owner paths use recorded regression proof rather
than manufactured production churn. Source compact/detail journeys additionally
pin that existing boundary, not a replacement synthetic unit test.

**GREEN.** Compose complete code-mode recording with `RequirementDetails` and
existing step reports, engine/result staging, explicit details readers and SDK /
HTTP/MCP projections. Preserve legacy default-empty calls and model recipient
on historical descriptive records. Update affected consumers and docs through
their owners, without full-default compatibility aliases or new endpoints.
Rerun each affected public proof after shared contract changes.

**REFACTOR.** Reuse the source compact/detail owner and code-mode dispatch,
settlement and capture owners; delete duplicated field mapping or hidden-read
workarounds. Keep code-mode runtime/call limits and source compact semantics.

### Remaining import proof


History composition, already-correct optimizations and documentation use
regression/static proof. Only new executable contract composition above needs
behavioral RED; do not manufacture one for already-correct source behavior. Existing
source execution evidence is context, not proof of the combined candidate.
When reconciliation reveals a real executable defect, first extend the smallest
existing affected regression and demonstrate its behavioral failure after the
candidate compiles, then make the minimal owner correction and rerun preceding
proof. Do not count compilation/setup failure as behavioral RED.

Verify the sole recorder and all-engine consumer chain without Prometheus,
source client replay/deadline semantics, persisted report regeneration, and a
real code-mode verification journey. Inspect clean merges as well as textual
conflicts. Add a focused affected seam only when a resolution changes behavior
beyond its source version; record the gap, test selector and expected result.

## Acceptance Criteria

| Obligation / owner | Required observation and proof |
| --- | --- |
| Requested latest import | Earlier integration preserved/completed; exact new source and destination candidate recorded, committed history accounted for, staged/unstaged/untracked inventory preserved. No untracked source work imported. |
| Verifier runtime/authority/evidence/record preservation mapped above | Existing settled-work, call-retention and real public code-mode regressions pass after the import. No rollback of capture, recipient, Busy/timeout/citation or accepted-authority behavior. |
| Source benchmark-latency-diagnosis sole backend/consumer/reporting contract | Recorder/consumer tests prove no mixed backend or Prometheus dependency, valid complete snapshots and exact counts; canonical reports regenerate diagnostics from saved records without raw artifacts. |
| Source SDK/native/ownership optimizations | Client transport, Gateway/provider and changed runtime owner checks pass. Body sharing/deadlines, validation, response content, audits, bounds and staging retain source behavior. No restored clone or bypass used as merge repair. |
| Latest Agent/schema/trace/token delta | Shared schema edits and borrowed request forwarding preserve output/repair/citation behavior without restoring disposable copies. Busy outcomes retain cleanup/deferral. Tenant trace copies retain safe identities/ancestry independently of operator verbosity. Full signature/claims/tenant verification follows every tenant hint. Added focused owner/negative checks below pass. |
| Compact/details composition, REQ-025 / AC-026 | All three SDKs plus HTTP/MCP preserve compact verdicts and explicit detail records/custom data; absent/denied details remain honest. Focused public/schema/type checks below pass without hidden IO. |
| TASK-003 handoff | Task uses wyrd-benchmark snapshots/standard reporting and current SDK/provider boundaries; naming remains server = server-induced and client = end-to-end client–server–client. Canonical workload, exclusions and objectives remain TASK-003-owned. |
| Repository boundaries and honest evidence | Locked builds, format/lints, dependency/tenant/generated checks pass; no weakened tests or invented qualification results. Pending caller-owned Git work is explicit. |

## Implementation Reference

Inspected at the pinned latest source; paths guide reuse, not a private allowlist.

| Existing owner / consumers | Change and integration boundary |
| --- | --- |
| `crates/shared/wyrd-benchmark/src/lib.rs::{install,record_timing_parts,BenchmarkSnapshot::since}` and `recorder.rs::BenchmarkRecorder`; server `app/metrics.rs` | Timing moved out of wyrd-telemetry; the new owner handles all benchmark metrics, bounded series and coherent snapshots. Preserve exclusive startup selection and production fallback choice, not a parallel registry. |
| `verification/telemetry.rs::ExecutionTelemetry::emit`, `hosted/blocks.rs` overlap | Source records benchmark durations once and retains ordinary production histograms outside benchmark mode. Preserve code-mode BlockClock and TASK-002A's allocation-test reconciliation while importing updated recorder consumers. |
| `wyrd-testing/src/release_server.rs::LocalServer::{timings,metrics}`, `Metrics::of_snapshot`, capacity consumers | All engines read timings and standard metrics from `/benchmark/snapshot`. Missing or incomplete evidence is an error, not zero. No benchmark Prometheus scrape/parser remains. |
| `wyrd-testing/src/experiment.rs::{Repetition::diagnostic,Record::render}`, `capacity/main.rs` | Source persists bounded per-repetition diagnostics and uses the same renderer for live/saved reports, even after artifacts are removed. Reuse this rather than custom output or scripts. |
| `wyrd-client/src/transport/http.rs::Buffered`, `verify/mod.rs::Target::execute`, state/observe owners | Bytes share serialized bodies across attempts; Verify sets its minimum timeout on the request. Preserve per-call identity/deadline/auth behavior, not a whole-client timeout clone. |
| Gateway request/adapter/native provider owners and Scribe staged/audit projections | Source adds raw/borrowed ownership, combined validation/usage reads and bounded chunk reuse. Import complete owners and their tests; do not reimplement a parser, buffer or audit path. |
| `wyrd-spec::RequirementDetails`, `VerificationDetails`; server execution details reader; public SDK `Execution.details` and MCP details tool | Source compact result omits configuration/provenance/steps; code-mode check records must attach to the descriptive requirement owner and survive typed details conversion. Extend existing owners, not compact defaults. |
| Source SDK Card authoring and Gateway local-governance owners | These are additional approved imported public/security contracts. Compare their rebased patches and source R1 verdict/evidence; preserve complete behavior and route unresolved source findings to their issued remediations. |
| `skald-spec::SharedSchema`, Prompt response-schema lookup/edit owners; Agent request builder/structured validator and runtime provider dispatch | Latest source reuses named schema edits and serialized requests through borrowed ownership. Compose tool declarations and check citation narrowing with these owners; preserve validation/repair and token usage, not old clone-heavy request shapes. |
| `vala-verify/src/blocks/agent.rs::block_error`, `ServerWyrdGatewayCaller` | Latest source maps Gateway service-unavailable to Busy and shares native provider requests; code-mode settlement and exact tool/downstream authorization still surround that path. |
| `skald-runtime/src/dispatch.rs::inference_span`, `wyrd-telemetry/src/export.rs` tenant-copy selection | The changed architecture owns one GenAI model-call span and verbosity-independent tenant copies. Reuse its catalog/sampling/ancestor selection, preserving call accountability and safe fields. |
| `wyrd-auth-verify::AccessTokenClaims::unverified_tenant`; server token extractor and native Gate bearer consumers | Shared partial decode replaces duplicate full/JSON decode before verification. Preserve the full TokenVerifier decision and its original tenant/audience/signature boundaries at every consumer; no new authentication owner. |
| `profile.rs::{Capture,HeapProfiler}`, `wyrd-benchmark::heap` and shared-client observations | Source already profiles driver and server. Preserve explicit activation and process roles; do not claim its native allocator sampler measures Python/V8 heaps. |
| `capacity/verification/run.rs::{drive,call}` | Despite newer SDK internals, its timer still surrounds run/context preparation and validation. TASK-003 must narrow that public-call boundary; no blanket claim that source already implements canonical timing is valid. |

The earlier integration task and evidence remain the owner of its initial engine
and hosted-artifact resolutions. Review this additional range against that
result, rather than redoing those changes from the original branch base.

## Verification and Evidence

Read merged rules/tasks and run Cargo-backed work sequentially through mise.
Confirm exact selectors after integration; zero selected tests are not proof.

### Imported recorder, SDK and reporting

```sh
mise exec -- cargo nextest run --locked -p wyrd-benchmark --lib
mise exec -- cargo nextest run --locked -p wyrd-client --test integration -E 'test(/^transport::http::/)'
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=tests::all_engine_consumers_read_benchmark_snapshots_without_prometheus)'
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=experiment::tests::reports_keep_repetition_diagnostics_after_artifacts_are_removed) | test(=experiment::tests::reports_regenerate_from_persisted_records) | test(=release_server::tests::snapshot_conversion_rejects_incomplete_metric_evidence)'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=app::metrics::tests::benchmark_mode_installs_only_benchmark_recorder) | test(=verification::hosted::blocks::tests::histogram_overhead_excludes_real_overlapping_blocks)'
```

### Latest Agent, schema, trace and token preservation

These are existing source owner tests. Package/library lanes cover the shared
schema, request/validation/forwarding and token-negative branches. The exact
checks pin changed host-outcome and tenant tracing behavior:

```sh
mise exec -- cargo nextest run --locked -p wyrd-skald-spec -p wyrd-skald-agent -p wyrd-skald-runtime -p wyrd-auth-verify -p wyrd-telemetry --lib
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(=blocks::agent::tests::unavailable_gateway_is_busy_and_other_problems_are_classified)'
mise exec -- cargo nextest run --locked -p wyrd-telemetry --lib -E 'test(=export::tests::sampling_matrix_holds_with_and_without_an_operator) | test(=export::tests::debug_catalog_span_passes_its_place_to_its_children)'
```

Inspect full-verification call paths after merging, not only partial tenant
parsing. Preserve the source tests' signed-token/wrong-signature/cross-tenant
negative coverage and the code-mode public permission journey below. If a
reconciliation alters an HTTP/native Gate consumer beyond its imported form,
add the smallest affected negative transport proof through its existing owner;
record the exact selected command rather than trusting a unit helper alone.
Do not add an unbounded security or full SDK suite to this import.

### Public code-mode and compact/detail composition

The existing destination code-mode tests acquire explicit details for their
record assertions. Confirm preserved selectors after reconciliation. Build
server and dev Python/TS bindings sequentially with existing `build:server`,
`py:setup`, `ts:build`, `ts:build:testing` tasks before real journeys as needed.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::code_mode_preserves_authority_and_records)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/integration/test_verifier_building_blocks.py -k "code_mode_reduces_evidence_and_preserves_citations or verification_decisions_and_details_are_separate_reads"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/verifier-building-blocks.test.ts -t "code_mode_reduces_evidence_and_preserves_citations|verification_decisions_and_details_are_separate_reads"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all -E "test(=pg_openapi_contract::verification_tool_records_match_served_contract) | test(=pg_verification_routes::details_preserve_visibility_and_read_authority)"'
mise run py:format:check
mise run py:lints
mise run py:typecheck
mise run ts:format:check
mise run ts:lints
mise run ts:typecheck
mise run ts:napi:check
mise run ts:pack:check
mise run docs:check
```

Source has the details-visibility and separate-read tests; destination has the
code-mode/schema tests. Preserve/compose both and document exact selector changes.
The served-schema proof now checks descriptive call arrays and compact omission.

### Code-mode runtime and changed owner preservation

```sh
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(=blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks) | test(=blocks_agent::timed_out_attempt_settles_before_retry_or_result) | test(=blocks_agent::agent_check_settles_before_result)'
mise exec -- cargo nextest run --locked -p wyrd-client -p wyrd-skald-providers -p wyrd-gateway --lib
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations)"'
mise run fmt:check
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:check
mise exec -- git diff --check
```

These prove the changed owners and a focused real public composition. Full SDK
journey suites, broad aggregate gates and qualification workloads remain at
final change review or TASK-003, as appropriate. Add only focused Bifrost/server
Postgres slices if reconciliation changes their production paths beyond the
imported source, using the repository wrapper and owning migrations. Preserve
supplied source performance/review limitations without rebranding them as
integrated evidence.

## Material Stop Conditions

Stop for the owning specification for public/persisted, authority or lifecycle
changes beyond the already approved source and revision-4 compact/details
composition. This task explicitly permits that selected public projection
migration; it does not permit new tenant/audit/ACK semantics, cleanup or
resource ownership, recorder activation or approved benchmark boundaries. No
partial merge that drops one side is an acceptable substitute. New source
scope outside the pin requires refreshed planning. Routine borrow/type/caller
and selector/setup repairs remain implementation-owned. Preserve the pending
merge and local work when caller-owned Git action is needed; do not abort or
commit it on this task's authority alone.

## Authority Links

- [Verifier spec revision 4](../spec.md), retaining runtime revision 2
- [Earlier integration and evidence](TASK-002A-integrate-resource-scaling.md)
- [Canonical benchmark task](TASK-003-canonical-sync-verification-benchmark.md)
- Pinned source: `changes/active/resource-scaling/spec.md` revision 5 and `changes/active/benchmark-latency-diagnosis/spec.md` revision 7 and `changes/active/card-authoring/spec.md` revision 6
- [Workflow](../../../spec-driven-development.md), [execution](../../../implementation-execution.md), [testing](../../../../TESTING.md)
- [Agent surfaces](../../../../agent_docs/agent-surfaces.md), [maintainer style](../../../../agent_docs/maintainer-style.md), [Verifier authority](../../../../architecture/wyrd-design.md#agents-tools-and-workspaces)

## Implementation Evidence

### Identities and Git actions

| Item | Identity |
| --- | --- |
| Source | `change/resource-scaling` at `2ce7caec7` (rechecked at execution; unchanged from the pin) |
| TASK-002A merge | committed unchanged as `95d201a4f` (tree `0d4bfae2c` = recorded index); packet files `08c842785` |
| Latest merge | `ff91d90ab` (parents `08c842785`, `2ce7caec7`) |
| Follow-on commits | `08a3c66a3`, `472ab3f33`, `c6987f19c`, `919c44247` (details-read migrations of code-mode proofs) |

Commits were authorized by the caller for this run. Not pushed. Untracked
work left alone: the destination's `changes/active/enterprise-readiness/` and
`changes/active/gateway-benchmark-payload-sizing/`. The source's untracked
`changes/active/gateway-latency-capture/` and
`benchmark-latency-diagnosis/review/TASK-001-review-002-67ee01bc5/` are not
in any commit (`git ls-tree -r HEAD` has 0 matches).

### History composition

Because the source was rebased, its merge base with the destination stayed
`5234961d5` (247 commits, 515 paths). A default-base merge produced 41
conflicts and silently reintroduced superseded content: the removed
`wyrd-server/src/heap_profile.rs` (moved to `wyrd-benchmark::heap`), its
`lib.rs` module and `Cargo.lock` deps, and a duplicate `pg_tests` module in
`hosted/blocks.rs`. The candidate instead uses the merge result whose base is
the earlier-imported `8d25b13a1`
(`git merge-tree --merge-base=8d25b13a1 HEAD 2ce7caec7`). Rebased source
patches present in both histories therefore compose once, source deletions
and replacements win where code-mode did not edit the file, and only 11 files
needed reconciliation. `MERGE_HEAD` stayed `2ce7caec7`, so ancestry records
the full source history.

Optimization preservation: every merged file outside the 32 that code-mode
also changed is byte-identical to `2ce7caec7`. That covers the client
transport, Gateway adapters and routing, providers and raw wire, the
benchmark recorder and heap profiler, telemetry export, auth-verify, server
metrics/main/Gateway routes/verification service/runner, scribe outbox
production code, and all of wyrd-testing. Each production difference in the
32 shared files was inspected. All are code-mode's settled `ToolCallsOwner`
records, its sandbox `call_tool` bridge and owned workspace writer. The
source's `PreparedSchemas`/`SharedSchema`, agent phase timing, borrowed
`&ProviderRequest` forwarding and `block_error` Busy mapping are kept.

### Conflict decisions

| File | Decision |
| --- | --- |
| `wyrd-spec/src/verification/execution.rs` | Source compact `RequirementResult`; code-mode's check `tool_calls` lives on `RequirementDetails` and is now carried by `VerificationDetails::decode_data`. The round-trip test reads the historical record through `VerificationDetails`. |
| `vala-verify/src/blocks/{agent,engine}.rs` | Code-mode's `BlockCall` destructuring and `ToolCallsOwner` alongside the source's `phase_start` timing, `PreparedSchemas` and `ExecutedOutcome`/`RequirementDetails` imports. |
| `vala-verify/tests/integration/blocks_agent.rs` | Union of imports; the code-mode engine helper returns `ExecutedOutcome` and reads `requirement.result.*`. |
| `hosted/sandbox.rs`, `wyrd-sdk-rust/src/lib.rs` | Union of imports/re-exports (`ToolCallRecord`, `ToolRecipient` plus details types). |
| `architecture/wyrd-design.md`, verify docs page, Python `.pyi` | Source compact/details text; code-mode check `tool_calls` and `recipient` described on the details read. |
| Python journey and typecheck fixture | Source details reads; code-mode recipient records moved to the details fixture/assertions; retained output compared with details. |

Also adapted after merge (compile or behavior composition, no production
change): code-mode test mocks implement the borrowed `Provider::send(&ProviderRequest)`;
the source's `pg_verification_groups` fixture sets `tool_calls: Vec::new()`;
Rust SDK, TS, Python and MCP code-mode journeys read step/check records from
explicit details; the TS typed check moved to `detailsTyped`; the served
OpenAPI check asserts `tool_calls` on `StepResult`/`RequirementDetails` and
its absence from compact `RequirementResult`; `agent-tools-and-authorization`
docs point at the details read (llms index regenerated).

Diagnosis — MCP code-mode journey after migrating to details:

- **Symptom:** `code_mode_preserves_authority_and_records` failed with
  `execution … never settled`.
- **Evidence:** 838 consecutive `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`
  polls by the `mcp-code-no-query` session; `direct_execution` maps the
  caller's `PermissionDeniedRbac` from `InputResolver::direct`
  (`verification/inputs.rs:449`) to not-found by design.
- **Cause:** the migrated test read the narrowed principal's direct execution
  as that principal, which lacks Bifrost query read. Direct-execution
  visibility follows that permission.
- **Fix site:** test only (independent diagnostician concurred). Execution
  stays on the narrowed caller and the readback uses `granted`, as SDK
  journeys read details as an administrator. No server change.

Selector note: `-p wyrd-telemetry --lib` alone fails to compile at the source
pin too (`log_writer` tests use `crate::tests`, gated by `test-support`).
The family lane unifies that feature. The exact telemetry selectors were run
with `--features test-support`. This was not fixed here (source-owned, not
merge-caused).

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Requested latest import | identities and composition above | merge parents; untracked inventory; 0 untracked source paths in HEAD | PASS |
| Verifier runtime/record preservation | engine/agent/sandbox resolutions | `vala-verify` all 407/407; 3 exact `blocks_agent` settlement selectors 3/3; `wyrd-server`+`wyrd-ai --lib` under Postgres 604/604 | PASS |
| Sole benchmark backend/consumers/reporting | source owners byte-identical | `wyrd-benchmark --lib` 12/12; capacity `all_engine_consumers_read_benchmark_snapshots_without_prometheus` 1/1; 3 experiment/release_server selectors 3/3; `benchmark_mode_installs_only_benchmark_recorder` + `histogram_overhead_excludes_real_overlapping_blocks` 2/2 | PASS |
| SDK/native/ownership optimizations | byte-identical owners | `wyrd-client` `transport::http::*` 26/26; `wyrd-client`+`skald-providers`+`wyrd-gateway --lib` 81/81 | PASS |
| Agent/schema/trace/token delta | source owners kept | skald-spec/agent/runtime + auth-verify + telemetry `--lib` 182/182; `unavailable_gateway_is_busy_and_other_problems_are_classified` 1/1; 2 telemetry export selectors 2/2 | PASS |
| Compact/details composition (REQ-025/AC-026) | details-read migrations | MCP `code_mode_preserves_authority_and_records` 1/1; Python 2/2; TS 2/2; server `verification_tool_records_match_served_contract` + `details_preserve_visibility_and_read_authority` 2/2; Rust `code_mode_reduces_evidence_and_preserves_citations` 1/1; `wyrd-spec --lib` 765/765 | PASS |
| TASK-003 handoff | — | every TASK-003 path resolves in the candidate; `wyrd-telemetry/src/timing.rs` is cited only as removed; `BenchmarkRecorder` in `wyrd-benchmark/src/recorder.rs` | PASS |
| Boundaries and honest evidence | — | `fmt:check`, `lints`, `check:deps`, `check:tenant-isolation`, `codegen:check`, `docs:check`, `py:format:check`, `py:lints`, `py:typecheck`, `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `ts:pack:check`, `git diff --check` all exit 0 | PASS |

Limits: source review findings stay open in the source packets and are not
claimed here (resource-scaling TASK-005/006 FIX_REQUIRED closure, the
unresolved measurement-reference issue). The same applies to source
qualification results. Non-goals were not touched: the canonical workload,
exclusions, latency objectives and native Python/TS drivers belong to
TASK-003. Full SDK journey suites and aggregate gates were not run. TASK-002A
and TASK-002B still need independent task review.

### Follow-up import of `11474b4a9`

The caller asked for the newer source head, so it was merged as `163d6b88c`
(parents `c3821e95a`, `11474b4a9`). That source is a fast-forward of
`2ce7caec7` (37 commits), so the default merge base is `2ce7caec7` and no
explicit base was needed. The merge had no conflicts. Every file only the
source changed is byte-identical to `11474b4a9`. The four files both sides
changed (`wyrd-design.md`, `blocks/agent.rs`, the Python verify stub,
`llms-full.txt`) keep both sides' hunks. `docs:generate` produced no
difference. The source's untracked `gateway-latency-capture/` and
benchmark-latency-diagnosis review directories were not imported.

| Check | Result |
|---|---|
| `fmt:check`, `lints`, `check:deps`, `check:tenant-isolation`, `codegen:check`, `docs:check` | PASS |
| `vala-verify`, `wyrd-spec`, `wyrd-skald-spec`, `wyrd-skald-prompt`, `wyrd-client` (with Postgres) | 1326/1326 PASS |
| `wyrd-server --lib` + the served tool-record, details-visibility and `pg_verification_groups` selectors (with Postgres) | 612/612 PASS |
| `py:setup`, `py:format:check`, `py:lints`, `py:typecheck`, `ts:build`, `ts:build:testing`, `ts:format:check`, `ts:lints`, `ts:typecheck` | PASS |
| Python `code_mode_reduces_evidence…`, `…decisions_and_details_are_separate_reads` | 2/2 PASS |
| TypeScript `verifier-building-blocks` code tests | 5/5 PASS |
| Rust SDK `code_mode_reduces_evidence…`, `code_mode_failure_releases_session`; MCP `code_mode_preserves_authority_and_records` (`--run-ignored all`) | 3/3 PASS |
