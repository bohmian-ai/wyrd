# TASK-002 R3 Concurrency and Lifecycle Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verified-change-contract`
- Base: `c8bb490ad814c0c7770cac33ed7779897ff776e4`
- Candidate: `04f73570397d5123eb767abafa60d016c37de1db`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-002-scoped-observation-and-bifrost-tables.md`
- Prior remediation authority: `review/TASK-002-r1/TASK-002-R1-close-scoped-observation-gaps.md` and `review/TASK-002-r2/TASK-002-R2-close-boundary-and-journey-gaps.md`

`HEAD` resolved to the candidate before inspection and again immediately before
this report was written.

## Reviewed boundary

This review covered the state-owned Bifrost start/close phase machine, fixed and
dynamic table describe gating and cache convergence, bounded `WriterPool`
admission, flush/shutdown drain and ambiguous retry, stale-schema fencing proof,
and the two Oracle lease-test corrections added after the R2 capability run. It
also traced the relevant public `WyrdState` and `Bifrost` callers into the
producer pool and inspected the Rust, Python, and TypeScript journey use of the
shared owner.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| One writer lifetime | REQ-133; TASK-002 Scenario 1; `AGENTS.md` async/ownership rules | `observe/lifecycle.rs`, `state.rs`, lifecycle tests in `observe/tests.rs` | PASS |
| Fixed-table startup | REQ-127; AC-025 | `StartClaim::complete`, full fixed-schema checks, the three SDK preflight journeys, test-server describe fault | PASS |
| Describe cache and convergence | REQ-127/128; TASK-002 Scenario 6; Ponytail minimum-abstraction rule | `Bifrost::writer_table`, `describe_gate`, `cached_writer_table`, shared concurrent-first-use test, per-SDK describe-count assertions | PASS |
| Bounded admission and producer reuse | REQ-126/128; `architecture/bifrost-design.md` ingest ownership | `Bifrost::insert_into`, `WriterPool::{insert,producer_for,shutdown}`, shared producer/byte budget | PASS |
| Drain, terminal close, ambiguous retry | REQ-126/133; TASK-002 Scenario 1 | `BifrostLifecycle::shutdown`, `Bifrost::drain`, `WriterPool::shutdown`, `Producer::shutdown`, state-level ambiguity test | PASS |
| Stale writer fence | REQ-128; AC-025 | `pg_bifrost_e2e::assert_stale_writer_is_fenced` and its enclosing real-server readback | PASS |
| Oracle cutoff test corrections | Bifrost Oracle lease/self-fence authority; repository rule against accepting flaky/pre-existing failures | `pg_router_smoke.rs`, `reader_pins.rs`, `reader_expiry_ordering.rs` | PASS |

## Boundary assessment

The lifecycle claim is taken before connection IO and never held across an
await. A failed or cancelled start rolls only `Starting` back to `NotStarted`;
a shutdown racing a start moves the owner to terminal `Closed`, and the losing
claim cannot publish or reopen it. Once started, shutdown retains `Started` on
any drain error, so a later call reaches the same `Bifrost`, producer pool, and
retained batch identity; it publishes `Closed` only after every producer drain
succeeds. `WriterPool` closes admission under the same producer-map lock used
to resolve producers, waits for admitted direct sends, drains every snapshotted
producer, and retains only non-drained producers for retry.

Dynamic describe uses the existing connected-writer cache with one owner-wide
async miss gate and a second cache check. That is sufficient to converge
same-table first use without a per-key subsystem, while cached reads avoid the
gate. Fixed destinations are described through the same mechanism before the
started writer is published. The shared test proves concurrent first use of
two names yields one describe and one producer per name; all three real SDK
journeys prove repeated dynamic use does not re-describe and fixed projections
do not perform per-observation schema IO.

The stale-writer real-server proof sends a mismatched batch and then attempts a
replacement registration, receives
`WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` for both, drains the settled
refusal, and subsequently reads only the canonical writer's rows. This closes
the R2 requirement at the shared Rust owner without duplicating the same server
fence in thin language wrappers.

The Oracle follow-up changes are test-only. The blocked-renewal test now derives
its absolute bound from the database-reported remaining lease and checks the
production admission cutoff (database allowance plus readiness margin) with
only two seconds of scheduling slack. The reader-expiry test now waits until
the production supervisor observes the test-only collapsed deadline before it
asserts surviving protection. Neither change alters Oracle production state,
lease ownership, renewal, or self-fencing behavior.

## Prior-finding closure

| Finding | Status | Evidence |
|---|---|---|
| `FIND-TASK-002-6` duplicate concurrent describes | CLOSED | Cache check, owner-wide gate, cache recheck, and the concurrent first-use test remain present and passing. |
| `FIND-TASK-002-7` non-terminal shutdown | CLOSED | Never-started and in-flight-start shutdowns publish terminal `Closed`; a losing claim cannot reopen the state. |
| `FIND-TASK-002-8` missing same-state ambiguity proof | CLOSED | The state-level test retries the same retained batch ID through the same writer and then proves restart/write refusal. |
| `FIND-TASK-002-14` startup/cache/fingerprint journey gap | CLOSED for this domain | All three SDK journeys cover both fixed-table failures and cached reuse; the existing shared-owner real-server journey covers stale-fingerprint refusal as the R2 remediation requires. |

## Verification and limits

I reran the exact focused `wyrd-client` nextest expression for five lifecycle
and describe-convergence tests; all five passed, with 205 other tests skipped.
The task records successful focused Postgres runs for both Oracle tests and the
stale-writer journey. `verify:bifrost` passed 9/9 at `4fc251ce`; it was not rerun
after the two test-only Oracle commits, so this review does not claim a fresh
full capability run at `04f73570`. The later Oracle commits have focused test
evidence, and the server integration lane passed after the first of them. No
test covers renewal completion becoming ready in the same scheduler poll as
the cutoff; that is a disclosed test limit, not a TASK-002 behavior gap.

## Findings

No material concurrency, lifecycle, bounded-admission, describe-cache,
shutdown/retry, schema-fence, or Oracle-test finding remains.

## Overall result

**PASS**
