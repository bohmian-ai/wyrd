---
name: wyrd-review-and-plan-v3
description: Run a risk-routed terminal v3 static review of one committed Wyrd target against one committed base, validate specialist findings, and separate autonomous reversible remediation from material replanning. Use for final, holistic, merge-readiness, or post-plan implementation review; never for working-tree or per-task review.
---

# Wyrd Review And Plan v3

Use `.agents/model-routing.md`. Review one immutable committed target against one
base. Never modify source or run project commands. State: `Static analysis: no
runtime verification performed.` Never invoke v1/v2.

## Required input

```yaml
protocol: wyrd-review-and-plan-v3
request_id: <stable id>
repository_root: <absolute path>
review_output_path: <absolute artifact path outside reviewed worktree>
base_sha: <immutable base>
target_sha: <immutable integrated target>
intent: {path: <digest-bound intent>, sha256: <digest>}
integrated_tasks:
  - {id: <task id>, packet_sha256: <digest>, integrated_sha: <commit>}
proof_artifacts:
  - {task_id: <task id>, path: <artifact>, sha256: <digest>}
```

Resolve the SHAs and digests before dispatch. Reject a dirty or moving target.
Read `AGENTS.md`, agent rules, design/doctrine, applicable authorities,
manifests, generated-contract owners, and intent. Use CodeGraph only when its
indexed revision equals `target_sha`; otherwise use `git show` or a detached
read-only worktree. Derive the impact graph from the committed diff and audit
recorded proof semantically without rerunning it.

## Risk-route specialists

Select one through five independent review-tier specialists whose domains have
a plausible failure surface in the impact graph. Use all five only for broad
multi-domain, security/tenancy, migration, or high-blast-radius durable work.
Do not dispatch an irrelevant specialist merely to fill a quota.

Available domains:

1. `correctness-lifecycle` — behavior, concurrency, cancellation, recovery,
   cleanup, and performance-critical lifecycle.
2. `security-tenancy-audit-errors` — auth, RLS, secrets, audit boundaries,
   stable public errors, unsafe input, and abuse.
3. `architecture-contracts` — doctrine, ownership, struct-centered Rust,
   public projections, dependencies, and unnecessary complexity.
4. `tests-verification` — requirement traceability, journey/negative coverage,
   assertion strength, and proof binding.
5. `build-generated-deployment-release` — manifests, features, generated drift,
   packaging, migrations, topology, CI, and release compatibility.

Give each selected specialist the same immutable tuple, impact graph,
authorities, and intent plus one primary domain. Require candidate findings,
clean coverage, strongest counterexample attempted, and the exact tuple. They
do not assign final IDs, plan, edit, or communicate with the user.

## Validate and consolidate

Independently validate every candidate against source at `target_sha`. Reject
preference-only, speculative, duplicate, stale, or unproved claims. For each
confirmed finding record severity, stable ID, violated invariant, source and
consumer evidence, failure scenario, bounded required outcome, regression
assertion, and verification gate.

Classify each finding:

- `REVERSIBLE` when existing intent and material decisions determine a bounded
  code, test, generated-output, or evidence correction;
- `TASK_CONTRACT_REPAIR` when a non-material packet field must change; or
- `MATERIAL` when correction requires a new product, public/durable contract,
  owner, dependency, security, tenancy, migration, or acceptance decision.

## Output contract

Write one authoritative artifact in the caller-selected review directory and
return exactly this YAML shape:

```yaml
protocol: wyrd-review-and-plan-v3
request_id: <input id>
reviewed: {base_sha: <base>, target_sha: <target>, intent_sha256: <digest>}
reviewed_integrated_tasks: [<exact input integrated-task identity objects>]
reviewed_proof_artifacts: [<exact input proof-artifact identity objects>]
specialists: [<selected domains>]
verdict: <CLEAN|REMEDIATION_REQUIRED|MATERIAL_DECISION_REQUIRED|REVIEW_BLOCKED>
findings:
  - id: REV-001
    class: <REVERSIBLE|TASK_CONTRACT_REPAIR|MATERIAL>
    severity: <critical|major|minor>
    affected_task_ids: [<existing task ids>]
    owners: [<paths/modules>]
    evidence: [<exact source facts>]
    consequence: <failure scenario>
    required_outcome: <bounded correction>
    acceptance_assertions:
      - {id: <REV-001-A1>, text: <observable assertion>}
    verification:
      - {command: <narrow exact or forecast-derived check>, assertion_ids: [<REV-001-A1>]}
rejected_candidates: [<candidate finding dispositions>]
static_limits: [<limits>]
```

`CLEAN` requires no confirmed required findings. Return reversible findings to
the controller for autonomous remediation atop current integration. Return
task-contract repairs through the explicit bounded `task_contract` repair
bundle in `$wyrd-plan-v3`; already integrated packet identities remain
historical and any correction becomes an additive remediation generation from
the current target. Invoke
`$wyrd-plan-v3` only for `MATERIAL` findings, then stop before implementation.
After reversible remediation, require new proof and repeat terminal review of
the new immutable target.
