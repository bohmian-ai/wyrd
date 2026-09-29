# TASK-002 R5 retry 1 concurrency and lifecycle review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `935cc6324d213414402a1d73d6eeec475965fbcb`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review authority: `review/TASK-002-r1/` through `review/TASK-002-r4/`

`HEAD` resolved to the candidate before inspection and again after the focused
test. The repository has no `.codegraph/` directory, so source and caller
tracing used `rg`, direct reads, and immutable diff inspection. Per the current
user ruling, AI co-author trailers are allowed and outside this review. An
external process later dirtied `crates/wyrd/wyrd-testing/src/server.rs`; that
uncommitted edit is not part of the candidate and was excluded in favor of the
committed `base..candidate` range.

## Reviewed boundary

This review covered the one state-owned Bifrost lifetime, start/shutdown races,
ambiguous same-writer drain retry, bounded producer and direct-send admission,
dynamic-table describe convergence and caching, stale-schema fencing, the
Oracle admission-cutoff test corrections, and the R4 TypeScript serializer's
single-read snapshot semantics. It reviewed the complete cumulative result and
the R4 delta rather than relying on the remediation summary.

## Authority and source coverage

| Boundary | Governing obligation | Source and proof inspected | Result |
|---|---|---|---|
| One writer lifetime and fixed-table preflight | Spec `REQ-127`, `REQ-133`; TASK-002 Scenario 1 | `observe/lifecycle.rs`, `state.rs`, lifecycle tests | PASS |
| Terminal shutdown and ambiguous retry | Spec `REQ-126`, `REQ-133`, `AC-028`; Bifrost shutdown authority | `BifrostLifecycle::shutdown`, `WriterPool::shutdown`, `ambiguous_shutdown_retries_the_same_batch_on_the_same_state` | PASS |
| Bounded admission and drain ordering | Spec `REQ-126`, `REQ-128`; Bifrost bounded-resource rules | `WriterPool::{producer_for,write_batch,shutdown}`, `DirectSendPermit`, pool race tests | PASS |
| Describe convergence and immutable routing | Spec `REQ-128`, `AC-025`; TASK-002 Scenario 6 | `Bifrost::writer_table`, `cached_writer_table`, `Observe::record`, concurrent-first-record proof | PASS |
| Server fingerprint fence | Spec `REQ-128`, `AC-025` | `pg_bifrost_e2e::assert_stale_writer_is_fenced` and real-server readback | PASS |
| Oracle cutoff tests | Bifrost Oracle self-fence and repository prohibition on weakened tests | `pg_router_smoke.rs`, `reader_expiry_ordering.rs`, prior recorded focused/full evidence | PASS |
| TypeScript exact-value snapshot | R4 `FIND-TASK-002-4`; accepted values must not change between validation and native admission | `strictJson`, all four callers, `mediaJson`, accessor and media-descriptor unit cases | PASS |

## Assessment

The cumulative Rust lifecycle and queue owners are byte-for-byte unchanged from
the R4 candidate. `BifrostLifecycle` still claims startup before IO, prevents a
losing start from publishing after shutdown, retains `Started` after an
ambiguous drain failure, and publishes `Closed` only after successful drain.
`WriterPool` still closes admission and snapshots producers under the same
registry lock, waits for admitted direct sends, attempts every producer, and
retains only ambiguous undrained producers for retry. The shared describe door
still uses cache-check, owner-wide gate, cache-recheck, then one remote
describe; explicit `WriterTable` routing avoids concurrent active-table swaps.

The R4 correction closes the remaining exact-value race without adding a
second serializer or synchronization mechanism. `strictJson` recursively
constructs the plain value it validates and stringifies that snapshot, so each
ordinary object property or array element is read once and later getter values
cannot replace the admitted value. All public callers (`drift`, `eval`, and
`record`, with `mediaJson` feeding Eval media) continue through this owner.
`mediaJson` checks each original descriptor's own keys before projection, reads
the four declared fields once through one destructuring operation, and sends
the projected snapshot through `strictJson`. The focused tests exercise root,
nested, and array accessors through all three emission paths, unsupported first
values, closed media keys, and a getter-backed `mediaType`, while asserting
zero native calls on refusal and one read on acceptance.

No R4 code change touches the Oracle, lifecycle, describe-cache, producer,
bounded-queue, shutdown, retry, or server fingerprint paths. The previously
disclosed owner-wide describe gate remains a throughput ceiling only; cached
lookups bypass it and no correctness obligation requires per-table gates.

## Prior-finding closure

| Stable finding | Status in this domain | Evidence |
|---|---|---|
| `FIND-TASK-002-4` | CLOSED | The serializer stringifies its one-read validated snapshot, and original media descriptors are checked before their declared fields are read once and projected. |
| `FIND-TASK-002-6` | CLOSED | Concurrent first uses still converge through cache-check/gate/cache-recheck to one describe and one producer per table. |
| `FIND-TASK-002-7` | CLOSED | Successful shutdown remains terminal before startup, during startup, and after a started drain. |
| `FIND-TASK-002-8` | CLOSED | An ambiguous drain retains the same state, producer, and batch identity for retry. |
| `FIND-TASK-002-14` | CLOSED | SDK journeys prove startup refusal and cached reuse; the shared real-server journey proves stale fingerprint refusal. |

## Verification and limits

I independently ran:

```text
mise exec -- pnpm exec vitest run tests/unit/observe.test.ts
```

It passed all 10 tests at the immutable candidate. The supplied evidence also
reports TypeScript unit, integration, typecheck, formatting, lint, and diff
checks passing. `verify:bifrost` was still running when this review was
requested, so no completed R5 aggregate result is claimed here; that is a
verification limit for the orchestrator, not evidence of a concurrency defect,
because the R4 delta changes only the TypeScript serializer/tests and the
focused test directly exercises that behavior. No test forces a lease renewal
to complete in the exact scheduler poll that reaches the Oracle cutoff; the
production deadline still fences admission, and the candidate does not change
that logic.

## Findings

No material concurrency, lifecycle, bounded-admission, shutdown/retry,
describe-cache, stale-fence, Oracle-test, or TypeScript single-read finding
remains.

## Overall result

**PASS**
