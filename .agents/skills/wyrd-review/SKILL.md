---
name: wyrd-review
description: Orchestrate two independent, adversarial reviews of either an ongoing Wyrd implementation checkpoint or a stable final candidate, then validate and consolidate findings for task alignment, claim satisfaction, correctness, regressions, applicable repository rules, and credible evidence. Checkpoints never grant completion approval.
---

# Wyrd Review

Use `CHECKPOINT` to review mutable, partial work for current correctness,
directional drift, and remaining gaps. Use `FINAL` to adjudicate a stable
candidate for task completion. The default role is `ORCHESTRATOR`; only an
agent explicitly assigned `CANDIDATE` performs one independent review pass,
and only an agent explicitly assigned `VALIDATOR` performs escalation
validation. Remain read-only in both modes and all roles.

Review acceptance is an evidence contribution and candidate-local judgment. It
does not transition a Wyrd `Change`, finalize an `EvidenceManifest`, grant
authorization, imply merge, or replace policy evaluation over finalized
evidence.

## Common review principles

Read `AGENTS.md`, `architecture/agent-rules.md`, the task or remediation
authority, and relevant Wyrd design/doctrine. Follow CodeGraph instructions and
inspect changed behavioral owners plus only the callers, consumers, error and
cleanup paths, tests, and generated surfaces needed to establish task claims
and candidate-introduced regressions.

Review priorities are:

1. alignment with requested outcomes, locked decisions, scope, and non-goals;
2. required claim or acceptance-criterion satisfaction;
3. behavioral correctness, negative paths, regression, and consumer closure;
4. credible evidence and verification; and
5. applicable hard repository rules in changed code.

Be risk-directed. State transitions, rollback, cancellation, concurrency,
durability, tenancy, audit, bounds, and telemetry matter only where the changed
behavior makes them relevant. Mechanical/generated/rename-only changes may be
sampled once equivalence or generation provenance is established.

Map obligations to the strongest applicable evidence. Runtime behavior normally
needs tests; source, schema, documentation, or absence claims may be proven by
static evidence. Commands are not proof when their assertions cannot detect the
claimed defect. Preserve evidence-class distinctions; do not average
deterministic evidence, model evaluation, and human attestation into a score.

Review adversarially without writing an adversarial report. Try to falsify the
candidate's claims, changed invariants, tests, and clean-looking paths. Search
for the strongest reachable counterexample and attempt to disprove each
suspected finding before returning it. Use a neutral, precise tone. There is no
finding quota or reward for severity; aggressive discovery must be followed by
skeptical validation and conservative promotion.

## Independent review orchestration

The `ORCHESTRATOR` captures one review subject and authority packet before
forming conclusions. Include the mode, task/remediation authority, locked
decisions and non-goals, applicable repository and architecture rules, exact
checkpoint snapshot or final base-to-candidate range, claim subset when
applicable, and available evidence. Do not include suspected defects, a desired
verdict, or one reviewer's output in another reviewer's packet.

Spawn exactly two distinct review agents concurrently and assign each
`role: CANDIDATE`. Give both the same neutral packet and require the candidate
ledger below. Dispatch each without inherited conversation or parent-review
context; the neutral packet must be self-contained and must direct the agent to
the applicable repository authorities and this skill's `CANDIDATE` contract.
When the harness exposes history-fork controls, use its no-inherited-history
setting. Distinct passes by one agent are not independent. The candidates must
not communicate, see each other's output, or spawn additional reviewers.
Different models or review lenses are optional and never substitute for
separate agent identities reviewing the full subject. If two isolated agents
cannot be run, return `CHECKPOINT_BLOCKED` or `REVIEW_BLOCKED`; never silently
claim independent review from a single pass.

Candidates inspect the code, relevant callers and consumers, negative and
cleanup paths, tests, generated surfaces, and existing evidence under the
common principles and selected mode. They do not own the consolidated verdict.
They return:

- reviewer identity and exact subject;
- candidate findings with temporary ID, severity, location, authority or
  claim, reachable scenario, observable consequence, required testable
  outcome, supporting evidence, confidence, and attempted falsification;
- claim statuses supported by that review;
- verification or static-analysis limits; and
- for a clean review, the strongest counterexample attempted and why it did
  not establish a finding.

Do not run duplicate Cargo-backed or repository-wide verification concurrently
in candidate agents. Candidates inspect existing evidence and may perform
narrow read-only analysis. The orchestrator owns any necessary command reruns
and executes them sequentially after candidate discovery unless the repository
provides an explicitly safe isolated lane.

## Validation and consolidation

Treat both candidate ledgers as untrusted claims. The `ORCHESTRATOR` validates
every raw candidate finding against the exact source, applicable authority,
relevant caller or consumer, and available tests. True same-root duplicates may
share one validation entry only when it maps back to every raw candidate ID.
Seek evidence that disproves the finding as actively as evidence that supports
it. Agreement raises confidence but is not proof; the same unsupported claim
from both reviewers is rejected, while a source-confirmed finding from only one
reviewer survives.

Classify candidate provenance as:

- `CORROBORATED` — both reviewers independently identify the same root cause;
- `UNILATERAL` — one reviewer identifies it and the other is silent; or
- `DISPUTED` — the reviewers reach materially conflicting conclusions.

Then disposition it as `PROMOTED`, `REJECTED`, or `UNRESOLVED` from source
validation. Deduplicate only findings with the same underlying cause; preserve
distinct consequences when they change severity, required outcomes, or claim
coverage. Retain reviewer provenance, dissent, and a concise rejection reason
in the internal ledger. No candidate finding may disappear without a mapped
disposition. Never compute the verdict by vote, finding intersection, finding
count, averaged confidence, or averaged severity.

Run the narrowest checks needed to adjudicate missing, stale, contradictory,
or suspicious evidence. When a severe finding remains disputed, both reviewers
declare a high-risk boundary clean without strong proof, or adjudication would
reject a plausible `CRITICAL` or `MAJOR` finding, spawn one independent
agent that produced no candidate ledger and explicitly assign
`role: VALIDATOR`. Dispatch it without inherited conversation context. Give it
the captured subject, authority packet, candidate ledgers, and preliminary
dispositions without a desired verdict. When escalation challenges a clean
high-risk boundary rather than a candidate finding, the orchestrator creates a
stable referral ID tied to the exact claim or boundary, the reviewers' clean
conclusion, and the missing or weak proof. Referral IDs organize validation;
they are not candidate findings and carry no presumed defect or severity.

The `VALIDATOR` does not spawn agents, discover a fresh candidate ledger, or
own the final verdict. It attacks the disputed or high-risk preliminary
dispositions from both directions, inspects the exact source and evidence, and
returns a source-grounded recommendation of `PROMOTED`, `REJECTED`, or
`UNRESOLVED` for each referred candidate finding ID or orchestrator referral
ID, including counterevidence and any remaining uncertainty. For a clean-boundary
referral, `PROMOTED` means the validator established a finding that the
orchestrator must record with the referral provenance; `REJECTED` means the
clean conclusion survived challenge. The orchestrator adjudicates that
recommendation and still owns consolidation and the final candidate-local
verdict. If an isolated validator cannot run when this escalation condition
applies, return the applicable blocked verdict rather than improvising
validation.

## CHECKPOINT mode

Checkpoint review judges the implemented slice and trajectory, not whole-task
completion. Accept the task, an optional caller-declared checkpoint scope or
claim subset, an optional task base, current `HEAD`, staged and unstaged diffs,
and relevant untracked files. A clean tree, candidate commit, manifest, digest,
or complete verification set is not required.

At the start, the orchestrator records the available base, `HEAD`,
staged/unstaged path inventory, and relevant untracked paths. Capture diff
digests when practical and send the same captured subject to both candidates.
Never stash, reset, format, or mutate the implementer's tree. Recheck the
snapshot after candidate review and validation. If it changed, scope findings
to the captured state or return
`CHECKPOINT_STALE`; never imply the latest tree was reviewed.

A dirty-tree checkpoint is an ephemeral subject, not an immutable Wyrd
`ChangeRevision`. Its evidence becomes stale when the subject changes.

Classify the visible state:

- `CURRENT_DEFECT` — implemented or claimed-complete behavior is wrong,
  internally inconsistent, regressive, or violates an applicable hard rule;
- `DIRECTIONAL_DRIFT` — the current design conflicts with a locked decision,
  owner, non-goal, or required claim and continuing would compound rework;
- `EXPECTED_GAP` — legitimate unfinished work outside the claimed checkpoint
  slice or a natural later closure step; not a finding; or
- `CURRENT_RISK` — a source-grounded concern that needs later proof but is not
  yet a defect; nonblocking.

Missing final tests are normally expected gaps. They become current defects
when the checkpoint claims proof is complete or an existing test falsely passes
and cannot detect the behavior it claims to establish. An intermediate build
failure is expected only when the task/checkpoint explicitly permits that state
or the breakage is an obvious bounded consequence of work underway.

Run only checks proportionate to the current slice: parse/type/compile,
focused tests, meaningful format/lint, or a check needed to adjudicate a
suspected defect. List final journeys, generated closure, and broad gates as
remaining work rather than requiring them for a clear checkpoint.

Checkpoint verdicts are:

- `CHECKPOINT_CLEAR`
- `COURSE_CORRECTION_NEEDED`
- `DECISION_NEEDED`
- `CHECKPOINT_BLOCKED`
- `CHECKPOINT_STALE`

Every checkpoint response begins:

`Checkpoint review only — this is not task completion approval.`

Report snapshot/scope, verdict, current defects or drift, provisional claim
progress, expected gaps, focused verification, and risks. Use provisional claim
statuses `PROVEN_NOW`, `IMPLEMENTED_UNVERIFIED`, `PARTIAL`, `NOT_STARTED`,
`NOT_APPLICABLE`, or `DRIFTED`. Later changes may invalidate every status.

## FINAL mode

Final review requires repository root, task/remediation authority, and an
immutable, unambiguous base-to-candidate range. The base need not be the
candidate's immediate parent and a merge is reviewable when the caller supplies
unambiguous range semantics. Controller IDs, generations, manifests, and
digests are optional provenance.

When a Wyrd `ChangeRevisionId` is supplied, confirm its base/candidate subject
identity matches the reviewed range. Report a stale or superseded revision as
an integrity problem; never apply evidence from one revision to another.

Both candidates review the same immutable diff before returning their claim
traces. The orchestrator reuses credible verification evidence and reruns only
the narrowest checks needed when evidence is absent, contradictory, suspicious,
stale, or insufficient. Accept an equivalent or stronger canonical `mise`
command unless exact command identity is itself a task requirement. A
candidate-caused failure, weakened gate, ineffective task-critical test, or
unproved required behavior prevents acceptance.

Pre-existing broad-lane failures may be recorded as static limits only when
base comparison plus focused owning-surface evidence proves non-regression and
the task does not own that failure. `NOT_APPLICABLE` requires a short
task-authority reason.

Final verdicts are:

- `ACCEPT`
- `CHANGES_REQUIRED`
- `DECISION_REQUIRED`
- `REVIEW_BLOCKED`

Only FINAL may return `ACCEPT`. Reassess all task changes and required claims;
checkpoint results are hints, never inherited acceptance. When supplied, state
whether prior checkpoint defects and expected gaps were resolved.

## Finding threshold

A blocking finding requires:

- exact source or authority;
- a reachable scenario;
- a concrete observable consequence;
- a violated required claim, contract, or applicable hard repository rule; and
- a testable required outcome.

Use severity proportionately:

- `CRITICAL` — reachable security/tenant breach, data loss/corruption, unsafe
  irreversible transition, or broken durability/audit invariant;
- `MAJOR` — required claim or public/task behavior is wrong or unproved;
- `MODERATE` — reachable in-scope correctness/reliability defect or material
  regression risk with a concrete consequence; or
- `NOTE` — preference, readability, speculative optimization, extra coverage,
  or unrelated/pre-existing concern; never blocks and normally omit it.

High-confidence source/test evidence may block. A medium-confidence finding
must still demonstrate a reachable consequence. Low-confidence hypotheses do
not block: investigate, note only if useful, or omit. Best-practice concerns
block only when they violate an applicable hard rule or create a concrete
in-scope correctness, security, or maintainability risk.

Keep findings compact: ID, severity, location, affected claim/rule, reachable
problem, impact, required outcome, and focused verification. Add class, owner,
or confidence only when routing needs them. Do not report harmless preference
differences or unrelated cleanup.

## Final output

Lead with mode and consolidated verdict, then validated blocking findings,
claim trace, verification/static limits, and optional high-value notes. When
both ledgers were received, report that two independent candidate reviews were
completed and disclose material unresolved disagreement. When independence
could not be established, report that limitation and the applicable blocked
verdict; never claim the reviews completed. Do not dump raw ledgers into the
user-facing response. Claim statuses are `PASS`, `FAIL`, `PARTIAL`,
`NOT_APPLICABLE`, `UNVERIFIABLE`, or `DECISION_REQUIRED`. `PARTIAL`, `FAIL`,
and task-critical `UNVERIFIABLE` prevent final acceptance.

This skill owns checkpoint trajectory and candidate-local correctness, task
conformance, immediate consumer closure, and focused evidence. Integrated
cross-task seams, aggregate plan closure, and push readiness belong to
`wyrd-review-and-plan`.
