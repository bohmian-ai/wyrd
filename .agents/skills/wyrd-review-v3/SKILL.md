---
name: wyrd-review-v3
description: Independently review an immutable Wyrd commit range against its task packet, implementation, tests, and verification evidence.
---

# Wyrd Review v3

Review the supplied `base_sha..candidate_sha` against the supplied Wyrd task or
remediation packet. The goal is to decide whether the candidate actually
completes the task, not to enforce workflow bookkeeping.

Remain read-only with respect to the candidate implementation. Review committed
code, not uncommitted edits. A clean checkout at the candidate is acceptable;
otherwise use `git show`, `git diff`, or a detached read-only worktree.

## Required input

A direct review needs only:

- repository root;
- task or remediation packet, by path or supplied text; and
- immutable base and candidate SHAs.

Confirm that both commits exist, the base is an ancestor of the candidate, and
the candidate is not a merge unless the caller explicitly asks to review a
merge. Treat `base_sha` as the review-range base; it need not be the candidate's
immediate Git parent.

Controller-supplied request IDs, generations, manifests, proof artifacts, and
digests are optional provenance. Validate and report them when supplied. Their
absence does not block a direct user-requested review and must never replace
reviewing the code.

Use `REVIEW_BLOCKED` only when the commits or task cannot be read, the range is
mutable or ambiguous, or the environment prevents a necessary review step. Do
not block on missing orchestration metadata.

## Review workflow

Read `AGENTS.md`, `architecture/agent-rules.md`, the task packet, and the
relevant portions of `architecture/wyrd-design.md` and
`architecture/wyrd-doctrine.mdx`. Follow CodeGraph instructions from
`AGENTS.md`; use `git show` or a detached worktree when CodeGraph is not indexed
at the candidate.

Review the diff before mapping acceptance criteria. Inspect:

- every changed production symbol and test;
- immediate callers, consumers, error paths, and cleanup paths;
- state transitions, rollback, cancellation, concurrency, durability,
  tenancy, audit, bounds, and telemetry where relevant;
- whether tests can fail for the defect they claim to cover; and
- whether the implementation stays inside the task's owners and non-goals.

Be proportionate. Do not invent style findings, demand unrelated cleanup, or
expand a bounded remediation into terminal integrated review. Report only
defects that can affect task behavior, correctness, verification, or a hard
repository rule.

Map each task acceptance criterion or remediation finding to source, tests, and
verification. A concise mapping is enough; do not repeat the packet.

## Verification

Audit the implementer's claimed commands and results when evidence is
available. Rerun the narrowest relevant checks when evidence is absent,
contradictory, suspicious, or needed to complete the review. Do not blindly
rerun a broad suite that valid focused proof already covers.

Required task verification must pass before approval. A failure proven to be
unchanged from the review base may be recorded as a pre-existing static limit
when the candidate's narrower owning-surface checks pass and the task does not
require fixing that unrelated owner. Do not accept a candidate-caused failure,
a weakened check, a filtered required test, or an unverified task-critical
behavior.

When a task names a conditional lane or future owner that does not exist, check
the repository and task sequencing. Treat it as not applicable only when the
packet makes it conditional or the work is explicitly owned by a later,
out-of-scope slice. Otherwise report a task-contract defect or missing proof.

An independent companion review is optional. Use one when the caller or an
active controller requires it, or when the primary reviewer decides a focused
specialist pass would materially reduce risk. Lack of a companion never blocks
an otherwise complete direct review.

## Findings and verdicts

Classify findings as:

- `REVERSIBLE`: bounded code, test, documentation, or verification work;
- `TASK_CONTRACT_REPAIR`: a mechanically impossible or contradictory packet
  requirement;
- `MATERIAL`: a genuinely new product, contract, owner, security, tenancy,
  audit, or migration decision; or
- `REVIEW_INTEGRITY`: the supplied commits or task do not identify a stable
  review target.

Verdicts:

- `APPROVE`: all in-scope criteria are satisfied, relevant verification is
  adequate, the code review found no defect, and one adversarial probe survived;
- `REMEDIATION_REQUIRED`: bounded implementation or proof work remains;
- `TASK_CONTRACT_REPAIR_REQUIRED`: the packet itself needs mechanical repair;
- `MATERIAL_DECISION_REQUIRED`: completion needs a new material decision; or
- `REVIEW_BLOCKED`: the review target cannot be reliably inspected.

For each finding give a stable ID, class, severity, confidence, affected
criteria, exact source location or authority, reachable scenario, observable
consequence, expected behavior, permitted owner, acceptance assertion, and
required focused verification. Do not produce findings for harmless preference
differences.

## Output

Lead with the verdict and findings. For approval, say explicitly that no
findings remain. Include a compact review record:

```yaml
review:
  task: <task id or path>
  range: <base_sha>..<candidate_sha>
  verdict: <verdict>
  changed_paths: [<paths>]
  verification: [<commands and outcomes>]
  static_limits: [<pre-existing or untestable limits>]
acceptance_trace:
  - {ac_id: <ID>, status: <SATISFIED|UNSATISFIED|MATERIAL_DECISION|UNREVIEWABLE>, evidence_refs: [<source/test/proof refs>]}
code_review:
  inspected_impact: [<symbols, callers, consumers, tests>]
  covered_concerns: [<focused concerns>]
  adversarial_probe:
    invariant: <most dangerous changed invariant>
    counterexample: <strongest reachable counterexample>
    evidence_refs: [<source/test/proof refs>]
    outcome: <SURVIVED|FINDING|STATIC_LIMIT>
```

Keep the artifact concise. Do not paste logs or restate the task. A direct
human-readable review is preferred over empty procedural fields.

## Boundary

This skill owns candidate-local correctness, task conformance, immediate
consumer closure, and focused verification. `wyrd-review-and-plan-v3` owns
cross-candidate seams, aggregate plan closure, integrated drift, and final push
readiness.
