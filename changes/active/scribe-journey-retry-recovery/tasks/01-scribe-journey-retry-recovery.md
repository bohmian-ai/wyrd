---
id: SCRIBE-RETRY-T01
title: Reconcile exact staged members before retry and repair the Scribe journey
kind: implementation
mode: DECOMPOSE
status: proposed
spec: SPEC-scribe-journey-retry-recovery
spec_revision: 1
depends_on: []
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007]
---

# Scribe journey and staged-member retry recovery

## Outcome and value

Scribe retries and restarts reuse an exact validated durable member instead of
attempting to encode it into its occupied directory. Recordless residue is
removed only while the exact retained WAL generation proves its ownership;
contradictory evidence remains untouched and fails closed. The private-peer
journey reaches a real HTTPS dial failure, and the unchanged 512 MiB journey
continues to qualify production geometry.

Required execution skill: `$wyrd-implement`.

## Current repository facts

- `PersistenceRuntime::persist_once` stages and registers a member before it
  advances the WAL manifest and publishes due claims.
- `PersistenceRuntime::process_job` reports every later failure with
  `PersistenceCompletion.staged = None`; the shard retains the immutable front,
  marks it retryable, and submits the same generation again.
- `ScribeMemberStager::encode_runs` correctly refuses every occupied member
  directory. The retry currently reaches that guard even when the directory
  contains the exact valid member created by the first attempt.
- `ScribeHotStage::member` already validates a final record and every referenced
  run. `ScribeStagingRuntime::restore` already reconstructs ready/claim,
  hot-source, context, terminal-publication, and cleanup authority after a
  restart.
- `PersistenceFaults::fail_next_manifest_publication` deterministically fails
  after `register_member` and before manifest advancement.
- `write_read::scribe_undialable_private_peer_returns_typed_visibility_failure`
  assumes an `http://` advertised peer even though the production harness now
  advertises `https://`.
- The Scribe journey target is the single `wyrd-testing` `scribe` binary;
  `source_boundary_recovery.rs` owns publication failure/retry/replay behavior.

## Owners, scope, consumers, and prohibited changes

- `ScribeHotStage` owns filesystem classification and validation of one exact
  staged directory.
- `ScribeStagingRuntime` owns reconciliation of that durable fact with the
  ready/claim index, encoding context, hot-source authority, publication
  recovery, staged-byte charge, and cleanup.
- `PersistenceRuntime` owns the one pre-encoding decision and the existing
  stage → manifest → publication order.
- `StagingAssembler` owns exact idempotent reconciliation of recovered ready or
  claimed membership. `ScribeHotSourceRegistry` owns exact idempotent
  reconciliation of memtable, staged, and published authority. Neither gains a
  second lifecycle.
- `wyrd-testing` owns the real client → server → client proof and the corrected
  HTTPS negative fixture.

Do not change `PersistenceCompletion`, the public or persisted contract, the
staged record schema, lifecycle states, dependencies, Cargo features, object
geometry, lane concurrency, timeouts, or the collision guard in `encode_runs`.
Do not edit `qualification.rs`, add retries or sleeps to tests, resurrect a
bench, or create a generic recovery framework.

## Selected implementation architecture

Add one crate-private exact-retry operation to `ScribeHotStage`. It accepts the
expected `ScribeAssemblyKey`, `StagedMemberId`, and `StagedLsnRange` and returns
`Result<Option<StagedMember>, HotStageError>`:

1. An absent directory returns `None`.
2. A directory with no final `member.staged.json` is removable only in this
   operation, because its caller still owns the matching immutable generation
   and WAL range. Remove the exact directory, fsync its key directory, and
   return `None`. Bulk startup scanning continues to preserve uncorrelated
   recordless run bytes.
3. A directory with a final record goes through the existing bounded decode,
   version, run-length, digest, and nonempty validation. Require the decoded
   key, member, and WAL range to equal all expected values.
4. A malformed record, missing or changed run, identity mismatch, unexpected
   entry type, or filesystem failure returns the current typed internal failure
   without deleting or overwriting any evidence.

Add one crate-private `ScribeStagingRuntime` method over that operation. When it
receives a valid member, it reconciles the member through private helpers shared
with `restore`; it does not create a parallel recovery path:

- `Ready` restores or confirms the exact ready ownership, context, staged-byte
  charge, and staged hot-source authority.
- `Claimed` and `Publishing` preserve the recorded claim identity and re-enter
  the current resumable-claim/publication reconciliation route; they never form
  a new claim over the same member.
- `Published` and `CleanupPending` use the current terminal-member cleanup and
  published hot-source facts; they are not registered as ready again.
- An exact in-memory replay is a no-op. An in-memory member, claim, context, or
  hot-source fact that disagrees with the durable record is a fail-closed
  contradiction.
- Staged bytes are admitted once and released once. Reconciliation must not
  count an already live-owned member a second time.

Extract the smallest private per-key/per-member reconciliation helper from
`ScribeStagingRuntime::restore` necessary to make live retry and startup call
the same logic. Add `StagingAssembler::reconcile_recovered`: exact ready or
claim membership is success, absence restores the durable member, and any
different key, claim, member facts, bytes, rows, or ordering is an error. Add
`ScribeHotSourceRegistry::reconcile_durable`: absence restores the durable
authority, `Memtable` advances to it, exact durable equality is success, and
same-stage different identity or a backwards state is an error. Keep the
existing strict insertion and forward-transition methods for non-recovery
callers.

In `PersistenceRuntime::stage_member`, resolve the existing write recipe and
derive the exact assembly key, member ID, and WAL range before transferring the
footer reservation or submitting `StageMember` to the CPU lane. Call the
runtime reconciliation method:

- `Some(member)` returns that ID and continues the existing manifest and
  publication workflow without encoding or registering again.
- `None` follows the current footer transfer, CPU encoding, durable record
  publication, and registration path unchanged.
- `Err` fails the attempt while retaining the immutable generation, WAL, and
  local evidence.

The retry still reports the first manifest failure to the caller. Only a later
successful attempt completes the shard handoff and permits ordinary WAL
retirement. No success is synthesized from the presence of a staged record.

## Ordered implementation scenarios

### Scenario 1 — Exact durable member survives failure, retry, and restart

**Behavior.** A manifest failure after durable registration leaves acknowledged
rows visible once. A live retry and a restart both validate and reuse the exact
member, converge publication once, preserve audit/object identity, and settle
ownership. Recordless exact residue may be cleared for restaging; contradictory
evidence is preserved and refused. Maps REQ-002–REQ-007, every approved
durability/identity invariant, and AC-002–AC-005.

**RED.** First add
`source_boundary_recovery::scribe_manifest_failure_reuses_staged_member_across_retry_and_restart`
to the existing Tier-1 `scribe` target. Use `BifrostClusterSpec::one_mixed()`
with the existing `PersistenceFaults` option retained across node restart:

1. Append a unique first batch, record zero publication audits/objects, arm
   `fail_next_manifest_publication`, and flush.
2. Require the flush error to contain `test manifest publication failure`;
   require the public strict read to return the acknowledged rows once and
   require no hot object or publication audit.
3. Flush again and require success, the same rows once, one publication audit,
   one set of object identities, and zero settled generation ownership.
4. Append a disjoint second batch, arm the same fault, require the same failed
   boundary, then terminate the node abruptly and restart it on its retained
   roots and stable node identity.
5. Require the restarted public strict read to return both batches once, flush
   successfully, retain the first publication identities, add exactly the
   second rows and one audit transition, and finish with no staged, claim,
   scratch, generation, or WAL ownership leak.

The current code fails on the live retry with `already occupies its durable
directory`; it may not be made green by accepting that error or by restarting
earlier. Exact command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey -E 'test(=source_boundary_recovery::scribe_manifest_failure_reuses_staged_member_across_retry_and_restart)' --run-ignored=all"
```

In the same RED step add the supporting unit test
`scribe::staging_runtime::tests::exact_member_retry_reuses_valid_evidence_and_rejects_every_contradiction`.
Using temporary local staging, cover one test table with these decisive cases:

- absent directory returns vacant;
- recordless exact directory is removed only through the retained generation's
  expected key/member/WAL retry call and returns vacant;
- a valid `Ready` record is returned twice without changing its record or run,
  duplicating ready ownership, or charging its bytes twice;
- the same valid member restored into a fresh runtime reaches the same result;
- mismatched key, member, or WAL range, malformed record, missing run, and
  digest mismatch all fail while their bytes remain present; and
- valid `Claimed`, `Publishing`, `Published`, and `CleanupPending` fixtures are
  routed to their existing claim or terminal recovery owners without encoding
  or ready re-registration.

Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::staging_runtime::tests::exact_member_retry_reuses_valid_evidence_and_rejects_every_contradiction)'
```

**GREEN.** Implement the selected `ScribeHotStage`,
`ScribeStagingRuntime`, assembler/hot-source helper, and
`PersistenceRuntime::stage_member` flow above. Keep `process_job` and shard
failure signaling intact: the first flush fails, the immutable/WAL stays owned,
and the next attempt begins by reconciling the durable member. Reuse the
existing startup recovery projections and publication owners for every state.

**REFACTOR.** Keep one filesystem validator, one lifecycle reconciliation path,
and one encode path. Remove duplicate state branching introduced during GREEN;
do not replace the closed lifecycle enum or broaden visibility of private
helpers.

### Scenario 2 — The private-peer journey tests transport, not URI syntax

**Behavior.** The existing public journey verifies the initially advertised
private Scribe socket is reachable, replaces only its address with a valid
unreachable HTTPS endpoint, refreshes registry authority, and receives the
typed visibility terminal. Maps REQ-001, the approved HTTPS/error invariants,
and AC-001.

**RED.** Run the existing test unchanged and retain its present failure at the
`http://` assumption:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey -E 'test(=write_read::scribe_undialable_private_peer_returns_typed_visibility_failure)' --run-ignored=all"
```

**GREEN.** In `write_read.rs`, change only the fixture's scheme handling and
expectation text from `http://`/plaintext to `https://`/TLS, and persist
`https://{closed}` as the unreachable address. Keep the raw `TcpStream`
reachability check because it proves the advertised socket is listening without
performing a TLS handshake. Preserve the registry refresh, public SDK query,
failed terminal, and exact `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE`
assertions.

**REFACTOR.** No endpoint parser, helper, TLS client, configurable scheme, or
production change is earned by this one fixture correction.

### Scenario 3 — Production physical geometry remains qualified

**Behavior.** The unchanged production geometry still closes a real object at
or above 512 MiB, leaves the required residue, and reads every acknowledged row
once. Maps REQ-008, the approved no-weakening invariant, and AC-006.

**RED.** No new test or production behavior is required. `qualification.rs` is
existing acceptance authority and must remain byte-for-byte unchanged. Record
its pre-change result when the current integrated branch compiles, then rerun
the same exact test after GREEN:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test scribe -P journey -E 'test(=qualification::scribe_512_mib_physical_object_qualifies)' --run-ignored=all"
```

**GREEN.** No geometry or qualification edit. If the recovery change breaks
this test, fix the shared production ownership/recovery defect.

**REFACTOR.** Do not reduce input size, target bytes, object-count assertions,
row-group assertions, or exact read-back.

## Cross-scenario closure and expected write set

- `crates/vala/vala-bifrost-redux/src/scribe/hot_stage.rs` — exact filesystem
  classification, cleanup, validation, and focused helpers/tests.
- `crates/vala/vala-bifrost-redux/src/scribe/staging_runtime.rs` — one shared
  live/startup reconciliation owner and focused test.
- `crates/vala/vala-bifrost-redux/src/scribe/persistence.rs` — pre-encoding
  reconciliation and unchanged durable ordering.
- `crates/vala/vala-bifrost-redux/src/scribe/assembly.rs` — the private
  exact-idempotent `reconcile_recovered` operation and focused assertions.
- `crates/vala/vala-bifrost-redux/src/scribe/hot_source.rs` — the private
  exact-idempotent `reconcile_durable` operation and focused assertions.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/source_boundary_recovery.rs` —
  manifest failure, live retry, restart, exactly-once, audit, object, and
  ownership journey.
- `crates/wyrd/wyrd-testing/tests/bifrost/scribe/write_read.rs` — HTTPS fixture
  correction only.

No manifest, lockfile, schema, generated artifact, benchmark, or qualification
file belongs in the implementation diff.

## Verification

Run Cargo-backed commands sequentially. After each scenario's exact RED and
GREEN commands, run:

```bash
mise run test:bifrost
mise run test:bifrost:journey:scribe
mise run fmt
mise run lints
git diff --check
```

`mise run gate` is not the local bar for this focused Scribe change. If any
public, generated, persisted, dependency, feature, audit, tenancy, or data-loss
contract appears to require change, stop with `BLOCKED`; revision 1 does not
authorize it.

## Completion evidence

- Record the RED directory-collision failure and the exact GREEN focused
  results.
- Record the HTTPS setup failure before the fixture correction and the exact
  typed terminal after it.
- Record the unchanged 512 MiB qualification result.
- Record the complete Tier-2 Bifrost and Tier-1 Scribe lane results.
- Audit the final diff against the expected write set and prove no task
  prohibition or approved spec invariant changed.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
- `changes/active/scribe-journey-retry-recovery/spec.md`
