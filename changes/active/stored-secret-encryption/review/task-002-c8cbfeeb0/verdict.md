# TASK-002 verdict: FIX_REQUIRED

Behavioral acceptance: **FIX_REQUIRED**. TDD compliance: **VIOLATED** for original
scenarios 2 and 3. Four independently validated implementation defects remain;
passing adjacent tests does not cover their failing paths.

## Immutable subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; base
`451aa0c4d332af3bf8f334ea9eb1bc9d0679a34a`; candidate
`c8cbfeeb0d5cf3a4fa63bb386cbf460f009ead24` on `revised-encryption`.
Reviewed actual `git diff 451aa0c4d...c8cbfeeb0`, eight commits, with unchanged
candidate source. Only this new review directory was written.

Contract: [approved spec revision 1](../../spec.md), original
[TASK-002](../../tasks/TASK-002-standardize-secret-owners.md), assigned
REQ-001–005, REQ-008–009, REQ-012. Authority: root and applicable nested AGENTS,
changes/spec-driven-development.md, TESTING.md, agent_docs guides;
architecture/wyrd-security-posture.md stored-secret/tenant sections,
architecture/wyrd-design.md Operator, deployment-and-release.md configuration,
and reliability-and-recovery.md backup/restore. Initial-schema replacement has
explicit unshipped-format authorization. TASK-003/004 administration is excluded.

## Independent assignments

- `behavior_rev`: requirement/scenario coverage, init and language journeys,
  composition, provider/CI/docs, cross-tier rules and owner simplicity;
  [primary report](task-review-behavior.md).
- `domain_rev`: auth/platform/Operator security, tenant capabilities, persistent
  context, concurrency and maintenance; [domain report](task-review-domain.md).
- `system_rev`: Gateway preparation, outage/recovery, snapshot lifecycle and
  hot-path waits; [system report](task-review-system.md).
- Fresh `ponytail_rev`: independent coverage and finding validation;
  [validation](findings-validation.md). A single focused domain follow-up
  independently checked the response-semantics conflict before the validator
  retained finding 4: [focused report](focused-response-semantics.md).

No selected coverage is missing. Domain/system cover complex state and recovery;
primary and specialists cover relevant maintainability and repository rules, so
additional invariant, maintainer and repo reviewers would duplicate assigned work.
The orchestrator compiled their reconciled result without acting as a reviewer.

## Reconciled acceptance matrix

| Requirement / original scenarios | Implementation, assertion and execution evidence | Result |
| --- | --- | --- |
| REQ-001 / 1–3,6 | boot build_kms supplies one facade to existing owners; init and actual auth/platform/Gateway/delivery consumers use common envelopes; competing cipher owners removed. | PASS |
| REQ-002 / 1,3 | Existing shared KMS fresh-DEK/nonce implementation reused; current KMS/crypt 21/21 and common-envelope init assertions support cutover. | PASS |
| REQ-003 / 3–4,7 | Operator/Gateway/platform contexts and negative consumer/backend assertions inspected; human connection context omits concrete owner identity. | FAIL: FIND-TASK-002-1 |
| REQ-004 / 3–4,7 | Common envelope, trusted configured key selection and refusal proof retained; KMS units and real Transit rerun pass. | PASS |
| REQ-005 / 1–3,6–7 | Protected persistent init key reproduced; three-language stored connection restart journey assertions inspected; production cloud configuration/facade tests wired. | PASS within accepted CI limits |
| REQ-008 / 3–4 | Remote calls outside transactions, exact SQL fences, redaction and tenant capabilities traced; replacement owner retries after losing its fence. | FAIL: FIND-TASK-002-4 |
| REQ-009 / 2,5 | Restart delivery, warm request counts, generation/listener fencing and immutable snapshots supported; completed failed preparation returns wrong Gateway status. | FAIL: FIND-TASK-002-3 |
| REQ-012 / 1–2,6 | Competing formats/config/readers removed, authorized schema and setup/docs cut over; existing bounded maintenance progress regresses. | FAIL: FIND-TASK-002-2 |

These rows reconcile the validator's complete matrix; exact source locations,
starting states, consequences and smallest corrections are recorded once as
retained findings in findings-validation.md. No evidence-only or speculative
finding is retained.

## Contract reconciliation and closure

Finding 4 is a settled REQ-008 defect, not a specification ambiguity: TASK-002
assigns existing replacement races and requires preserving the winner. The new
retry loop can overwrite another replacement after losing the original fence.
The task's implementation evidence and test expectation of 200 cannot override
the approved conflict requirement. The primary report's description of this loop
as pre-existing is superseded by focused base/current source validation.
Preserving Disabled remains required; returning conflict preserves that outcome.
No task mechanics prevent the settled correction, so neither BLOCKED nor
SPEC_REVISION_REQUIRED applies.

This is TASK-002's first review, not a remediation round. TASK-001 prior acceptance
supports unchanged shared crypto proof; it cannot close new owner integration
defects. No convergence assessment or historical RED reconstruction is needed.

Remediation: [TASK-002-R1](TASK-002-R1-secret-owner-binding-and-runtime-safety.md),
covering FIND-TASK-002-1 through FIND-TASK-002-4 under the same spec revision.
Review does not launch implementation, commit, push, merge or deployment.

## Wait audit and verification limits

System review traces warm managed Gateway calls to process memory with no added
PG/KMS wait. Cold/invalidation loading retains its sanctioned snapshot statement
and queues remote preparation rather than waiting on it. Remote unwrap occurs in
background reload and post-commit administrative warming; Operator delivery and
maintenance remain background work. Administrative sealing is outside DB locks.
No added hot-path Scribe/object-store/Vault wait was identified. Full path inventory
is in task-review-system.md. The fixes must preserve these boundaries.

[Current execution evidence](verification.md): KMS/crypt 21/21, exact initialized
server journey 1/1, managed dev Transit 1/1 and exact Transit selector 1/1, plus diff
check. One direct Transit invocation lacked its task-scoped executable and failed
setup; supplying that declared tool passed. Additional owner journeys, format,
lints, docs, codegen, examples, dependency and tenant gates are recorded historical
implementation evidence; no fresh aggregate run is claimed. The four failing paths
are source-proven and lack their own closing regression proof on this candidate.

Live AWS/Cloud KMS checks and Linux-only production startup remain explicitly
accepted CI limits. No benchmark requested: current root AGENTS and approved spec
withdraw automatic qualification. Scenarios 2/3 remain TDD VIOLATED; current GREEN
and future regression tests cannot erase that history.
