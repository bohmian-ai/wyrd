# TASK-002 R4 Concurrency and Lifecycle Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `b56560e511918efbdd84d8756b13100fc381eda0`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior review and remediation authority: `review/TASK-002-r1/`, `review/TASK-002-r2/`, and `review/TASK-002-r3/`

`HEAD` resolved to the candidate before inspection and again immediately before
this report was written. Per the caller's explicit ruling, AI co-author trailers
are allowed and outside this domain review.

## Reviewed boundary

This review covered the one state-owned Bifrost lifetime, start/shutdown races,
fixed-table startup, dynamic-table describe convergence and cache authority,
shared bounded producer admission, direct-send and producer shutdown ordering,
ambiguous drain retry, stale-schema fencing, and the cumulative Oracle cutoff
test corrections. It traced the relevant `WyrdState`, `BifrostLifecycle`,
`Bifrost`, `WriterPool`, and SDK-journey paths rather than relying on the
implementation summary.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| One writer lifetime and fixed-table preflight | Spec `REQ-127`, `REQ-133`; TASK-002 Scenario 1; `AGENTS.md` ownership and async rules | `observe/lifecycle.rs`, `state.rs`, fixed-table projection checks, lifecycle tests | PASS |
| Terminal shutdown and ambiguous same-handle retry | Spec `REQ-126`, `REQ-133`, `AC-028`; Bifrost uncertainty invariants | `BifrostLifecycle::shutdown`, `Bifrost::drain`, `WriterPool::shutdown`, producer retry ownership, state-level ambiguity test | PASS |
| Describe cache and concurrent first use | Spec `REQ-128`, `AC-025`; TASK-002 Scenario 6 | `Bifrost::writer_table`, `describe_gate`, `cached_writer_table`, `Observe::record`, concurrent-first-use test | PASS |
| Shared bounded admission and drain | Spec `REQ-126`, `REQ-128`; `architecture/bifrost-design.md` resource and shutdown invariants | `WriterPool::{insert,producer_for,write_batch,flush,shutdown}`, `DirectSendPermit`, pool race tests | PASS |
| Server fingerprint fence | Spec `REQ-128`, `AC-025` | `pg_bifrost_e2e::assert_stale_writer_is_fenced` and enclosing real-server readback | PASS |
| Oracle cutoff test changes | Bifrost Oracle self-fence and structured-shutdown authority; repository rule against weakening failing tests | `pg_router_smoke.rs`, `oracle/reader_pins.rs`, `reader_expiry_ordering.rs`, recorded full `verify:bifrost` result | PASS |
| R3 remediation interaction | R3 non-goals and preservation requirements | `04f73570..b56560e5` diff: import-only Rust changes plus TypeScript validation/type changes; no lifecycle, queue, cache, or Oracle behavior changed | PASS |

## Boundary assessment

`BifrostLifecycle::claim` takes the start transition before IO and releases the
mutex before awaiting. Failed or cancelled startup rolls back only `Starting`;
shutdown from `NotStarted` or `Starting` publishes terminal `Closed`, and a
losing `StartClaim` cannot install or reopen a writer. Started shutdown retains
the same `StartedBifrost` on drain failure and publishes `Closed` only after the
same facade drains successfully, preserving ambiguous batch identity for retry.

Explicit-table writes carry their immutable `WriterTable` instead of mutating
the active binding. `writer_table` checks the writer-lifetime cache, takes the
existing owner-wide asynchronous miss gate, checks again, and performs one
describe; `WriterPool::producer_for` then creates at most one producer per table
under its registry lock. The global miss gate is a documented throughput
ceiling, not a correctness gap, and cached lookups bypass it.

Shutdown closes producer admission under the same registry lock used by
producer lookup, waits for admitted direct Arrow sends through
`DirectSendPermit`, attempts every producer, and removes only producers that
reached their drained state. An ambiguous producer remains retained for the
next shutdown attempt. The stale-writer journey independently proves both
flush and replacement-registration refusal at the server fingerprint fence and
reads back no stale row.

The Oracle changes remain test-only. The blocked-renewal proof derives its
deadline from the lease remainder reported by PostgreSQL and retains a margin
that fails before lease expiry; the related expiry-ordering test now waits for
the production authority to close admission instead of assuming the test hook
acts synchronously. The R3 import cleanup changes neither test timing nor
production Oracle behavior.

## Prior-finding closure

| Finding | Status | Evidence |
|---|---|---|
| `FIND-TASK-002-6` duplicate concurrent describes | CLOSED | Cache-check/gate/cache-recheck remains in the shared owner; the concurrent test observes one describe and one producer per table. |
| `FIND-TASK-002-7` non-terminal shutdown | CLOSED | Never-started and in-flight-start shutdowns publish `Closed`; successful started drain also publishes `Closed`. |
| `FIND-TASK-002-8` missing state-level ambiguous retry proof | CLOSED | The same state retries the retained batch identity to one durable settlement, then refuses writes and restart. |
| `FIND-TASK-002-14` missing startup/cache/fingerprint journey proof | CLOSED for this domain | All three SDK journeys cover fixed-table startup refusal and cached reuse; the shared real-server journey proves stale-fingerprint fencing. |

R3 findings `FIND-TASK-002-4`, `FIND-TASK-002-11`, `FIND-TASK-002-16`, and
`FIND-TASK-002-17` do not reopen this boundary. The only Rust edit in their
remediation is dependency-import normalization in test support and the Oracle
test; the cumulative runtime owners above are unchanged.

## Verification and limits

I reran one exact `wyrd-client` nextest expression containing six focused tests:
the terminal never-started shutdown, controlled start/shutdown race, ambiguous
same-state retry, concurrent describe convergence, admitted direct-write drain,
and producer-admission/shutdown lock-order tests. All six passed, with 204
unselected tests.

The task records `verify:bifrost` passing 9/9 at `d6231892`, after every code
commit in the candidate. That run included both Oracle cutoff tests, the three
SDK journeys, the denied describe, and audit-timeout coverage. I did not rerun
the Postgres-backed capability aggregate in this Wave 1 checkout. No test
forces renewal completion and the cutoff to become ready in the same scheduler
poll; that disclosed scheduling edge is not an observable TASK-002 gap because
the production deadline still fences admission and no candidate change alters
the selection logic.

## Findings

No material concurrency, lifecycle, bounded-admission, describe-cache,
shutdown/retry, schema-fence, or Oracle-test finding remains.

## Overall result

**PASS**
