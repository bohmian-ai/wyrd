# TASK-003 collateral boundary review

Subject: base `74034459d`, candidate `c0900bff5`; cumulative committed diff.
Approved contract: SPEC-bifrost-write-throughput-and-retention revision 4,
working-tree TASK-003 and its scenario 4 execution record. Unrelated dirty
benchmark files are excluded. This report covers collateral hosted admission,
SDK testing projections and the Forge resource assertion; it does not duplicate
the pipeline/state-owner reviewers' coverage.

## Authority and navigation

Read root AGENTS, Python/TypeScript SDK AGENTS, spec-driven-development,
maintainer-style and applicable TESTING rules. Owning hosted-capacity authority
is `architecture/wyrd-design.md:2360–2440`: charge actual retained inputs,
share the process ledger, pause workloads above their contended share, refuse
new buffers that cannot fit, preserve one deadline and queued deferral.

Traced `Sandbox::occupy_for_test` → WorkloadManager::begin → ByteCharge
attribution → pressure sample; `Sandbox::exec` → execute → process charge →
WorkloadManager::admit → fresh telemetry → worker checkout. Checked
WorkloadManager admission/pressure/targets and OccupiedForTest drop.
Consumers are WyrdTestServer's sanctioned memory hold, the three SDK capacity
journeys, the MCP capacity seam recorded by the task, and the existing
`isolated_capacity` process journeys. Forge's consumer is the real rewrite
fixture in `worker_selects_current_iceberg_files`.

## Findings

No blocking source defect or supported-use regression found in this boundary.

- `sandbox.rs:189–220` attributes the synthetic hold to Verification and keeps
  that operation alive as long as its charge. The initial sample makes pressure
  visible to admission immediately. `OccupiedForTest::drop` aborts absorption
  and removes the charge before its operation field drops; this preserves the
  six existing test controls without adding a production reservation.
- `sandbox.rs:450–489` charges materialized code/globals before admission and
  keeps their charge across the capacity wait. Failed admission and cancellation
  drop the local charge; successful admission attributes the same charge rather
  than charging again. The original bounded deadline, telemetry freshness,
  worker ownership and error catalog remain in the connected path. This is
  consistent with the owner's immediate refusal for a new buffer that cannot
  fit. The test-only hold change alone would otherwise change leave-zero from
  an immediate memory refusal into an admission wait.
- The SDK `submit` stories still assert accepted queued execution and a final
  passed verdict. Removing `wait_for_hosted_waiters(1)` is justified: a queued
  execution may defer at the runner's earlier admission boundary before any
  Sandbox waiter exists. This does not erase the deferral contract: unchanged
  `isolated_capacity.rs:508–516` waits for the production DEFERRED log carrying
  the execution ID while pressure is held, then clears pressure and asserts a
  passed result. Direct wait, immediate refusal and arrival-deadline assertions
  remain in each SDK story. Python/TS native comments and authored Python stub
  describe the changed test hold consistently; no public function signature,
  export or durable client behavior changed.
- `production_routes.rs:1784–1824` still proves the full shared root refuses a
  real rewrite and the free root admits it. Comparing Forge-held bytes to the
  occupant's actual reservation fixes the invalid assumption that no Scribe
  owner exists. The final assertion still requires zero Forge bytes, total
  governed bytes equal only Scribe's tracked bytes, and zero spill files. It
  does not excuse leaked Forge ownership or untracked governed bytes.

## Evidence and limits

Inspected actual assertions and production dependency paths. Scenario 4 records
the repaired Python capacity journeys (3/3), all TS integration files (44/44),
Rust capacity journeys (3/3), MCP capacity, isolated-capacity (5/5), hosted unit
tests (36/36), scoped integration and format/lint results. These are maintainer
packet execution claims; this specialist did not rerun Cargo or journeys.

The TypeScript AGENTS finishing list additionally names `ts:typecheck` and
`ts:napi:check`. Their execution is not recorded in the task: `ts:lints` runs
Biome, and `ts:test:integration` builds addons but does not run these checks.
The orchestrator was asked to obtain `mise run ts:typecheck` followed by
`mise run ts:napi:check` before finalizing review. The latter checks generated
public `wyrd/index.d.ts`; the changed private harness declaration is generated
by `ts:build:testing`, already a dependency of the integration lane. Missing
records are not evidence that the checks failed or were deliberately skipped.

No benchmark was run. Final performance qualification and the recorded TDD
violation remain separate from this collateral source assessment.
