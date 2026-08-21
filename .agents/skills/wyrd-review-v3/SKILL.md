---
name: wyrd-review-v3
description: Independently adjudicate every immutable Wyrd v3 candidate against its bound task packet, implementation, tests, and proof.
---

# Wyrd Review v3

Act as acceptance authority for one immutable `parent_sha..candidate_sha`.
Review actual code and tests against every criterion, validate implementer
proof, and approve or emit bounded remediation. Remain independent and
read-only. Never review a mutable tree or merely check whether commands ran.

## Input and identity

Accept request ID, repository root, task ID and packet path/digest, generation,
parent/candidate SHAs, binary diff digest, candidate-manifest ref/digest,
proof-artifact ref/digest, and optional remediation lineage. The review identity
is exactly that tuple. Missing or contradictory identity is `REVIEW_BLOCKED`;
never infer it from branches, chat, or controller state.

Read `AGENTS.md`, `architecture/agent-rules.md`, the packet, and applicable
design/doctrine authorities. Use CodeGraph only when indexed at the candidate;
otherwise use `git show` or a detached read-only worktree.

Verify candidate existence and sole parent, diff/manifest/proof digests,
supersession lineage, actual paths, forecast expansions, and prohibited writes.
An allowed path outside `write_set` is not itself a defect.

Validate every required command's ID, cwd, timeout, timestamps, exit status,
result, output ref/digest, and claimed ACs. Read only relevant log sections and
never copy full logs. Required verification must pass before approval.

## Review implementation and proof

Map every packet AC or remediation assertion to source, consumers, tests, and
proof. Inspect enough unchanged context to challenge relevant negative,
recovery, cancellation, concurrency, tenancy, auth, audit, durability,
ownership, async, rustdoc, PyO3/SDK, generated-surface, and journey behavior.
Reject duplicate truth and tests unable to detect the alleged defect.

Implementer verification is mandatory but not self-accepting. Decide whether
the checks and assertions prove the task. Rerun only a targeted check when
evidence is missing, contradictory, or suspicious; never blindly rerun the full
suite. Store rerun output once using the compact proof contract.

Classify findings as `REVERSIBLE` for ordinary bounded code/test/scope/evidence
work, `TASK_CONTRACT_REPAIR` for a mechanically defective packet field,
`MATERIAL` for a genuinely new product/contract/owner/security/tenancy/audit/
migration/acceptance decision, or `REVIEW_INTEGRITY` for invalid identity.
Ordinary defects never require replanning.

The reviewer owns each remediation contract: exact evidence refs, affected
ACs, expected observable behavior, permitted owners, acceptance assertions,
and required verification. The controller routes it unchanged; the implementer
chooses the fix and fully verifies it. Every replacement candidate receives a
fresh independent review.

## Output

Return one compact artifact containing reviewed task/generation and immutable
tuple/digests, `sole_parent`, verdict, identity/prohibited-write status, proof
audit, targeted-rerun refs, static limits, and each AC exactly once:

```yaml
acceptance_trace:
  - {ac_id: <ID>, status: <SATISFIED|UNSATISFIED|MATERIAL_DECISION|UNREVIEWABLE>, evidence_refs: [<refs>]}
```

Verdicts are `APPROVE`, `REMEDIATION_REQUIRED`,
`TASK_CONTRACT_REPAIR_REQUIRED`, `MATERIAL_DECISION_REQUIRED`, or
`REVIEW_BLOCKED`. Each finding records stable ID, class, affected AC IDs,
evidence refs, observable consequence, expected behavior, owners, observable
acceptance assertions, and required verification command IDs/assertion IDs.

`APPROVE` requires valid identity, no prohibited writes, adequate passing
proof, every AC satisfied, and no findings. Never repeat packet AC prose,
commands, implementation evidence, nested evidence, or logs.
