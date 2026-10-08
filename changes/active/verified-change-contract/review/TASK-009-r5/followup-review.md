# TASK-009 r5 focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verification-closeout`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Candidate: `11eb8ed2b64b70903f171335723dcc549b463352`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 46
- Task chain: TASK-009 plus R1, R2, R3, and R4E; R4E supersedes R4
- Scope: only the five uncertainties supplied by the review orchestrator

The repository has no `.codegraph/` directory, so navigation used direct source
and caller inspection. `HEAD` matched the candidate before this report was
written. No test or build lane was run.

## Resolution 1 — provider equality aliases distinct provider identities

**SYS-R5-001 is CONFIRMED as a reachable bounded implementation finding.**

`install_run_correlation` accepts `provider: Any` and uses only the duck-typed
`add_span_processor` capability (`sdks/wyrd-sdk-python/python/wyrd/otel.py:239-269`).
Neither REQ-151 nor the public hook restricts a provider's equality semantics.
The outcome owner is a `weakref.WeakKeyDictionary` (`otel.py:200-203`), whose
lookup uses the referent's hash and equality rather than object identity. Thus
two simultaneously live, distinct, weak-referenceable provider objects that
compare equal share one entry. At `otel.py:258-260`, the second object receives
the first object's cached outcome and is never asked to install the processor.
This path is reached directly through the public private-provider escape hatch;
the only non-test private-provider caller in the candidate journey uses that
same hook (`sdks/wyrd-sdk-python/tests/integration/state/test_observe_journey.py:380`).

The consequence is not speculative: a healthy equal peer can be skipped after
either a successful or failed first provider. It then lacks Run correlation,
so its spans cannot satisfy REQ-151 or the AC-032 persisted joins. Existing
tests cover repeated calls on the same object and two ordinary identity-equal
`TracerProvider` objects (`test_observe_surface.py:392-456`); they do not cover
distinct equal objects.

The correction belongs in the existing registration-outcome owner. Preserve
weak lifetime, the existing lock, pre-marking, cached Boolean outcome, and one
attempt per **object identity**, but do not use equality-keyed weak bookkeeping.
The smallest credible closure proof uses two distinct equal, weak-referenceable
duck-typed providers and proves each is attempted once; a failed equal peer must
not suppress a healthy peer. No public API, provider wrapper, dependency, or
retry is needed.

## Resolution 2 — accept-then-raise enrichment semantics

**TEL-R5-001's source diagnosis is true, but it cannot be retained as a bounded
implementation finding under the current internally conflicting authority.
This uncertainty resolves to `SPEC_REVISION_REQUIRED`.**

The runtime path is unambiguous. `install_run_correlation` pre-marks `False`,
passes the shared `_PROCESSOR`, and leaves the cached outcome `False` when the
foreign method retains the processor and raises (`otel.py:236-269`). The
retained processor has no registration outcome or provider identity and stamps
the current scope unconditionally (`otel.py:206-220`). The focused test proves
only one retained processor and repeated `False` results; it never invokes that
processor inside a Run scope (`test_observe_surface.py:414-436`). Therefore an
accept-then-raise provider can enrich spans while the public installation result
remains `False`.

Revision-46 authority requires the opposite observable outcome in two places:

- REQ-151 says a provider whose registration fails, including one that raises
  after accepting, "simply gets no enrichment"
  (`changes/active/verified-change-contract/spec.md:335-342`).
- The changed Run authority says `False` means enrichment is unavailable and
  repeats that a failed accept-then-raise provider gets no enrichment
  (`changes/active/verified-change-contract/architecture/logic/run_api.md:64-67,153-175`).

But R4E, which declares revision 46 authoritative for every listed behavior,
explicitly permits one shared stateless processor and requires the retained
accept-then-raise processor count to remain one
(`TASK-009-R4E-token-free-run-correlation.md:64-94,151-160,206-219`). Once that
shared stateless object is retained, `SpanProcessor.on_start` receives a span
and parent context, not the provider that called it. It cannot distinguish a
failed provider from a successful provider, and a successful registration on
any peer must keep the shared processor active. Consequently the explicit R4E
allowance cannot guarantee the spec's failed-provider no-enrichment outcome.

This is not resolved by interpreting "no enrichment" as merely "no retry":
`run_api.md:64-67` separately defines a `False` result as enrichment unavailable.
Nor may review silently reject the R4E allowance: the spec-driven authority
requires conflicts between a ready remediation task and approved behavior to
return for human revision rather than letting code or tests redefine the spec
(`architecture/references/languages/spec-driven-development.md`, Authority and
Task contract sections).

A revision must choose one observable contract: either failed
accept-then-raise registration may still enrich because the foreign side
effect is unknowable, or a `False` provider must remain inert, which rules out
the permitted shared stateless processor on this path and requires an
activation-aware processor per registration attempt. The current review cannot
select that product/contract behavior. TEL-R5-001 should therefore be revised
from `INCORRECT` with a prescribed code fix to an authority-conflict entry that
requires specification revision.

## Resolution 3 — stale Run authority revision

**MAINT-009-1 is CONFIRMED.**

`changes/active/verified-change-contract/architecture/logic/run_api.md:3` says
the document is approved only through specification revision 34. The same
candidate-modified document now defines the revision-46 import-time context
key, token-free tuple stack, matching-top exit, and at-most-once registration
contract (`run_api.md:130-230`), while `spec.md:1-4,2111-2128` identifies and
records approved revision 46. This is a direct status contradiction in the
owning architecture document, not optional editorial polish. The bounded
correction is to change only the status revision to 46.

## Resolution 4 — superseded R4 remains ready

**REPO-R5-001 is CONFIRMED.**

`TASK-009-R4-stabilize-provider-registration-idempotency.md:1-13` declares
`status: ready` while its body says R4E supersedes it. R4E independently records
`supersedes: [TASK-009-R4]` and revision 46 (`TASK-009-R4E-token-free-run-correlation.md:1-18`).
The task lifecycle permits `superseded` specifically when an approved revision
or replacement task invalidates a task
(`architecture/references/languages/spec-driven-development.md`, Task contract).
Leaving the invalid revision-45 task machine-readable as ready allows task
selection to choose obsolete authority. Change only R4's frontmatter status to
`superseded`; retain its historical body and R4E link.

## Resolution 5 — exact named Rust proof

**REPO-R5-002 is CONFIRMED, and the candidate must correct its durable evidence.**

The candidate-added implementation evidence names these Rust tests:

- `observe::tests::run_for_card_selects_the_initial_view_and_shares_its_invocation`
- `observe::tests::run_for_card_refuses_an_unknown_alias_without_network_io`
- `scoped_run_emits_drift_eval_and_generic_rows` in the `observe_run` target

Their definitions exist at
`crates/shared/wyrd-client/src/observe/tests.rs:655,685` and
`sdks/wyrd-sdk-rust/tests/observe_run.rs:537`. The record instead gives one
non-exact regex command without `mise exec --` for the shared tests and a bare
Cargo command described only as being "under the Postgres wrapper" for the SDK
journey (`TASK-009-run-context-and-python-otel-correlation.md:261,269-272`).
The task itself had already required each new Rust test by exact final name
(`:211-214`).

AGENTS.md section 11 and the spec-driven Test command precision section require
every named Rust test in a task artifact or implementation report to include
and run its exact `mise exec -- cargo nextest run` command with explicit
package, target, and `test(=...)`; environment-owning setup must be complete.
Broader green lanes and a two-test regex run do not replace that durable proof.
The candidate must run and record the two shared tests separately with exact
expressions and record the complete repository Postgres wrapper plus exact
`mise exec --` journey command. No production-code change or new test is
required.

## New or revised proposals

| Proposal | Resolution |
|---|---|
| `SYS-R5-001` | Retain as a bounded implementation finding: registration outcomes must be weakly keyed by provider identity, not provider equality. |
| `TEL-R5-001` | Revise to an approved-authority conflict requiring specification revision; do not prescribe an implementation correction under revision 46 as written. |
| `MAINT-009-1` | Retain as a bounded documentation-status finding. |
| `REPO-R5-001` | Retain as a bounded task-lifecycle finding. |
| `REPO-R5-002` | Retain as a bounded evidence-format and execution finding. |

## Follow-up result

**RESOLVED.** All five uncertainties were traced to their producers, callers,
tests, and governing authority. Four are decision-complete bounded candidate
findings. The accept-then-raise mismatch is a demonstrated runtime fact, but
the approved packet gives incompatible observable and implementation authority,
so its only valid review disposition is `SPEC_REVISION_REQUIRED`.
