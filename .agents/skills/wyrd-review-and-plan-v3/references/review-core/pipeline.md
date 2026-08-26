# Wyrd v3 Terminal Static Review Pipeline

Run a delegated static review of one immutable integrated Wyrd target against
its immutable execution baseline, approved intent, plan, task packets, and v3
candidate evidence. Validate and deduplicate specialist candidates, write one
consolidated `review.md`, and return a v3 remediation or material-decision
handoff. Stop without implementation.

Read `orchestration-contract.md`, `artifact-contract.md`, `packet.md`,
`specialist-contract.md`, `adversarial-contract.md`, `evidence-contract.md`,
`dispatch-contract.md`, and `validation/maintainer-gate.md` completely before
acting.

## Phase 1: Resolve the immutable execution result

Resolve and record the execution baseline, integrated target, and merge base.
Gather commits, changed files, renames, and the complete committed diff. Exclude
all staged, unstaged, untracked, and other working-tree content.

Resolve the caller-supplied intent, approved v3 plan and plan review, every task
packet, every candidate/integration identity, task proof and candidate-review
artifact, and integrated-verification proof. Verify paths, digests, SHAs,
lineage, approvals, and target contribution. A missing or contradictory binding
is `REVIEW_BLOCKED`.

Read applicable target-snapshot repository instructions, design authorities,
hard gates, manifests, generated-contract owners, and review policy. Build the
intent and acceptance map, changed-symbol and downstream-impact map, risk and
change profiles, seven baseline assignments, deterministic triggered
assignments, and static-analysis limits.

Create the review directory and required evidence tree from
`evidence-contract.md`. Write the stable packet and input digests. Build
`coverage.json` before dispatch so every changed production file, requirement,
task criterion, and high-risk boundary has an explicit assignment. Confirm the
active harness can provide every required capability with distinct reviewer
identities; persist and return `REVIEW_BLOCKED` if the required independent
roster cannot run.

## Phase 2: Dispatch independent specialists

Build the immutable assignment packet defined in `dispatch-contract.md` and
dispatch all seven baseline domains to separate review contexts using the exact
prompts and capabilities in `orchestration-contract.md`.
Dispatch every triggered persistence, async, PyO3, Vala, UI, or other required
specialist in addition to the baseline roster. Respect the active harness's
capacity and stopping behavior. The root validates result identity and publishes
accepted reports atomically. Capacity limits never justify combining or omitting
domains.

Apply the coverage floors in `evidence-contract.md`. Auth, tenancy, destructive
persistence, migration, concurrency, recovery, and public wire-contract
boundaries require two assignments with distinct reviewer IDs. User-facing
capabilities require the tests/user-journey assignment.

Give every reviewer the immutable packet, exact target SHA, scoped impact slice,
prompt path and digest, requirements and task criteria, authorities, hard gates,
and assigned report path. Require the report schema from
`specialist-contract.md` and the falsification attempts from
`adversarial-contract.md`.

Specialists inspect code independently and return only their structured result.
The root writes accepted evidence reports. Specialists do not run project
commands, assign final IDs, plan, remediate, launch agents, inspect
working-tree-only content, or modify source.

Use the active harness's native delegation mechanism. When capacity is lower
than the roster size, dispatch in capacity-bounded waves. Harness choice never
changes the artifact contract, coverage floor, or reviewer-independence rule.

## Phase 3: Validate, deduplicate, and ground

Validate roster completeness, capabilities, trigger coverage, report
identity, prompt digests, target SHA, and independent reviewer IDs. Create the
preliminary `ledger.json` with every namespaced candidate visible.

Read `validation/validate-findings.md` and apply it to every candidate. Re-read
the cited target source and full owning symbol, then inspect affected callers,
consumers, contracts, tests, authorities, plan requirements, task criteria, and
local precedents. Attempt to disprove the issue and its proposed closure.
Reject speculative, preference-only, stale, working-tree-only, duplicate, or
unsupported claims with a concrete durable disposition.

Merge only candidates with the same root cause and required correction. Apply
`validation/maintainer-gate.md`. Classify every confirmed required finding as:

- `REVERSIBLE` when approved intent fully determines bounded remediation;
- `TASK_CONTRACT_REPAIR` when a mechanical packet or proof field is defective;
- `MATERIAL` when safe correction requires a new product, public/durable
  contract, owner, dependency, security, tenancy, audit, migration, or
  acceptance decision.

Assign `REV-NNN` only after validation and root-cause deduplication. Ground the
current flow, failure scenario, natural owner, bounded correction, constraints,
acceptance assertions, and exact future verification in the final finding.

Before accepting a clean conclusion, challenge the three most dangerous changed
invariants across the integrated impact cone. Record whether each survived,
became a candidate, or remains a static limit.

Dispatch the final independent adjudicator required by `evidence-contract.md`
when its floor triggers. The adjudicator must have produced no candidate report
and receives no desired verdict. Reconcile its challenges in `validation.md`
and `ledger.json`.

## Phase 4: Write and validate the terminal review

Complete `coverage.json`, `ledger.json`, and candidate-by-candidate
`validation.md`. Write `{REVIEW_DIR}/review.md` using
`artifact-contract.md`. State
`Static analysis: no runtime verification performed`.

Choose exactly one verdict:

- `CLEAN`;
- `REMEDIATION_REQUIRED`;
- `MATERIAL_DECISION_REQUIRED`;
- `REVIEW_BLOCKED`.

Finish by running the ported artifact validators:

```bash
python "${REVIEW_ROOT}/scripts/validate_review.py" "${REVIEW_DIR}/review.md"
python "${REVIEW_ROOT}/scripts/validate_evidence.py" "${REVIEW_DIR}"
```

Fix every artifact-contract error before returning a verdict.

## Phase 5: Return the v3 handoff

For `CLEAN`, set `Execution handoff: Ready to push`. No remediation artifact is
created.

For `REMEDIATION_REQUIRED`, set
`Execution handoff: Controller remediation required` and include complete
bounded remediation contracts for every `REVERSIBLE` and
`TASK_CONTRACT_REPAIR` finding. Do not create a second implementation plan.
The controller routes ordinary corrections through `$wyrd-implement-v3`, full
candidate proof, `$wyrd-review-v3`, serial integration, integrated checks, and
a fresh terminal review.

For `MATERIAL_DECISION_REQUIRED`, invoke `$wyrd-plan-v3` with the consolidated
`MATERIAL` findings and exact committed identities. Set `Execution handoff:` to
the resulting canonical v3 plan path or the concrete material-authority
blocker, then stop before implementation.

For `REVIEW_BLOCKED`, set `Execution handoff: Not ready — <reason>` and make no
push-readiness claim.

Return only the review ID; resolved SHAs; verdict and risk; `review.md` and
evidence paths; finding counts by class; execution handoff; and
`Static analysis: no runtime verification performed`.
