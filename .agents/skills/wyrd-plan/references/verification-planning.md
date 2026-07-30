# Verification Planning

Design verification before task generation. Focused checks prove bounded task
acceptance; integrated closeout proves the feature.

## Contents

- [Inspect live verification](#inspect-live-verification)
- [Map requirements to proof](#map-requirements-to-proof)
- [Design task verification](#design-task-verification)
- [Choose the test tier](#choose-the-test-tier)
- [Plan closeout](#plan-closeout)
- [Examples](#examples)
- [Completion evidence](#completion-evidence)

## Inspect live verification

Read `mise.toml`, `AGENTS.md` §11, and
`architecture/references/languages/testing-workflows.md`. Name only commands
that exist. Account for environment setup, feature selection, generated
artifacts, external services, and test tier.

Prefer repository `mise` tasks when they cover the required surface. Use raw
Cargo only for a narrower pure test that needs no repository setup.

Before a task becomes `Ready`, execution-check every proposed command:

1. inspect the actual task or script body;
2. confirm package, feature, explicit target, filter, fixture, and support
   export names against current source and manifests;
3. run the narrow command when feasible;
4. at minimum compile the exact target with the exact feature selection;
5. prove the filter selects the intended tests rather than zero or an
   unrelated target;
6. identify required repository setup, services, migrations, and checked-in
   local test environment.

Record the exact command that worked. If an external service is unavailable,
separate proven command/setup shape from unexecuted runtime behavior. Do not
mark unavailable runtime evidence as passed.

## Map requirements to proof

For every requirement, identify:

- the task that implements it;
- the highest-value test tier that proves it;
- named tests or journeys and their critical assertions;
- the exact focused command;
- generated-artifact, typing, schema, docs, migration, or boundary checks;
- the closeout evidence confirming integrated behavior.

| Requirement | Task | Test or evidence | Command | Closeout |
|---|---|---|---|---|
| `R1` | `T1` | Named Rust integration test | Existing focused `mise` task | Integrated journey |
| `R2` | `T2` | Public Python workflow | Python integration task | Typecheck + journey |

A requirement without objective proof is not ready.

## Design task verification

For every task packet state:

1. affected crates, packages, modules, and dependent consumers;
2. required features and why each non-default feature is earned;
3. required tests, setup, action, and important assertions;
4. exact commands in sequential order;
5. broad or unrelated commands the task must not run;
6. evidence the implementer must return.

Exact commands are executable recipes, not immutable user requirements. Mark a
command normative only when its exact lane, feature set, or environment is
itself part of the acceptance contract. Otherwise permit `$wyrd-implement` to
replace a defective command with equivalent non-weaker proof and record the
correction in the task.

Select the smallest complete dependency surface:

1. narrow unit or integration tests for changed behavior;
2. applicable crate-family or language task;
3. boundary, codegen, typing, schema, migration, or docs checks caused by the
   change;
4. format and default-feature workspace checks at a stable boundary.

Use default features or the exact optional features exercised by the task. Do
not use `--all-features` as a generic task test option.

Run all Cargo-backed commands sequentially across agents sharing a checkout or
target directory. Parallel source work must not overlap builds, tests, Clippy,
rustdoc, migrations, or Cargo-backed codegen.

## Choose the test tier

Wyrd ranks proof:

1. user journey;
2. integration test;
3. unit test.

Every new user- or agent-facing capability requires a real SDK → server → SDK
journey. Cover the smallest successful path, common repeat behavior, and
negative or edge behavior a caller encounters. Cover only the first-class
surfaces on which the capability ships.

Use lower tiers for pure logic, exact seam contracts, and failures materially
harder to drive end to end. Record why a user-observable negative flow remains
below the journey tier.

## Plan closeout

After all tasks are integrated:

1. confirm each task review and acceptance report;
2. run final integration and journey tasks;
3. run affected language, codegen, docs, migration, and boundary gates;
4. run the all-feature workspace closeout once;
5. run `mise run pre-pr` only when the plan requires it, shared CI/build/test
   infrastructure changed, a release is being prepared, or the user requested
   it;
6. inspect the complete diff and produce final requirement traceability.

Avoid duplicate verification when the source revision and captured result
remain valid. Rerun affected gates after later changes.

## Examples

### Localized Rust task

```markdown
Verification surface:

- Affected crate: `wyrd-registry`
- Affected tests: workspace hydration unit tests
- Required features: default only
- Unaffected: server, Python, TypeScript, storage, generated contracts

Run sequentially:

1. `mise exec -- cargo test --locked -p wyrd-registry hydrate -- --nocapture --test-threads=1`
2. `mise run test:shared`
3. `mise run fmt`
4. `mise run check:default`
5. `git diff --check`

Do not run:

- `mise run check`
- `mise run pre-pr`
- unrelated language suites
- concurrent Cargo commands
```

### Public Python projection task

```markdown
Verification surface:

- Rust owner: `wyrd-sdk` with exact `python` feature
- Python exports and generated stubs
- Public Python journey against `WyrdTestServer`

Required tests:

- Unit import test asserts the public `wyrd` path.
- Integration journey asserts success and the stable negative error.
- Generated stubs expose the exact argument and result types.

Run sequentially:

1. `mise run test:wyrd-sdk`
2. `mise run py:setup`
3. `mise run py:test:unit`
4. `mise run py:test:integration`
5. `mise run py:typecheck`
6. `mise run codegen:check`
7. `mise run py:format`
8. `mise run py:lints`
9. `mise run check:default`

Do not run `pre-pr`; the parent plan owns integrated closeout.
```

### Requirement traceability

```markdown
| Requirement | Task | Implementation | Proof | Result |
|---|---|---|---|---|
| R1 | T1 | Server owner and typed response | HTTP/Rust journey | Required pass |
| R2 | T2 | Python SDK projection | Python integration journey | Required pass |
| R3 | T1, T3 | Authz and MCP projection | Permission-denied MCP journey | Required pass |
```

## Completion evidence

Require each task implementer to report:

- task status;
- every acceptance criterion as PASS, FAIL, or UNVERIFIED;
- source and test evidence;
- commands executed in order and exact results;
- feature sets used;
- generated outputs checked;
- deviations and approvals;
- unresolved or unverified work;
- diff or commit reference.

Task completion does not imply feature completion. Only integrated closeout
and final traceability complete the plan.
