# TASK-001 / R1 independent re-review

**FIX_REQUIRED**

## Immutable subject and prior authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.
- Original full base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- Prior candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Current candidate: `295446005b18604030bf376de6209098a28886f7`.
- Approved [specification r2](../../spec.md), [original task](../../tasks/TASK-001-framework-agnostic-telemetry.md).
- [Prior verdict](../TASK-001-r2-692d390dc-20261015/verdict.md), [prior validated findings](../TASK-001-r2-692d390dc-20261015/findings-validation.md), and [R1 task/evidence](../TASK-001-r2-692d390dc-20261015/TASK-001-R1-telemetry-contract-closure.md).

The review covers the complete original-base-to-current-candidate range. The
latest fix diff was used to locate changed owners, not as the acceptance scope.
The original six findings were hypotheses, not automatically open or closed.
No source, spec, task, index or candidate was changed. Only this new review
packet was written; no implementation, staging, commit, merge, push or deploy
was performed. The implementation-reported joined-flush limitation does not
amend approved per-call flush semantics.

## Independent topology and results

Each discovery role and the follow-up/validator was a separate fresh agent. All
received the immutable subject, full diff, applicable authority and verification;
current discovery conclusions were withheld from other discovery reviewers.
The security reviewer returned its complete report under its no-file-edit rule;
the orchestrator persisted that report verbatim, preserving independent authorship.

| Role | Report | Result |
|---|---|---|
| Behavior acceptance | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariant acceptance | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | PASS |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| Security/tenant/content | [domain-review-security.md](domain-review-security.md) | PASS |
| Concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS |
| OTEL interoperability | [domain-review-otel.md](domain-review-otel.md) | FAIL |
| Durability/lifecycle | [domain-review-durability.md](domain-review-durability.md) | FAIL |
| Focused overlap follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

All nine discovery reports, focused follow-up and independent validation are
required inputs. The orchestrator did not discover or validate its own findings.
Only the validator's confirmed/revised ledger is carried into this verdict.

## Comparison and follow-up

[claim-comparison.md](claim-comparison.md) groups proposals by owner/invariant.
A fresh follow-up was required because system/concurrency closure and concrete
joined-success counterexamples appeared to disagree, and the repeat startup
correction revealed a Python sibling path. It traced all relevant Python/TS
setup/flush/shutdown owners, bridge and application-owned provider lifecycles,
shared native drain/error settlement, and unchanged Rust telemetry.

The disagreement is resolved: the original wait deadline and worker bound are
fixed, while a successful old operation need not cover spans ended before a
later call. The same-state TS setup correction works; Python's separate
get/construct/set owner can still race. No stronger arbitrary-thread context,
lossless crash, export cancellation or exactly-once guarantee was introduced.
The TypeScript docs prerequisite gap is the existing recipe obligation.

## Prior finding closure

| Stable ID | Reconciled status | Source/proof |
|---|---|---|
| FIND-TASK-001-1 | CLOSED | Root declarations use self-contained structural inputs; clean strict package consumer passes; real Agents assignability retained |
| FIND-TASK-001-2 | OPEN | Construction/readback improved, but TS install command omits two modules imported unconditionally by the OTEL entrypoint |
| FIND-TASK-001-3 | CLOSED | Actual Python3/TS1 journeys read after live state.flush, continue native/framework work, hold an active span, and export through the global provider after shutdown |
| FIND-TASK-001-4 | CLOSED | Actual payload-free Responses direct usage and zero/fallback mapping now supported |
| FIND-TASK-001-5 | CLOSED | Managed Python wait is bounded, one attempt retained, callbacks remain usable and native drain occurs on timeout; joined-success regression is a different cause introduced by R1 |
| FIND-TASK-001-6 | CLOSED | TS rechecks after its import await before synchronous install/assignment; overlapping setup unit proof passes |

The same file or broad lifecycle obligation does not force the obsolete-success
regression to inherit the original timeout ID. New genuinely distinct causes
receive IDs 7 and 8; no prior ID is silently repurposed.

## Validated retained ledger

| Stable ID | Class | Observable consequence and selected correction |
|---|---|---|
| FIND-TASK-001-2 | MISSING | A clean TS recipe install cannot load the OTEL module; add existing log/metric exporter peers to its installation prerequisites |
| FIND-TASK-001-7 | REGRESSION | A later Python flush/shutdown can return success while a span completed before its call remains queued; obtain a fresh covering flush after joining within one original managed wait budget |
| FIND-TASK-001-8 | INCORRECT | Two setup calls on one Python state can reject that state's own provider; serialize initialization at that state with a GIL-aware gate, preserving the existing OnceLock/GC owner |

Full discovery dispositions, exact source locations, reachability, prior closure,
standard/native mechanism comparison, correction constraints and focused proof
are in [findings-validation.md](findings-validation.md). No optional suggestion
or unvalidated claim is promoted by the orchestrator. No material new contract,
auth/tenant, API, dependency or persistent-state decision is needed.

## Reconciled acceptance matrix

The validator's complete matrix supplies source and proof for subobligations.
This compact table covers every required REQ/INV/AC ID. PASS is scoped to the
recorded evidence and does not claim every historical lane was rerun.

| Obligation | Result | Closure or retained gap |
|---|---|---|
| REQ-010 | PASS | Bridge metadata, usage, timing, parentage, callback lifecycle and optional typed imports supported |
| REQ-011 | PASS | Three actual Python frameworks and TS OpenAI use the generic native/bridged OTEL paths |
| REQ-012 | FAIL | Owned-provider and TS setup remain correct; same-state Python initialization race: 8 |
| REQ-013 | FAIL | Deadline/drain/error behavior and normal live export work; old joined result is not a later call's barrier: 7 |
| REQ-014 | PASS | Native OTLP fidelity, correlation, sampling/content controls and version disclosures retained |
| REQ-015 | FAIL | TS prerequisite set omits unconditional OTEL module dependencies: 2 |
| REQ-016 | PASS | Real framework/negative matrix and owner/capability collection supported; new overlap branches need focused closure |
| REQ-017 | PASS | Existing ambient/current-token and stock gateway authentication reused |
| INV-001 | PASS | Authentication and server Card/tenant authority retained |
| INV-002 | PASS | No new durable protocol/runtime/registry/export queue or unrelated adapter |
| INV-003 | PASS | Payload-free metadata/errors and application exception semantics retained |
| INV-004 | PASS | Managed/owned lifecycle ownership remains explicit |
| AC-001 | PASS | Real Python3/TS OpenAI execution and readback |
| AC-003 | PASS | Clean package consumer and scoped check evidence supported within task review limits |
| AC-007 | PASS | Bridge fidelity, optional declarations and safe supplied usage supported |
| AC-008 | FAIL | Concurrent same-state Python setup: 8 |
| AC-009 | FAIL | Effective deadline/live readback proved, but later caller may receive obsolete success: 7 |
| AC-010 | PASS | Delivered nested/concurrent/enclosing/error/stream identity cases supported |
| AC-011 | PASS | Sampling/content/auth/tenant-negative boundaries retained |
| AC-012 | FAIL | Clean TS recipe module loading fails without required peers: 2 |
| AC-013 | PASS | Ambient gateway, credential refusal and exporter token freshness supported |

Non-goals remain excluded. Owner-directed Python floor/dev-dependency changes
remain authorized. Reported stock Python export-refusal logging, upstream TS
Chat Completions usage absence and already-active TS span behavior are not
expanded into new requirements.

## Verification and limits

Fresh checks in [verification.md](verification.md):

- Cumulative diff whitespace check: PASS.
- Python actual Responses callback/deadline/owned-correlation selectors: 3 passed.
- TS telemetry unit file: 5 passed.
- Existing packed clean runtime/strict declaration consumer checker: PASS.
- Real managed Python framework journey: 3 cases passed with tracing/Postgres.
- Real managed TS OpenAI journey: 1 selected case passed with tracing/Postgres.

Controlled independent discovery/follow-up probes reproduce the narrow stock
flush-return-to-Future-settlement window and Python same-state initializer race.
The corrected install prerequisite issue is source-local and was reproduced
with an isolated module dependency set. Passing ordinary journeys does not
exercise those schedules. Standard toolchain and current installed development
artifacts were used; broad aggregates/docs/lint records remain historical claims
in the committed implementation evidence, not fresh whole-suite execution here.
No host load, source mutation or production credential was used.

## Handoff

One bounded remediation: [TASK-001-R2-python-overlap-and-docs.md](TASK-001-R2-python-overlap-and-docs.md).
Route directly to `$wyrd-implement` under approved r2; preserve prior evidence
and re-review the cumulative original task afterward. This is not another
specification/plan gate, and implementation does not itself grant review PASS.

No fixes or broad aggregate reruns were performed. The remaining Python races
and incomplete TS install prerequisite block completion.
