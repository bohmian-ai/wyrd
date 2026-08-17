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

Review records exact `APPROVE`, `RESUME_IMPLEMENTATION`,
`ORCHESTRATOR_DECISION_REQUIRED`, or `REVIEW_BLOCKED`; acceptance requires
`APPROVE`, the current candidate/generation, an immutable result digest, and the
exact controller event hash given to the reviewer. This proves reviewer scope
used the same controller-state identity the root later accepts.

## Scheduling and integration

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

Run:

```bash
python3 .agents/skills/wyrd-implement-plan-v3/scripts/validate_controller_state.py state.json
```
