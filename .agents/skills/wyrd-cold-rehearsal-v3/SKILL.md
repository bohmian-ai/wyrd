---
name: wyrd-cold-rehearsal-v3
description: Adversarially rehearse a Wyrd v3 plan milestone before implementation and return only digest-bound PASS, FAIL, or STALE. Use after wyrd-plan-v3 artifacts exist and before any task is marked Ready or delegated.
---

# Wyrd Cold Rehearsal V3

Rehearse only the supplied `plan.md`, task packets, `execution-manifest.json`,
pinned repository revision, and normal repository authorities. Do not invoke a
v1 or v2 skill. Do not use planning conclusions, prior rehearsal findings, or
an intended solution as hidden context.

Run the root rehearsal invocation only as `gpt-5.6-sol` with `low` reasoning;
otherwise stop without issuing a verdict.

## Gate

Run the v3 validator first:

```bash
python3 .agents/skills/wyrd-plan-v3/scripts/validate_v3_artifacts.py <plan-directory> --repository-root <repository-root>
```

Return `STALE` when any artifact digest or repository revision differs from the
manifest. Return `FAIL` for every other validator error.

For a structurally valid milestone, independently simulate implementation in
DAG order using current source. All delegated rehearsal roles, when available,
must use exactly `gpt-5.6-sol` with `low` reasoning. Inspect every named path,
symbol, precedent, fixture, test, and command. Do not edit source.

Reject a task when it:

- needs code, generated output, migration state, or a mutable fixture owned by
  a peer task not in its transitive dependencies;
- leaves a public, durable, cross-owner, typed, error, ownership, transaction,
  lifecycle, cancellation, concurrency, recovery, or audit contract to the
  implementer;
- lacks exact path/symbol/why ownership or uses a vague placeholder;
- lacks lifecycle or concurrency proof, including an evidence-backed
  inapplicability explanation;
- uses raw Postgres setup instead of an established fixture or mise lifecycle;
- maps an acceptance criterion to an unnamed test, a non-exact command, an
  unexecuted preflight, or invalid evidence for its declared test kind;
- has overlapping parallel writes, a cycle, an unstated dependency, or cannot
  leave the repository coherent at its boundary.

Reject the milestone when concurrency was not designed rather than merely
declared. Recompute the selected and alternative DAG metrics with three
implementor slots. Confirm every serialization edge consumes predecessor code
or a committed contract; remove edges caused only by Cargo contention,
Postgres/stateful contention, integration order, manifests, registration, or
generated outputs that can be isolated in a foundation or join task. Attempt a
foundation/module/join decomposition for every shared central owner. Return
`FAIL` when a speed-optimized plan misses its critical-path, runnable-width, or
parallel-fraction threshold without an exact source-backed waiver explicitly
accepted by the user.

Simulate source implementation, root integration, Cargo verification, and
stateful verification as separate schedules. Report a failure when the plan
uses a verification constraint to serialize independently compilable source
candidates. Confirm that claimed utilization and wall-time improvement are
reproducible from the manifest graph rather than prose.

For `test_kind: new`, validate the planned test path and name against the task's
write set and required-test section, the accepted package/target via successful
compile-only preflight, and the positive expected post-implementation count.
Zero selected tests is correct only for this explicit new-test preflight; never
treat it as executed acceptance proof.

## Verdict

Write exactly one canonical `rehearsal.json` outside the manifest artifact set
and emit the same machine-readable result with no advice:

```json
{"verdict":"PASS|FAIL|STALE","plan_id":"...","repository":{"origin":"...","revision":"..."},"manifest_digest":"sha256:...","artifact_digests":{"plan.md":"sha256:..."},"failures":[{"task":"T1","code":"...","evidence":"..."}]}
```

`PASS` requires zero failures and exact current digests. `FAIL` includes every
material failure found. `STALE` includes only digest/revision mismatches. The
verdict is valid only for those digests; any material artifact change requires
a new cold rehearsal.
