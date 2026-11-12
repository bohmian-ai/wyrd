# TASK-005 and TASK-006 — FIX_REQUIRED

Most prior findings close, and the final Agent benchmark gains are supported by
the saved canonical records. Four findings remain: unresolved measurement
qualification, a remaining lost policy-reload path, a new public authentication
error regression, and a bounded new mandatory rustdoc subset. All corrections
fit the approved behavior; no specification revision is required.

## Immutable subject and review scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`.
Original base: `0c81715f2ac52efd2b105e454ee309724df1655f`.
Prior candidate: `180879328ac9dde94d7504855e821980e5db0f18`.
Candidate: `2ce7caec7d5a7bd51fb0ba1512f552b0f425ec0f`.

First remediation review of `git diff 180879328 2ce7caec7`, including all
subsequent optimization commits. Continuous ancestry permits carrying forward
the prior source-backed acceptance; no patch correspondence reconstruction is
needed. Cumulative original-to-candidate context remains available. The delta
touches 103 files, including the prior review packet and its implementation
evidence. The untracked TASK-001-review-002 and gateway-latency-capture packets
were excluded.

Authority: approved `changes/active/benchmark-latency-diagnosis/spec.md` revision
7, retaining TASK-005 revision-6 contract; original TASK-005 and TASK-006 under
that packet's `tasks/`; prior review verdict, independent ledger, reports and
both R1 tasks under `../TASK-005-006-review-001-180879328/`. Repository/nested
AGENTS, Rust/maintainer/security/operations/trace authorities govern above task
mechanics. The precise scope, navigation and pre-delegation role applicability
are recorded in [review-scope.md](review-scope.md).

Source remained immutable. No implementation, commit, push, merge or deployment
occurred. Only this review directory was created.

## Independent coverage and reconciliation

| Fresh role/report | Scope and result |
|---|---|
| [Remediation discovery](task-review-behavior.md) | All prior closures, benchmark evidence, compact/details negative flow, Agent/schema/request equivalence: FAIL on remaining FIND-005-1. |
| [Invariant reviewer](task-review-invariants.md) | Sync identity, bounded shared state/reload, stale/full/drain: FAIL on remaining FIND-006-5. |
| [Repository reviewer](standards-review.md) | Broad Rust tiers/bindings, SQL capabilities, migrations, bench isolation and docs: FAIL on new subset of FIND-005-3. |
| [Maintainer reviewer](maintainer-review.md) | Shared schemas, borrowed requests, caches/serialization/telemetry and lifecycle ownership: PASS; no material DRIFT. |
| [System reviewer](system-review.md) | Actual shutdown/retry/listener recovery and deployed failure propagation: FAIL on FIND-006-5. |
| [Data domain reviewer](domain-review-data.md) | Rolling upgrade projections, replay, RLS and persistent ownership: PASS. |
| [Security domain reviewer](domain-review-security.md) | Token trust/error projection and tenant tracing: FAIL on proposed SEC-R2-1. |
| [Independent validator](findings-validation.md) | Independently accepted role applicability/coverage and validated all closure/claims, correction burden and actual canonical evidence. Four retained findings, no validator-origin blocker. |

Concurrency is covered by invariant/data expertise. No Python/TS files or binding
signatures change; repository coverage checks the changed Rust projections.
Prior compact SDK/MCP, stored-evidence, authority and production bench proofs
carry forward with affected consumers checked. No missing specialist or source
gap remains. The [claim comparison](claim-comparison.md) groups the duplicate
reload reports by one cause. No focused follow-up was needed: source resolves
the remaining claims; unique security discovery required validation, not a vote.

The behavior review's unsaturated reload closure is narrowed by the validated
full-queue failure. Other closure rows remain supported. Discovery PASS/FAIL
labels do not substitute for the independently reconciled obligations below.

## Acceptance and prior-finding closure

| Obligation / prior finding | Reconciled evidence | Result |
|---|---|---|
| FIND-005-1: independent references and observer qualification | Recorder startup, per-boundary counts, ResourceWindow calibration corrected; native c64 row reference and Gateway observer cost remain unresolved | OPEN, narrower remaining proof |
| FIND-005-2: unavailable details and failed recording | Real-server proof covers pending/no result, completed/unretained result, exact 404, useful direct compact verdict during WAL failure and recovery | CLOSED |
| FIND-005-3: mandatory documentation | Original exact list corrected; new bounded optimization/helper/trait list independently checked | OPEN, new subset only |
| FIND-006-1: lazy Prompt reference/read order | Stable authored Prompt handle; construction, explicit retarget, named reference, read-order/clone tests | CLOSED |
| FIND-006-2: final measured reductions | Actual clean VALID canonical records, standard repetitions, process/population/byte evidence; candid original attribution limits | CLOSED |
| FIND-006-3: rolling deploy committed usage | Additive migration changes old read projection; current reads strict rows plus other contributions; separate writers/common ledger; PG overlap/settlement/expiry/isolation | CLOSED |
| FIND-006-4: sync authority lifetime | Arc retained before await through SQL/apply; capacity-one completion/cancellation proof | CLOSED |
| FIND-006-5: policy reload remains owed | Ordinary fenced/error retry corrected; saturated retry queue can still lose A while old A remains served | OPEN, same invariant |
| FIND-006-6: accounting drain | Actual Gateway retries with original shared deadline; reports incomplete persistence; temporary/permanent PG failure proof | CLOSED |
| FIND-006-7: stale/full refusal/recovery | Exact 503, no charge, wake and baseline/ack recovery; prior real-server dispatch proof retained | CLOSED |
| Compact/details, full-path profiles/recorder, SDK/MCP contract | Prior source-backed acceptance plus changed consumers and missing-detail proof | PASS except remaining measurement qualification |
| Schema reuse, post-hook validation, repair, Verdict/custom-data/citations | Changed producer/consumer tracing and focused tests preserve effective schema/validation ownership | PASS |
| Production bench-only phases | Prior feature-tree proof; unchanged manifests and cfg activation; changed callers inspected | PASS |
| Authentication and tenant tracing | Full verification, privacy/binding/sampling/nearest copied ancestor preserved; stable malformed-claim error changes | FAIL only FIND-006-8 |
| Owner-native bounded implementation and non-goals | Existing owners extended; no service/global coordination/new API/DRIFT required | PASS |

This is the first unsuccessful remediation review; the two-failure convergence
assessment is not yet triggered. Remaining causes are incomplete proof and an
incomplete bounded-retry correction, plus an optimization-induced error
regression and documentation subset. The next review is limited to these
corrections and affected paths; it must not restart the original task audit.

## Validated finding ledger and remediation

The complete source paths, producer-to-consumer evidence, materiality, correction
burden and focused proof are authoritative in [findings-validation.md](findings-validation.md).

| Stable ID | Status / classification | Material issue and selected correction |
|---|---|---|
| FIND-005-1 | OVERRIDDEN by maintainer (was REVISED / MISSING) | Required native-write reference still fails R+1; Gateway observation cost unqualified. Diagnose actual populations/absent series with existing Gate/recorder/proof owners; add only necessary bounded producer observation if existing evidence is insufficient. Do not assume retry causality or weaken equality. |
| FIND-005-3 | CONFIRMED / VIOLATION | Original docs fixed; new changed items lack required contracts. Add the exact bounded docs split across R2 tasks; no runtime machinery/checker. |
| FIND-006-5 | REVISED / INCORRECT | Full reload queue can drop fenced A while old policy keeps serving. Atomically queue the obligation or evict its snapshot at the existing shared owner; recover through cache-miss 503/background load. |
| FIND-006-8 | CONFIRMED / REGRESSION, VIOLATION | Malformed claims change existing HTTP401 to400/503. Preserve prior classification on failed/unavailable authentication paths while retaining tenant-only success; preserve exchange malformed-subject400 at its existing owner. |

Route the following directly to `$wyrd-implement`:

- [TASK-005-R2 — measurement reference and qualification](TASK-005-R2-resolve-measurement-reference-and-qualification.md): FIND-005-1 and its disjoint RequestJson subset of FIND-005-3.
- [TASK-006-R2 — policy recovery and auth errors](TASK-006-R2-preserve-reload-obligations-and-auth-errors.md): FIND-006-5, FIND-006-8 and the other subset of FIND-005-3.

Both packets diagnose the failing state, select the minimum existing-owner
correction, preserve adjacent behavior and prescribe narrow proof. No new
product/architecture/security/persistence decision or spec revision is needed.
The existing independent validator also read both packets and this verdict for
comprehension and ledger fidelity. Its one correction clarified token-exchange
configuration-error precedence; the packet now preserves that existing order.

## Verification and performance limits

Static review only. Reviewers inspected reported focused tests, fmt/lints and
actual test bodies; these checks were not rerun. The orchestrator rechecked HEAD,
tracked-source immutability and `git diff --check`. No builds, test suites,
migrations, benchmarks, profiles, external provider calls or host load were run.
Full journeys and gate remain final change-review work, not an R2 requirement.

Independent validation opened the original `0626a7b2d` and post-remediation
`bfd1748c9` canonical records, final assertion history
`20261112T030209Z-2ce7caec7.json`, and final model record
`20261112T030343Z-2ce7caec7.json`. Final model CPU medians support 605.936→356.535
µs at c1 and 1035.862→550.633 µs at c64; rates support 1940→3174/s and
22587→37707/s. Three standard repetitions per case, clean unprofiled VALID
identity, unchanged fixture/bytes and separate assertion/model selections are
retained. Assertion c1 CPU shows a small observed increase, 105.993→110.376 µs;
this is not an additional blocker. Original-reference per-reduction attribution
limits remain explicit. Valid final Verification results do not erase the
unresolved Gateway/engine measurement qualification or prove production row loss.

**TASK-005: FIX_REQUIRED — FIND-005-3.** FIND-005-1 overridden by the maintainer; not remediated.

**TASK-006: FIX_REQUIRED — FIND-006-5, FIND-006-8, shared FIND-005-3.**
