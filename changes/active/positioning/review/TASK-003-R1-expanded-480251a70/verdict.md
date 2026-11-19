# TASK-003 expanded remediation — FIX_REQUIRED

## Subject and authority

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.

- Original base: `5775c3795ebd6557d080b34d55dd2a67b8dbad63`.
- Prior candidate: `634e19cbc517293ed2458f30ce73401714f3447d`.
- Candidate: `480251a70f799b8581ca5a4be77983d6f345d5e3`.
- Reviewed remediation: `git diff 634e19cbc 480251a70`; cumulative
  `5775c3795...480251a70` remains context. No ancestry rewrite occurred.
- Approved authority: user-identified working SPEC-positioning revision 6
  (`7f9bdb2a67e6b113b02a58526925b75f91028112`) and
  SPEC-service-identity-and-telemetry revision 17
  (`6561fb1a747687f56a051edfeff23cc1f0e2375f`), plus owning committed
  architecture. The user explicitly approved the architecture update and
  public SDK changes; older task metadata does not veto them.
- Original TASK-003 content:
  `6846032793910170e76c0cc0ec9de2f50cb6ad6f`; prior R1 task plus evidence:
  `6f3484e808b5015efa78abdf0c2b52863082fea5`. Full paths are in
  [review-scope.md](review-scope.md). These are working-content pins, not
  necessarily stored Git objects.

HEAD and all four pins remained unchanged. Source was reviewed from the
commits. Uncommitted test, example, docs/rule and planning followups are excluded
from candidate acceptance; no dirty fix is counted as committed closure.
This review wrote only reports and performed read-only checks/build outputs.

## Round scope and independent coverage

This is the first substantive remediation review. The earlier subject-only
BLOCKED check did not review implementation and does not count as an
unsuccessful remediation round. No two-round convergence assessment applies.

The prior [substantive verdict](../TASK-003-first-634e19cbc/verdict.md) and
[validated matrix](../TASK-003-first-634e19cbc/findings-validation.md) carry
forward unchanged schedule execution/windows, retained evidence/tenant refusal,
routes/dispositions and server ownership. New configuration, provider setup,
flush, marker lowering, changed refund programs and request state/rendering
required targeted expansion. The current scope did not restart discovery of
unchanged tutorials.

| Independent role | Report | Result / coverage |
|---|---|---|
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL proposal; phase/provider lifecycle and interleavings; its concurrency proposal subsequently rejected |
| repo-rev | [standards-review.md](standards-review.md) | FAIL; 70-path cross-language rules, public/generated surfaces, new rustdoc and required unit proof |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL proposal; complete existing-owner comparison; declaration-layout proposal subsequently rejected |
| system-rev | [system-review.md](system-review.md) | PASS; default initialization, optional dependencies, application providers, ASGI and failure/recovery |
| evidence domain-rev | [domain-review-evidence.md](domain-review-evidence.md) | FAIL; canonical lowering, actual spans/native judgments, retained authority and Rust explicit sequence |
| focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED; traced multi-signal race and once-only setters against required restart recovery |
| Separate fresh ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; independent source/materiality/correction validation, reconciled matrix and specialist coverage |

The validator confirmed all selected coverage and omissions. Invariant plus
focused follow-up cover the only composed concurrency boundary. No separate
security/RBAC, SQL/storage/persistence or server-deployment review is required:
those implementation contracts did not change, and ordinary public
grant/tenant/evidence paths retain domain/system coverage. Comprehensive
first-round behavior discovery was correctly replaced by bounded remediation
validation with prior source-backed acceptance context.

[claim-comparison.md](claim-comparison.md) preserves cause grouping. The
follow-up was warranted for the concrete concurrency/recovery disagreement;
other proposals were resolved from source, authority and materiality without
additional discovery. No validator-origin blocker required separate review.

## Reconciled acceptance and closure

The full evidence matrix is in findings-validation.md.

| Obligation | Result |
|---|---|
| FIND-TASK-003-1: schedule reader CLI and direct Zod setup | CLOSED; committed guide/generated text plus supplied isolated-reader execution proof |
| Original five FIND-TASK-003-2 locations | CLOSED; new expanded items reopen the same documentation gate below |
| Request decision/hold/error behavior and one state per FastAPI lifespan | PASS |
| Actual complete/skipped work, plausible equal replies, native no-trace inconclusive and retained exact attribution | PASS evidence semantics |
| REQ-017/AC-016 explicit Rust configure/telemetry/state-flush program sequence | FAIL, FIND-TASK-003-3 |
| Same-name Trace model/mapping lowering, canonical wire slots and serialized values | PASS |
| Default managed configure, opt-out, TS async, frozen repeated/failed configuration and existing-provider ownership | PASS within disclosed evidence limits |
| AC-016 Python/TS missing-optional-package unit proof | Python PASS; TypeScript FAIL, FIND-TASK-003-4 |
| Writerless flush, started drain and closed refusal; ASGI and error settlement | PASS behavior |
| Unchanged server schedule/windows, retained reads/refusal, distinct routes/handoff, honest claims and existing public owners | PASS, carried forward with changed dependencies checked |
| New changed-item Rust field/test documentation | FAIL, revised FIND-TASK-003-2 |

## Validated ledger and rejected proposals

| Stable ID | Decision / source | Remaining correction |
|---|---|---|
| FIND-TASK-003-2 | REVISED / REPO-R1-001 | Document new Trace tuple field and new lifecycle test's panic conditions. Original five fixes stay closed. |
| FIND-TASK-003-3 | CONFIRMED / EV-1 | Add explicit state.configure before Rust managed setup and state.flush after telemetry export, retaining the handle's required force-flush. |
| FIND-TASK-003-4 | CONFIRMED / REPO-R1-002 | Add the prescribed public-configure TypeScript unit case with an unavailable optional telemetry package; source tracing alone is not unit evidence. |

These are bounded mandatory documentation and explicit acceptance/proof gaps;
current trace evidence integrity was not found broken. The detailed ledger
contains exact locations, producer/consumer and sibling traces, violated
obligations, materiality, correction burden and focused proof.

INV-001 is REJECTED: split concurrent Python installation is reachable, but
both attempts then fail; once-only setters and local cleanup exclude damage
to a successful managed owner or existing application provider. Approved
failed-install recovery requires process restart. A mandatory global setup
lock would add an unapproved stronger success guarantee, so no lock or reset
protocol is prescribed.

MAINT-003-R1-1 is REJECTED as blocking: inline/stub duplication departs from a
local convention, but the matching tiny declaration creates no demonstrated
supported typing/runtime defect or second executable lowerer. No mandatory
merge gate was established for that placement. A new private module merely
to enforce shape would impose correction burden without closing a material
failure. No declaration move or optional follow-up is retained.

No specification revision is needed for the three retained corrections.

## Verification and limits

[verification.md](verification.md) distinguishes fresh checks from supplied
and carried-forward evidence. Fresh passes: 102 docs tests, links, reader
commands, static build/Pagefind, 42-page accessibility, Svelte check (zero
errors/warnings), formatting, cumulative diff whitespace and exactly two
Rust unit tests for writerless flush and marker lowering. Broader
Rust/Python/TS unit/lint/typecheck, codegen and actual-program/application
journey results are supplied evidence, not independently rerun wholesale.

Concurrent external edits affected the workspace docs example/instructions and
later generated/content files. Consequently docs execution is explicitly
workspace verification, not a pristine candidate rendering claim. Reviewers
used Git for committed source. New opt-out tests and direct-model followups
are supplemental or human-directed work, not candidate contents or newly
invented reviewer finding IDs. Integration review must verify its actual
combined source, rules and generated text.

No designated verification/gateway/Oracle/Scribe engine hot path changed; no
PostgreSQL, Scribe, object-storage or Vault wait was added or moved in those
owners. SDK configuration, declaration and explicit flush reuse existing
lifecycle/authoring operations. No engine benchmark, full docs:check,
generator mutation, broad journey suite, live-provider or release check ran.

## Verdict and next task

**FIX_REQUIRED**. Route
[TASK-003-R2-complete-sdk-tutorial-proof.md](TASK-003-R2-complete-sdk-tutorial-proof.md)
directly to `$wyrd-implement`. It packages only the three independently
validated corrections, preserves closed/accepted behavior and requires the
exact affected Rust journey plus focused TS unit/docs/format proof. Review
does not implement, stage, commit, merge, push or publish.

Skipped: full journey/docs aggregate reruns and uncommitted followups.
Risk: candidate 480251a70 still lacks the explicit Rust sequence, required TS
negative unit proof and two mandatory rustdoc details.
