# Full Review Evidence Contract

Full mode preserves auditable supporting evidence while keeping `review.md` as
the only authoritative verdict and remediation source.

## Required tree

```text
{REVIEW_DIR}/
├── review.md
└── evidence/
    ├── packet.md
    ├── coverage.json
    ├── ledger.json
    ├── validation.md
    └── specialists/
        ├── <assignment>.md
        └── <assignment>.attestation.json
```

No evidence artifact may be required to understand a final finding or execute
its v3 remediation contract. Evidence proves how the conclusion was reached.

## Immutable packet

Record immutable review inputs and digests:

- review ID and resolved base, target, and merge-base SHAs;
- changed files, renames, commits, and diff digest;
- approved intent, plan, plan-review, task-packet, lineage, and proof paths and
  digests;
- target-snapshot authority paths and digests;
- risk and change profiles;
- baseline and triggered assignments, reviewer questions, capabilities, and digests;
- static-analysis boundary.

Do not require a second stored copy of a large diff when its commit range and
digest make it reproducible.

## `coverage.json`

Use this shape:

```json
{
  "schema_version": 3,
  "review_id": "",
  "mode": "full",
  "risk": "low|standard|high",
  "roster": [
    {
      "assignment_id": "baseline-security",
      "domain": "security",
      "requirement": "baseline|triggered",
      "trigger_ids": [],
      "required_capability": "security-specialist",
      "reviewer_id": "security-review-task",
      "prompt_path": "review-security/review-security.md",
      "prompt_digest": "sha256:...",
      "target_sha": "",
      "report_path": "evidence/specialists/baseline-security.md",
      "attestation_path": "evidence/specialists/baseline-security.attestation.json",
      "status": "completed|blocked"
    }
  ],
  "triggers": [
    {
      "trigger_id": "trigger-persistence-1",
      "domain": "persistence-storage",
      "matched_locations": [],
      "status": "assigned|gap"
    }
  ],
  "independent_validation": {
    "status": "completed|not_required",
    "reviewer": "",
    "reason": ""
  },
  "adversarial_review": {
    "mode": "full",
    "status": "completed|gap",
    "summary": ""
  },
  "changed_files": [
    {
      "path": "",
      "classification": "production|test|docs|config|generated",
      "primary_assignment": "",
      "secondary_assignments": [],
      "status": "inspected|sampled|not_applicable",
      "notes": ""
    }
  ],
  "requirements": [
    {
      "id": "",
      "source_locations": [],
      "proof_locations": [],
      "assignments": [],
      "status": "covered|gap|deferred|not_applicable"
    }
  ],
  "high_risk_boundaries": [
    {
      "id": "",
      "category": "auth|tenancy|destructive-persistence|migration|concurrency|recovery|public-contract|other",
      "locations": [],
      "assignments": [],
      "status": "covered|gap"
    }
  ],
  "user_facing_capabilities": [
    {
      "id": "",
      "surfaces": [],
      "assignments": [],
      "status": "covered|gap"
    }
  ],
  "policy_gates": [
    {
      "id": "",
      "authority": "",
      "locations": [],
      "required_assignments": [],
      "status": "covered|gap|not_applicable"
    }
  ]
}
```

Coverage floors:

- every baseline domain in `orchestration-contract.md` has one completed
  assignment;
- every full-mode baseline assignment has the required capability and a
  distinct reviewer identity;
- every matched trigger has one completed additional assignment;
- every changed production file has one primary assignment;
- every reference requirement and acceptance criterion maps to source and
  proof, or carries an explicit non-covered disposition;
- auth, tenancy, destructive persistence, migration, concurrency, recovery,
  and public wire-contract boundaries have two assignments with distinct
  reviewer identities;
- user-facing capabilities include the tests/user-journey lens;
- materially changed Rust structure includes the repository's structure and
  ownership lens when that policy applies.
- every full-mode specialist report records its inspected scope, at least one
  material adversarial probe, and, when clean, why its strongest realistic
  counterexample survived;
- every completed specialist has a compact dispatch attestation binding its
  assignment, reviewer, target, prompt, and published report digest;
- a clean full review challenges the three most dangerous changed invariants;

Record representative downstream inspection as `sampled`; never imply an
exhaustive call-graph review when only selected consumers were inspected.

## `ledger.json`

Use this shape:

```json
{
  "review_id": "",
  "candidates": [
    {
      "candidate_id": "correctness/C001",
      "assignment_ids": ["baseline-correctness"],
      "reviewer_ids": ["correctness-review-task"],
      "source_reviewers": ["correctness"],
      "original_severity": "critical|high|medium|low",
      "hard_gate": false,
      "status": "CONFIRMED|MERGED|FOLLOW_UP|KNOWN_DEFERRED|STATIC_LIMIT|REJECTED|MATERIAL_DECISION_REQUIRED",
      "final_finding": "REV-001|null",
      "merged_into": "candidate-id|null",
      "reason": "",
      "validated_locations": [],
      "requirements": [],
      "dissent": [
        {
          "reviewer": "",
          "position": "",
          "evidence": []
        }
      ]
    }
  ]
}
```

Every specialist candidate appears exactly once. `CONFIRMED` maps to a final
finding. `MERGED` maps through `merged_into` to a confirmed candidate.
Non-required states do not receive `REV-NNN` IDs.
Each final finding's `Candidates:` field lists both its confirmed root
candidate and every candidate merged into that root.

`coverage.json.independent_validation` records whether the independent floor
was triggered, who performed it, and why. A completed independent review uses
a stable reviewer identity or task name; `not_required` records the concrete
reason the floor did not trigger.

## `validation.md`

Create one section per candidate, keyed by the immutable candidate ID. Record:

- source reviewers, original severity, locations, and claim;
- final status and final-finding or merge mapping;
- source, callers, consumers, contracts, tests, and precedents reread;
- authority, intent, and requirements checked;
- reachability and materiality analysis;
- decision and concrete rationale;
- material dissent and its resolution.

Rejected candidates remain visible. Never reduce a rejection to an unexplained
count.

## Independent validation floor

Dispatch an independent validation reviewer when any condition holds:

- risk is high;
- a critical or high candidate is proposed for rejection;
- specialists materially disagree;
- deduplication may merge distinct root causes;
- an applicable hard-gate candidate is proposed for rejection;
- validation changes the proposed verdict.
- a high-risk boundary is declared clean.

Give that reviewer the immutable packet, specialist reports, coverage, the
preliminary ledger, and applicable authority. Do not provide a desired verdict.
Require it to challenge both selected candidate decisions and high-risk
boundaries declared clean. Record its challenges and reconciliation in
`validation.md` and `ledger.json`.
