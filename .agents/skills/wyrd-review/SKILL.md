---
name: wyrd-review
description: Review Wyrd implementations against approved wyrd-plan plans and task packets, living-task execution updates, architecture, ownership, contracts, repository rules, tests, and verification evidence. Use standalone after or during wyrd-implement, as the static task and final-plan reviewer controlled by $wyrd-implement-plan, or as the repository integration policy loaded by terminal review-and-plan branch reviews. Binding modes return static findings and evidence to their root orchestrator without writing another artifact, running verification, or routing work.
---

# Wyrd Review

Independently review implementation conformance and proof. Preserve the
boundary used by `$wyrd-implement`: reversible mechanics remain implementation
work; material decisions return to the applicable plan authority.

Use one fixed loop:

```text
resolve diff -> map requirements/ACs -> inspect source/tests
             -> classify adaptations/evidence -> verdict -> route
```

Do not modify implementation source, tests, generated artifacts, or the
reviewed plan/task. Writing the durable review is the only default mutation.

## Plan-execution binding mode

When `$wyrd-implement-plan` dispatches this skill for one task or the final
integrated plan, this mode overrides conflicting standalone instructions:

- Review the orchestrator-provided task delta from the last accepted commit,
  or the complete execution-baseline-through-branch delta for final review.
- Read the canonical plan, active tasks, implementation reports, recorded
  verification evidence, and applicable repository authorities completely.
- Perform static source, contract, caller, consumer, manifest, generated
  surface, and test analysis. Audit recorded verification semantically.
- Do not run tests, builds, lints, formatters, generators, migrations,
  services, plan validators, repository gates, or independent verification.
- Load the progressive references required by the affected surface.
- Return a requirement/acceptance traceability matrix, findings, evidence
  audit, inspected surfaces, and material static-analysis limits.
- Use only these verdicts:
  - `APPROVE`: implementation and proof satisfy the reviewed task or plan.
  - `RESUME_IMPLEMENTATION`: reversible implementation or evidence work
    remains.
  - `ORCHESTRATOR_DECISION_REQUIRED`: a material plan, product, security,
    contract, migration, dependency, ownership, or acceptance decision must be
    resolved by the plan orchestrator.
  - `REVIEW_BLOCKED`: the review target or mandatory evidence cannot be
    resolved well enough for static review.
- Do not write `review.md`, modify source or plan artifacts, assign task
  status, invoke another skill, create a remediation plan, or communicate with
  the user.

For first review, operate as a fresh independent agent. For focused re-review,
verify prior findings first and inspect affected seams without reopening
accepted decisions absent new evidence. The `$wyrd-implement-plan`
orchestrator is the only controller, writer, committer, and router.

## Terminal integration-binding mode

When `$review-and-plan` or `$review-and-plan-quick` loads this skill as the Wyrd
repository integration policy, this mode overrides conflicting standalone
workflow instructions:

- Review only the orchestrator-provided committed target branch against the
  committed base branch at their resolved SHAs.
- Use only the caller-supplied optional reference as intent. Apply formal plan
  conformance only when that reference is an approved Wyrd plan or task;
  otherwise mark it not applicable while still applying Wyrd architecture and
  repository rules.
- Perform static source, contract, caller, consumer, manifest, and test
  analysis. Do not run project tests, builds, lints, formatters, generators,
  migrations, services, repository gates, or independent verification.
- Load the progressive references required by the affected surface.
- Return candidate findings, clean coverage, conformance evidence, and material
  static-analysis limits to the root orchestrator.
- Do not assign the final verdict or finding IDs, write `review.md`, modify the
  reference, invoke `$wyrd-plan`, or route into implementation.

The terminal orchestrator is the only writer and owns deduplication, final
classification, the durable artifact, and canonical planning.

## Establish the review contract

1. Read `AGENTS.md`, `architecture/agent-rules.md`,
   `architecture/wyrd-design.md`, and `architecture/wyrd-doctrine.mdx`
   completely.
2. Resolve the exact target from the caller-provided diff, commit range,
   branch base, or working tree. Include untracked files and preserve unrelated
   user changes.
3. Locate the canonical plan and every task in scope when plan conformance is
   claimed. Read them completely and run:

   ```bash
   python .codex/skills/wyrd-plan/scripts/validate_plan_artifacts.py \
     .dev/plan/<slug>
   ```

   Require an `Approved` parent plan. A `Ready` task is eligible for an active
   or legacy review; `Complete` is the normal completion-review state; review
   a `Blocked` task only to validate its blocker claim. A `Planned` task is not
   eligible for implementation-conformance approval.
4. Read `$wyrd-implement` completion evidence and living-task execution updates
   when present. Treat them as claims to verify, not proof by themselves.
5. Read the owning manifests, relevant `mise.toml` tasks, tests, generated
   sources, and nearest implementation patterns needed to judge the change.
6. Resolve the applicable execution skill from the task and write set. Use
   `$wyrd-implement` for non-UI work and `$wyrd-ui` for Svelte/UI work; load
   both when one task crosses the server/UI boundary. Treat a task that names
   an execution skill which rejects its write set as an executable-task defect.
7. When `.codegraph/` exists, use CodeGraph before grep or manual traversal to
   locate owners, callers, consumers, dispatch paths, and tests.

If no plan relationship is claimed, mark plan conformance
`Not applicable: no plan relationship was claimed` and continue the repository
review.

Apply authority in this order:

1. current user instructions;
2. active task;
3. approved parent plan;
4. applicable `AGENTS.md` files;
5. current repository architecture and conventions.

## Load references progressively

Always read:

- `references/conformance-and-adaptation.md`
- `architecture/references/languages/implementation-execution.md`

Read `references/verification-evidence.md` when commands, test results,
environment setup, coverage, or completion evidence are in scope. Read
`references/review-format.md` before writing the review.

Read `architecture/references/README.md`, then load only the knowledge required
by the affected surface:

| Reference | Load when the review touches |
|---|---|
| `architecture/references/doctrine/positioning-and-vocabulary.md` | Card vocabulary, `CardRef`, v1 kinds, or removed concepts |
| `architecture/references/doctrine/architecture-constraints.md` | Tier boundaries, deployment, or observation identity |
| `architecture/references/architecture/patterns.md` | Crate placement and server/client/storage/provider/audit ownership |
| `architecture/references/languages/rust-core.md` | Rust ownership, traits, async, allocation, or API shape |
| `architecture/references/languages/pyo3-boundaries.md` | PyO3 classes, GIL, lifetimes, conversions, or registration |
| `architecture/references/languages/errors.md` | Stable errors and boundary mappings |
| `architecture/references/languages/python-api-and-stubs.md` | Python exports, stubs, package layout, or tests |
| `architecture/references/languages/testing-workflows.md` | Test tiers, journey coverage, verification levels, or boundary gates |
| `architecture/references/languages/agent-harness.md` | MCP and other agent-facing contracts |
| `architecture/references/languages/typescript-guide.md` | `@wyrd/sdk` and napi conventions |
| `architecture/references/domain/iceberg-bifrost.md` | Bifrost, Iceberg, DataFusion, or object storage |

State which conditional references were loaded and the decision each governs.
Do not duplicate their repository rules in this skill.

## Review conformance and adaptations

Build a traceability matrix for every in-scope requirement and acceptance
criterion:

- implementation path and owning symbol;
- test or generated artifact;
- verification evidence;
- `PASS`, `FAIL`, or `UNVERIFIED`.

Inspect required behavior, invariants, ordering, errors, side effects,
negative cases, non-goals, task boundaries, consumers, and changed files.
Classify every difference between the task recipe and repository reality:

| Class | Review treatment |
|---|---|
| Local mechanic | Accept when behavior, ownership, and proof remain intact |
| Bounded correction | Validate its evidence; do not call it an unapproved deviation |
| Material deviation | Standalone: require replanning. Plan-execution binding: return `ORCHESTRATOR_DECISION_REQUIRED` |

Expected private paths, helpers, and fixture layouts are guidance rather than a
strict whitelist. Public/wire/generated/persisted contracts, migrations,
destructive behavior, dependencies or Cargo features, auth/tenancy/policy/audit
semantics, ownership direction, and acceptance outcomes are material.

Review proof equivalence rather than command-string identity. A corrected
command is acceptable only when it proves the same acceptance criterion,
target, relevant features, test tier, environment behavior, negative cases,
assertions, and consumers, and the living task records why the original recipe
was defective.

## Assign the verdict

- `APPROVE`: every in-scope requirement and acceptance criterion passes, the
  required proof is sufficient, and no blocking finding remains.
- `RESUME_IMPLEMENTATION`: bounded source, test, lint, fixture, setup, command,
  or evidence work remains within the approved material contract.
- `REPLAN_REQUIRED`: correctness requires changing a material decision or the
  approved approach is materially invalid.
- `REVIEW_BLOCKED`: the target, plan authority, task relationship, or mandatory
  evidence cannot be resolved well enough to review.

Use these exact uppercase verdict tokens in the artifact and handoff. Do not
rename them to `PASS`, `REJECT`, `CHANGES_REQUIRED`, or `BLOCKED`.

Do not approve intent, compilation alone, narrative claims, weakened proof, or
a subset of acceptance criteria. Do not use `REPLAN_REQUIRED` for work that
`$wyrd-implement` is already authorized to diagnose and fix.

## Write and route the result

Read `references/review-format.md` and write:

```text
.dev/review/<short-head>-<UTC-YYYYMMDD-HHMMSS>-wyrd-review/review.md
```

Use a caller-provided review directory when present. Write the artifact before
invoking another skill.

- For `APPROVE`, record that no work remains.
- For `RESUME_IMPLEMENTATION`, return the existing active task to
  its surface-appropriate execution skill with the finding IDs and evidence.
  Use `$wyrd-implement` for non-UI work and `$wyrd-ui` for UI work. Do not
  create a remediation plan.
- For `REVIEW_BLOCKED`, record the exact missing authority or mandatory proof
  and stop.
- For `REPLAN_REQUIRED`, announce the handoff, load `$wyrd-plan`, and revise
  the canonical plan/task when the original outcome remains active. Create a
  separate remediation plan only when the review identifies an independent
  follow-up after the original plan is closed.

Invocation authorizes review and applicable planning artifacts only. It does
not authorize source, test, generated-artifact, migration, dependency,
configuration, or task-status changes.

Return the review path, verdict, and next executable artifact: none for
`APPROVE`, the existing task and applicable execution skill for
`RESUME_IMPLEMENTATION`, the missing authority for `REVIEW_BLOCKED`, or the
revised plan/task paths for `REPLAN_REQUIRED`.
