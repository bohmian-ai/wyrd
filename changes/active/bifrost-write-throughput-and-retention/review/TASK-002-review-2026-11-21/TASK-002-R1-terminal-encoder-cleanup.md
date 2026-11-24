---
id: TASK-002-R1
kind: remediation
status: implemented
spec: SPEC-bifrost-write-throughput-and-retention
spec_revision: 3
parent: TASK-002
requirements: [REQ-012]
findings: [FIND-TASK-002-1, FIND-TASK-002-2, FIND-TASK-002-3]
---

# Release retained encoder contexts on every terminal shutdown path

After a warmed Scribe stops through direct abort or cancelled graceful shutdown,
its idle encoder contexts must release their shared-root charges even while the
caller retains the Scribe handle. Running synchronous encoding keeps its lease
until it settles and cannot refill a stopped context set. Restore the displaced
capture-owner rustdoc and make the shipped-server lint gate pass in the same
bounded correction.

## Subject and diagnosed gaps

Approved [spec revision 3](../../spec.md), original
[TASK-002](../../tasks/TASK-002-cpu-throughput.md), reviewed base
`5a057524f07d23a515d505caa4955539926d7fee`, candidate
`c40e7245c3ada4a35177e3a8c2d37c42d058c619`.
See [independently validated findings](findings-validation.md).

- **FIND-TASK-002-1 / REQ-012:** only the ordinary graceful-shutdown continuation
  clears encoder contexts. `abort_shutdown`, including cancellation finalization
  and the server's shutdown-deadline path, publishes STOPPED without closing
  retention. A retained Scribe/pool handle keeps native encoder memory and its
  Metadata lease; running work can subsequently restore into the open set.
  Source locations: `scribe/mod.rs:1453,1488–1533`,
  `scribe/preprocess.rs:1009–1031`. Existing cancellation proof does not warm an
  encoder or assert its Metadata retention.
- **FIND-TASK-002-2 / root AGENTS.md mandatory rustdoc:** the inserted
  `RecordedBounds` declaration took the existing `ReadableSource` capture
  description, leaving that owner undocumented (`scribe/memtable.rs:2221–2228`
  and `2313–2314`). Move that existing description back to its owner.
- **FIND-TASK-002-3 / mandatory lint gate:** `PreparedWalAppend::new` remains
  unconditional at `scribe/wal.rs:511`, while production preparation now calls
  `chunked`. All remaining callers are test, `test-support` or `bench-support`
  consumers. The shipped-server command in `mise run lints` fails with
  `associated function new is never used`; the all-features check masks this.

## Correction boundary and preserved behavior

Use the existing shared terminal shutdown owner and `EncoderContexts::clear`
to close retention on all terminal paths. Keep the current closed-set restore
checks and lease lifetime for an encode already in progress. A terminal caller
must not need to drop its handle to return idle scratch. Preserve ordinary
shutdown, duplicate shutdown, durability closeout and recovery outcomes.

Affected consumers are Scribe's direct abort, shutdown cancellation finalizer,
server deadline abort, pool encoding completion, and terminal inspection. Reuse
the existing inline lifecycle fixtures and pool/context tests. Do not add an
executor, detached cleanup, blocking wait, worker, public surface, accounting
category, or alternate shutdown workflow. No fork reversal, codec change,
benchmark run, or unrelated shutdown redesign belongs to this remediation.

## Ordered closure scenarios

1. **Warm terminal paths — FIND-TASK-002-1 / REQ-012.** Use real ingress
   preparation to retain an encoder and assert positive retained encoder bytes.
   Retain the Scribe handle, then exercise direct abort and cancellation of
   graceful shutdown before its normal cleanup continuation. Each terminal
   path releases idle contexts and their Metadata charges; repeated shutdown
   stays idempotent. Extend existing inline `scribe/tests/wal_closeout.rs`
   tests rather than starting a new harness. Confirm the missing behavior RED
   before production changes.

   **Evidence — PASS.** New `scribe/tests/wal_closeout.rs::aborted_scribe_releases_warm_encoder_contexts`
   (direct abort, then `shutdown` and a second abort) and extended
   `cancelled_shutdown_owner_is_recovered_by_abort_finalizer` (warmed before the
   stall; asserted right after the cancelled owner's finalizer). Shared helpers
   `warm_encoder` (real durable append, `encoder_memory > 0`) and
   `assert_encoders_released` (`encoder_memory == 0`, Metadata category
   reduced by exactly the warmed bytes). RED before production change, both
   tests: `a stopped Scribe retains encoder contexts`, left `26327`, right `0`
   (C1). Fix: `Scribe::finalize_shutdown_owners` (`scribe/mod.rs`), the owner
   every terminal path reaches (graceful, `abort_shutdown`, cancellation
   finalizer, server deadline abort), now calls `EncoderContexts::clear`; the
   graceful-only call was removed. GREEN under C1 with the existing
   `duplicate_shutdown_is_idempotent_and_closes_owners` and
   `lane_workers_reuse_charged_encoder_contexts` (4/4). Commit `45fe8b78d`.
2. **Running completion after closure — FIND-TASK-002-1 / REQ-012.** Through
   existing pool/context owners, hold an encode in progress while terminal
   closure runs. Its memory remains charged until settlement; completion or
   failure then releases it and cannot restore a context into the stopped set.
   Keep byte parity and normal reuse assertions green. Use deterministic
   synchronization rather than sleeps or synthetic host load.

   **Evidence — PASS (existing behavior, no RED expected).** New
   `scribe::preprocess::tests::closure_during_encode_keeps_its_lease_until_settlement`
   recreates `EncoderContexts::encode`'s in-flight state deterministically by
   taking the warmed slot's context and lease out, then runs `clear`: the root
   still charges the warmed bytes. Completion restores through
   `EncoderContexts::restore`; failure drops both. Either way the slot stays
   empty and the root charges `(0, 0)`. Byte parity against a fresh writer is
   asserted on the warming encode. Assertion strength: with both closed-set
   checks in `restore` removed, the test fails with `left: (218136, 218136)`.
   The source was restored afterwards (C2). Commit `57fb1bebc`.
3. **Documentation ownership — FIND-TASK-002-2 / AGENTS.md.** Source inspection
   shows the original capture ownership description above `ReadableSource` and
   the adapter's own description above `RecordedBounds`. Formatting is clean;
   no runtime test is needed for this move.

   **Evidence — PASS (source inspection).** `scribe/memtable.rs`: the capture
   ownership paragraph now sits directly above `#[derive] struct ReadableSource`,
   and `RecordedBounds` keeps only its own description. `fmt:check` is clean (C4).
   Commit `06dc38946`.
4. **Shipped-feature lint closure — FIND-TASK-002-3 / mandatory gate.** Keep
   the convenience constructor only within its actual existing consumer boundary,
   or remove it in favor of the existing chunked owner. Production continues to
   use ordered chunks; test/benchmark consumers still compile. Add no feature or
   lint suppression. Both existing commands in `mise run lints` must pass.

   **Evidence — PASS.** `PreparedWalAppend::new` and the `bytes::Bytes` import
   it alone needed outside `#[cfg(test)]` are gated
   `#[cfg(any(test, feature = "test-support", feature = "bench-support"))]`,
   the boundary of every remaining caller (`wal.rs`/`replay.rs` tests and
   test-support helpers, `bench_support.rs`). `new` gained its missing rustdoc.
   Production still calls `chunked`. No feature or suppression was added. The
   first lint run after gating `new` failed on the shipped graph with
   `unused import: bytes::Bytes`, which gating the import fixed. `mise run lints`
   passes both commands, and redux `--lib` clippy passes with `test-support` alone
   and with `bench-support` alone (C3). Commit `06dc38946`.

The narrow credible tier is inline owner tests with the existing temporary WAL
and in-memory object-store fixtures; no Postgres or new wire journey is needed.
Existing exact regressions:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::tests::wal_closeout::duplicate_shutdown_is_idempotent_and_closes_owners) | test(=scribe::tests::wal_closeout::cancelled_shutdown_owner_is_recovered_by_abort_finalizer) | test(=scribe::preprocess::tests::lane_workers_reuse_charged_encoder_contexts)'
```

Record exact focused selectors for newly authored tests before RED. Record
actual assertions, RED/GREEN and the affected shutdown/preprocess regressions
under these scenarios. Finish `mise run lints`, `mise run fmt:check`,
`mise run check:deps`, and `mise exec -- git diff --check`. Broaden only for an affected boundary or
new failure; integrated review owns broad journey suites. Do not manufacture
historical RED for the original task.

## Verification record

Mechanical deviation: the task's selectors name `scribe::tests::wal_closeout::…`;
nextest's actual path is `scribe::wal_closeout::…`. The corrected selectors
below run the same tests.

- **C1:** `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::wal_closeout::aborted_scribe_releases_warm_encoder_contexts) | test(=scribe::wal_closeout::cancelled_shutdown_owner_is_recovered_by_abort_finalizer) | test(=scribe::wal_closeout::duplicate_shutdown_is_idempotent_and_closes_owners) | test(=scribe::preprocess::tests::lane_workers_reuse_charged_encoder_contexts)'`
  The two new or extended tests were RED first, then all 4 passed.
- **C2:** `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::preprocess::tests::closure_during_encode_keeps_its_lease_until_settlement)'`
  passes 1/1, and fails under the mutation described above.
- **C3:** `mise run lints` passes both commands.
  `mise exec -- cargo clippy --locked -p vala-bifrost-redux --lib --features <test-support|bench-support> -- -D warnings`
  passes for each feature alone.
- **C4:** `mise run fmt:check` passes. `mise run check:deps` passes 1/1.
  `mise exec -- git diff --check c40e7245c HEAD` is clean.
- **Affected regressions:** `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner >/dev/null && cargo nextest run --locked -p vala-bifrost-redux --lib -E "test(/^scribe::/)"'`
  passes 443/443 Scribe lib tests, including every shutdown, preprocess and WAL test.

The diff touches only `scribe/mod.rs`, `scribe/preprocess.rs` (test only),
`scribe/tests/wal_closeout.rs`, `scribe/memtable.rs` (doc move) and
`scribe/wal.rs` (cfg and doc). No executor, wait, worker, public surface,
accounting category, fork, codec or benchmark change was made. Broad journey
suites were not run; integrated review owns them.

Behavioral verification: **PASS**. TDD compliance: **PASS**. Scenario 1 was RED
before production; scenario 2 is proven existing behavior with a mutation
check. Scenarios 3 and 4 are structural or lint proofs, and the lint failure
was observed in review and again during this work.
