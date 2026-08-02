# Orchestrator Authority

Use this reference when repository reality invalidates a plan, a worker reports
a blocker, a review requires a material decision, or integrated verification
exposes a cross-task defect.

## Authority order

Apply:

1. current user instructions and approved autonomous-execution policy;
2. the intended user or agent outcome of the supplied plan;
3. current `AGENTS.md`, Wyrd design, doctrine, and routed architecture
   references;
4. current repository evidence and established owners;
5. the canonical living plan and tasks;
6. local implementation preferences.

The plan is a living executable specification, not immutable history. Preserve
its outcome; revise its mechanics and material decisions when current evidence
shows a safer, simpler, more correct, or more ergonomic answer.

## Autonomous decision test

The orchestrator may decide and implement product, security, public contract,
persisted contract, migration, dependency, feature, ownership, verification,
task-boundary, and acceptance changes when all are true:

1. the decision is necessary to complete or materially improve the intended
   outcome;
2. repository authority and evidence support one reasonable answer;
3. correctness, security, tenant isolation, auditability, ergonomics, and user
   experience are preserved or improved;
4. the choice does not hide data loss, weaken proof, or introduce compatibility
   behavior that Wyrd doctrine rejects;
5. affected requirements, decisions, architecture authority, consumers,
   language projections, tests, tasks, and closeout gates are updated
   cohesively;
6. a fresh `wyrd-plan-reviewer` advisory pass approves the material revision.

Prefer the smallest cohesive resolution. Do not introduce speculative
abstractions, compatibility shims, dependencies, or future-proofing.

## Decision record

For every material revision, record in the canonical plan:

- discovery and repository evidence;
- superseded requirement or decision;
- selected resolution and rejected alternatives when materially plausible;
- correctness, security, ergonomics, compatibility, migration, and user
  effects;
- affected tasks and proof;
- advisory review result;
- plan version and date.

Update the owning Wyrd design or doctrine authority in the same change when the
decision replaces it. Do not leave implementation and authority contradictory.

## Blocker-resolution ladder

1. Inspect the worker's source and command evidence.
2. Reclassify local mechanics, bounded corrections, repository-managed setup,
   defective commands, and unrelated failures as implementation work.
3. Return a binding remediation to the same implementor.
4. Promote or replace the implementor when complexity or repeated failure
   exceeds its route.
5. For a material conflict, revise the canonical plan/task and affected
   authority.
6. Obtain advisory plan review and resolve its findings.
7. Resume implementation and per-task review.

There is no retry limit. Repetition triggers stronger diagnosis or model
promotion, not abandonment.

## Genuine external blockage

Use `EXTERNAL_BLOCKED` only when correct completion requires something outside
the authorized local repository workflow that the orchestrator cannot obtain
or substitute, such as:

- a user-owned or protected credential with no repository-managed local
  equivalent;
- external infrastructure or permission unavailable to the runtime when its
  behavior is mandatory and cannot be proved locally;
- an irreversible external action outside the invocation's authorization;
- a supplied artifact so incomplete that no intended outcome can be
  discerned from it or repository authority.

Before blocking, finish every unaffected task and proof, record exact evidence,
and name the missing authority. Context limits, time, cost, compiler/test
failures, review findings, model failures, plan defects, or difficult product
decisions are not external blockers.
