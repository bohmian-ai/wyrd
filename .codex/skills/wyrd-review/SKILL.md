---
name: wyrd-review
description: Review Wyrd implementations for conformance to approved wyrd-plan plans and task packets, wyrd-implement completion evidence, architecture, ownership, contracts, repository rules, and verification. Write a durable review artifact and route confirmed actionable findings through wyrd-plan into canonical remediation plans and task packets. Use after wyrd-implement, for one task or an integrated implementation, or whenever Codex must determine whether Wyrd code follows its approved plan. Do not modify implementation source.
---

# Wyrd Review

Review the implementation against its approved intent and current repository
authority. Do not modify implementation source, tests, generated artifacts, or
the reviewed plan to legitimize implementation drift. Write review artifacts,
then delegate remediation design to `$wyrd-plan`.

## Establish the review contract

Before judging the implementation:

1. Read `AGENTS.md`, `architecture/agent-rules.md`,
   `architecture/wyrd-design.md`, and `architecture/wyrd-doctrine.mdx` to EOF.
2. Identify the exact review target from the caller-provided diff, commit
   range, branch base, or current working tree. Include untracked files.
3. When the implementation claims to come from `$wyrd-plan`, locate the
   canonical plan and every task in review scope. Read them to EOF and run:

   ```bash
   python .codex/skills/wyrd-plan/scripts/validate_plan_artifacts.py \
     .dev/plan/<slug>
   ```

   Require an `Approved` plan and `Ready` task packets. Return `BLOCKED` when
   claimed plan artifacts are missing, malformed, unapproved, or cannot be
   matched to the review target. Do not report a structural omission after the
   validator passes.
4. Read the `$wyrd-implement` completion evidence when available. Treat it as a
   claim to validate against source, tests, and command results rather than as
   proof by itself.
5. Read the nearest owning manifests, relevant `mise.toml` tasks, tests, and
   generated-artifact sources for the changed surface.

If no plan relationship is claimed, mark plan conformance `Not applicable` and
continue the Wyrd repository review. Do not invent or require a historical
plan.

Apply authority in this order:

1. current user instructions;
2. active task packet;
3. approved parent plan;
4. applicable `AGENTS.md` files;
5. repository architecture and conventions.

When `.codegraph/` exists, use CodeGraph before repository search or manual
source-reading loops to locate changed symbols, callers, ownership boundaries,
and local precedents.

## Prove plan conformance

Build a traceability matrix before assigning findings. Map every in-scope plan
requirement and task acceptance criterion to:

- the implementation path and symbol;
- the test or generated artifact that proves it;
- the required verification command and recorded result;
- `PASS`, `FAIL`, or `UNVERIFIED`.

Check both omission and drift:

- every required change, interface, invariant, failure case, and test exists;
- normative pseudocode semantics, ordering, transactions, error mapping, and
  side effects are preserved;
- non-goals, prohibited changes, task boundaries, dependencies, and feature
  requirements are respected;
- later tasks and adjacent cleanup did not enter the diff;
- every changed file maps to a requirement or necessary verification support;
- every deviation was approved before implementation and reflected in the
  canonical plan when material;
- focused and integrated verification evidence uses the exact required
  commands and features.

Classify confirmed findings as:

- `MISSING_IMPLEMENTATION`: an approved requirement or acceptance criterion is
  absent or incomplete;
- `PLAN_CONTRADICTION`: implemented behavior conflicts with an approved
  requirement, decision, interface, or invariant;
- `SCOPE_DRIFT`: the diff enters prohibited, unrelated, or later-task scope;
- `UNAPPROVED_DEVIATION`: implementation changed a material decision without
  prior approval and plan revision;
- `MISSING_VERIFICATION`: required tests, generated checks, features, commands,
  or objective evidence are absent or insufficient;
- `CODE_DEFECT`: the implementation is plan-conformant in intent but incorrect
  in source;
- `REPOSITORY_RULE_VIOLATION`: the implementation violates Wyrd authority or
  an explicit repository rule.

## Wyrd-specific checks

- Verify the change uses Wyrd-native nouns, paths, contracts, and vocabulary;
  do not introduce legacy names, compatibility aliases, or stale migration
  surfaces.
- Confirm the owning crate or package matches the ownership boundaries in
  `AGENTS.md`, including dependency cost and client/server separation.
- Check Cards, `CardRef`, API versions, typed request/response bodies, stable
  errors, generated artifacts, MCP/CLI surfaces, and SDK boundaries against
  the active design authority when they are touched.
- Check tenant isolation, audit boundaries, server-owned durable behavior, and
  language-agnostic wire contracts when relevant.
- Check PyO3 feature gates, thin module aggregation, generated stubs, and
  public Python exports when cross-language surfaces are touched.
- Check Wyrd's test taxonomy and canonical `mise` task for the changed
  surface. A user- or agent-facing capability needs a user-journey test; a
  unit test alone is not sufficient.
- Enforce `AGENTS.md` §5 "Required Struct-Centered Rust Style" as a hard
  acceptance criterion. Treat new or materially changed module-level
  orchestration, repeated dependency/context threading, anemic structs whose
  natural behavior is detached, zero-sized utility structs, broad god objects,
  and traits around one implementation as confirmed findings. Do not force a
  genuinely stateless deterministic helper onto an artificial owner, and do
  not move registry, storage, policy, audit, or lifecycle IO onto declarative
  Card envelopes.
- Enforce the `AGENTS.md` §16 rustdoc requirement as a hard acceptance
  criterion. Inspect every new or materially modified Rust module, type, field,
  variant, trait item, constant, alias, function, method, helper, and test.
  Missing, placeholder, or mechanically restated rustdoc is a confirmed
  `BLOCK_BEFORE_MERGE` finding. Fallible functions require `# Errors`; document
  panics and async cancellation, partial progress, or retry behavior when
  applicable.
- Enforce synchronous Rust as the default. Every changed `async fn` must
  directly await IO or intentionally compose operations that do. Flag pure
  validation, parsing, planning, or transformation made async for caller
  uniformity or hypothetical future IO.
- Treat violations of `architecture/agent-rules.md` or explicit `AGENTS.md`
  requirements as confirmed evidence. Apply the shared materiality gate rather
  than silently ignoring a non-blocking violation.

## Assign the verdict

- `APPROVE`: every in-scope requirement and acceptance criterion passes, all
  required evidence is present, and no blocking finding remains.
- `CHANGES_REQUIRED`: the implementation can satisfy the approved plan through
  bounded source or test corrections without changing a material decision.
- `REPLAN_REQUIRED`: repository evidence disproves the approved approach or a
  correct fix requires changing architecture, contracts, scope, persistence,
  security, dependencies, features, or verification materially.
- `BLOCKED`: the claimed plan, task, review target, or required evidence cannot
  be resolved well enough to perform the review.

Do not approve based on intent, compilation alone, narrative evidence, or a
subset of acceptance criteria. Do not prescribe an implementation fix under
`REPLAN_REQUIRED`; return the conflict to `$wyrd-plan`.

## Write the review artifact

Write every completed review to a durable file. Use a caller-provided review
directory when present. Otherwise create:

```text
.dev/review/<short-head>-<UTC-YYYYMMDD-HHMMSS>-wyrd-review/review.md
```

Write the review before invoking another skill. Use this exact ordered
contract:

```markdown
# Wyrd Implementation Review

Status: APPROVE | CHANGES_REQUIRED | REPLAN_REQUIRED | BLOCKED
Repository: wyrd
Review ID: <review directory basename>
Created: <UTC YYYY-MM-DD>
Review target: <diff, range, or working tree>
Plan: <path, Not applicable, or unresolved>
Tasks: <task IDs, Not applicable, or unresolved>
Remediation plan: <path, not required, pending, or blocked>

## Summary

<Verdict and observable implementation state.>

## Plan Conformance

| Requirement / AC | Implementation | Test / artifact | Verification | Result |
|---|---|---|---|---|

## Findings

### WRD-001: <plain-English issue>

Type: <finding classification>
Plan or rule: <exact plan/task clause or repository authority>
Source: <changed path and line>
Impact: <concrete failure or maintenance path>
Required change: <bounded correction, or return to planning>
Verification: <canonical command>

## Verification

- Commands evidenced or independently checked, with exact results.
- Required commands that remain unverified.

## Inspected Surfaces

- Wyrd-specific owners, contracts, generated artifacts, and tests inspected.

## Planning Handoff

<Confirmed finding IDs, original plan/task paths, required outcomes,
constraints, evidence, and remediation-plan disposition.>
```

Consolidate duplicate symptoms under one root cause. A clean review with no
findings is valid, but still include the complete conformance matrix,
verification state, and inspected surfaces. Use
`Not applicable: <one-sentence reason>` for an inapplicable section; never
leave a section empty.

## Route confirmed findings through wyrd-plan

Do not make `review.md` imitate an executable task packet. `$wyrd-plan` owns
the canonical plan and task schemas; `$wyrd-implement` consumes its `Ready`
task files.

After writing `review.md`:

- For `APPROVE`, set `Remediation plan: not required`, explain that no work
  remains in `Planning Handoff`, and stop.
- For `BLOCKED`, set `Remediation plan: blocked`, record the exact missing
  authority or evidence, and stop.
- For `CHANGES_REQUIRED` or `REPLAN_REQUIRED`, set
  `Remediation plan: pending` and, unless the caller explicitly requested a
  review-only result, invoke `$wyrd-plan`.

Treat invocation of `$wyrd-review` as authorization to write only the review
and remediation-plan artifacts described here. It does not authorize source,
test, generated-artifact, migration, or configuration changes.

Before invoking `$wyrd-plan`, announce that `$wyrd-review` is handing confirmed
findings to planning. Load `.codex/skills/wyrd-plan/SKILL.md` to EOF and follow
its complete workflow. Provide:

- the durable `review.md` path;
- the original plan and in-scope task paths;
- the exact reviewed diff, range, or working-tree target;
- the conformance matrix and confirmed finding IDs;
- source-validated evidence and verification gaps;
- the review verdict and why the existing implementation can be corrected or
  requires a superseding design.

Require `$wyrd-plan` to create a new canonical remediation plan under
`.dev/plan/<slug>/implementation-plan.md` with separate task packets under
`.dev/plan/<slug>/tasks/`. It must:

- convert confirmed findings into observable requirements and acceptance
  criteria;
- preserve traceability to the original plan, tasks, and review finding IDs;
- consolidate findings only when one root-cause task remains cohesive;
- define exact allowed and prohibited scope, interfaces, tests, commands,
  features, escalation, and completion evidence;
- explicitly supersede affected original decisions when the verdict is
  `REPLAN_REQUIRED`;
- run the canonical plan artifact validator;
- apply the normal approval and independent plan-review gates before marking
  tasks `Ready`.

Do not edit the original approved plan merely to match the implementation.
Do not hand findings directly to `$wyrd-implement`. If `$wyrd-plan` needs a
material user decision, keep the remediation plan pending and return that
blocker without weakening the review.

After planning succeeds, update `review.md`:

- replace `Remediation plan: pending` with the canonical plan path;
- add the generated task paths and approval state to `Planning Handoff`;
- preserve the original findings and verdict unchanged.

Return the review path, remediation plan path, and every `Ready` task path.
State explicitly that `$wyrd-implement` receives one generated task file, not
`review.md`.
