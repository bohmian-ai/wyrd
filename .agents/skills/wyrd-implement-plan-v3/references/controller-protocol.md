# Controller Protocol

## Durable state

Persist one JSON document with `version: 1`, a stable `controller_id`, optional
`repo_root`, `mutable`, and `events`. `mutable` contains immutable
`initial_head`, current `integration_head`,
`evidence_commit`, `lanes`, `integration_order`, and every task. Do not keep
mutable scheduling truth outside this projection.

Each event contains the complete resulting `mutable` projection, `sequence`,
`transition`, `task_id`, `previous_hash`, and `hash`. Hash canonical JSON of the
event excluding `hash` with SHA-256. The first `previous_hash` is 64 zeroes;
subsequent events name the preceding event hash. Sequences start at one and are
contiguous. Resume only by validating the chain and requiring the last event's
projection to equal top-level `mutable`.

Every task projection records:

- `generation`, canonical task `digest`, and `revision`;
- `dependencies`, semantic `locks`, declared `write_set`, and `status`;
- dispatch `base_sha`, immutable `candidate_sha`, `candidate_parent`, and
  `diff_digest`/stable `patch_id`;
- stable `proof_requirements` keyed by proof ID, each with its exact integrated
  `prerequisites`; `proof_attempts` keyed by proof ID; and `accepted_proofs`
  keyed by proof ID with exactly one PASS for every required proof;
- exact review `verdict`, candidate binding, controller-state event hash, and
  content-addressed `result_digest`;
- optional `integrated_commit`, its declared direct `integrated_parent`, and
  the external-plan `evidence_commit` produced
  after source and evidence acceptance.

Populate every task projection completely in the initialization event. The
task digest binds its dependencies, locks, write set, and proof requirements;
none may be filled lazily. `write_set` must be a nonempty list of normalized
repository-relative paths. `dependencies` and `locks` must be present,
duplicate-free lists even when empty.

Content digests are lowercase SHA-256. Git object IDs are lowercase 40- or
64-character hexadecimal values. Artifact digests describe immutable proof or
review content; they are never predicted Git commit SHAs. Record the resulting
external-plan commit separately only after committing canonical evidence.

## Lanes

Configure zero, one, or two Cargo lanes. Every configured lane has a distinct,
nonempty worktree and `target_dir`, plus zero or one owner. A task owns at most
one Cargo lane. `stateful_owner` is zero or one active task and is the exclusive
lease for migrations, shared services, generators, databases, package caches,
and other mutable external state. `review_owners` contains at most two distinct
candidate tasks.

## Candidate and proof rules

Use these status transitions:

```text
ready -> active -> candidate -> accepted -> integrated
                    |    ^
                    v    |
                  active
any non-integrated state -> frozen -> ready
```

`frozen -> ready` requires a larger task `revision` or `generation` and a new
canonical digest. A candidate successor increments `generation`, receives a
new immutable SHA/diff identity and current `integration_head` as `base_sha`,
replays only the scoped patch, and clears every proof stream, review, and
integration field.

Each required proof ID has an independent ordered attempt stream. Each attempt
records its proof ID, ordinal, candidate SHA and generation, full command as an
argument array, Cargo/stateful lane identity, exit code, selected-test count,
classification, artifact digest, and the current `integration_head`. Before
recording an attempt, every task named by that proof's `prerequisites` must be
integrated. Proof prerequisites do not participate in dispatch readiness or
the implementation dependency DAG.

If a candidate's `base_sha` is not the current `integration_head` after a proof
prerequisite integrates, transition `candidate -> active`, increment
`generation`, create a fresh descendant candidate from the current integration
head by replaying the scoped patch, and clear all proof attempts, accepted
proofs, and review. This refresh is mandatory even when the old candidate
cherry-picks cleanly.

An accepted proof is exactly one passing attempt in its proof stream for the
current candidate with exit zero and selected count greater than zero. Failed
`infrastructure` or `setup` attempts may precede it. A `source` or `test`
failure forbids another attempt for that candidate in any stream: create a
successor candidate first. Every required proof ID has exactly one accepted
PASS; unknown proof IDs and duplicate PASS attempts are invalid.

Implementor runs of a repeatable task-focused proof command are development
diagnostics, not attempts in this proof stream. They do not consume the
controller's authoritative attempt or qualify the candidate. The controller
runs the proof fresh after sealing. Destructive, non-repeatable, measurement,
source-bound reporting, cross-task, and terminal qualification commands remain
controller-only because an earlier run could mutate or contaminate evidence.

A reversible diagnostic, proof, or review failure advances candidate
`generation` without changing task `revision`. Increase task `revision` only
when contract, write-set authority, dependencies, acceptance criteria, or proof
authority changes.

Review records exact `APPROVE`, `RESUME_IMPLEMENTATION`,
`ORCHESTRATOR_DECISION_REQUIRED`, or `REVIEW_BLOCKED`; acceptance requires
`APPROVE`, the current candidate/generation, an immutable result digest, and the
exact controller event hash given to the reviewer. This proves reviewer scope
used the same controller-state identity the root later accepts.

## Scheduling and integration

Every event projection includes a scheduling report with separate
implementors, proof lanes, reviewers, advisors, frozen tasks,
dependency-blocked tasks, and selected-but-idle tasks. Recompute it after every
transition. Give each selected-but-idle task one concrete reason: dependency,
source or semantic conflict, approved serialization, Cargo lane, stateful lane,
or agent capacity. A generic `blocked`, `busy`, or `waiting` reason is invalid.
Dispatch every dependency-ready, nonconflicting task selected by the approved
schedule when an implementor slot exists. Validate this report with
`scripts/validate_schedule_pass.py` before dispatch or wait.

- Implementation dependencies form a DAG. A task becomes active only after
  every implementation dependency is integrated. Proof prerequisites never
  affect dispatch. Integration order is dependency-respecting and
  duplicate-free.
- At most three tasks are active/candidate/accepted implementor allocations and
  at most two are under review.
- Active/candidate/accepted tasks may not overlap write paths or semantic locks.
  Equal paths and ancestor/descendant paths overlap.
- Candidate identity must match its current generation. A changed SHA creates
  a successor generation and invalidates all old proof and review.
- Accepted/integrated tasks require one accepted PASS for every required proof
  ID and approved review for the current candidate, plus matching patch
  identity.
- The first integrated commit's direct parent is `initial_head`; every later
  integrated commit's direct parent is the preceding integrated commit. No
  unrelated source commit may intervene. Where `repo_root` is supplied, validate Git
  objects, candidate parent and ancestry, source diff digest, stable patch ID,
  integrated-commit ancestry/order, and cherry-pick patch identity.
- `integration_head` equals the last integrated commit when any task is
  integrated. Task and global external-plan evidence commit values agree.

## Material-decision guard

Apply a presumption of local repair before entering this guard. Compiler,
formatter, Clippy, rustdoc, generated-drift, fixture, and focused-test failures
produce a successor implementation generation. They do not create an
escalation record. `$wyrd-implement-v3` directly applies and reports a
task-local mechanical path correction outside the original write set when the
candidate caused the diagnostic and the change adds no behavior, owner,
dependency, public contract, acceptance scope, unrelated cleanup, or prohibited
path. The root validates the reported path and diagnostic with the candidate;
it does not revise the task or request user authorization. Mere write-set
omission is not evidence of a material decision.

Enter the material-decision state machine only when source evidence identifies
an unresolved choice among materially different behaviors or a change to
product, durability, public contract, ownership, dependency, security, DAG, or
acceptance authority.

Keep one escalation record per frozen cone. Its phase advances only through:

```text
requested -> recommended -> root_validated -> root_accepted | root_rejected
```

The advisory artifact must pass `scripts/validate_advisory_result.py`. It names
one recommendation, rejected alternatives, evidence, authorities, impact cone,
and whether the decision changes the V1 objective, DAG, or cross-task semantic
ownership. Enumerating alternatives or delegating the choice is not a
recommendation.

After `root_accepted`, stage proposed successor artifacts outside canonical
execution authority. Promote a bounded successor task only after its digest and
complete scheduling identity validate. Invoke `$wyrd-plan-v3` only when the
accepted decision explicitly changes the objective, DAG, or cross-task semantic
ownership. Unaccepted or merely proposed artifacts never drive dispatch and do
not invalidate the last accepted plan.

Run:

```bash
python3 .agents/skills/wyrd-implement-plan-v3/scripts/validate_controller_state.py state.json
```
