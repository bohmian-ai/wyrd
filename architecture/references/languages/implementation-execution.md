# Bounded Implementation Execution

This reference defines the execution discipline for an actionable Wyrd change.
It covers authority classification, controlled adaptation, verification
recovery, test integrity, diff audit, and completion evidence without requiring
a particular request or packet shape.

## Contents

- [Authority](#authority)
- [Change contract](#change-contract)
- [Three execution classes](#three-execution-classes)
- [Execution record](#execution-record)
- [Failure routing](#failure-routing)
- [Repository-managed environments](#repository-managed-environments)
- [Equivalent verification](#equivalent-verification)
- [Implementation and test integrity](#implementation-and-test-integrity)
- [Focused verification](#focused-verification)
- [Final diff audit](#final-diff-audit)
- [Completion standard](#completion-standard)

## Authority

Apply instructions in this order:

1. current user instructions and explicitly locked outcomes;
2. applicable `AGENTS.md` files and repository architecture authorities;
3. the request's requirements, constraints, claims, and accepted evidence;
4. repository conventions and established owner patterns;
5. local implementation preferences.

The request fixes required behavior, acceptance outcomes, explicitly approved
public or persisted contracts, prohibited changes, and safety boundaries.
Repository architecture remains authoritative for ownership, protocol, tenant,
security, and operational invariants unless the user explicitly changes that
authority. Repository reality fixes private mechanics when several approaches
satisfy the same contract.

## Change contract

Before editing, establish:

- objective, requirements, non-goals, and acceptance criteria;
- allowed and prohibited material scope;
- owners, interfaces, invariants, and normative control flow;
- dependencies and exact required features;
- tests, focused verification, and completion evidence;
- material stop conditions.

Direct instructions, issues, Markdown, structured packets, and bounded review
findings are all valid inputs. No YAML shape, section name, ID, digest, clean
worktree, fixed write set, or predeclared command is required. When explicit
claims are absent, derive a concise working checklist from the requested
behavior and repository completion rules. A stale private name, defective
command recipe, or missing local test setup does not make an otherwise clear
behavioral contract malformed.

Keep the checklist live until every actionable requirement and required proof
is complete.

## Three execution classes

### Local and reversible

Implement autonomously:

- private helpers and local structure;
- rustdoc, formatting, and diff-caused lint repairs;
- mechanical caller and test updates;
- current equivalents of private names and paths;
- existing fixtures and test-support seams;
- in-scope compile and test repairs.

These changes preserve behavior and remain inside the established owner.

### Bounded correction

Proceed, record, and continue:

- replacing a defective command or filter with equivalent non-weaker proof;
- adding a narrow adjacent private fixture or support file within the owner;
- repairing a small pre-existing defect in touched code;
- provisioning repository-managed local services, migrations, and test
  environment;
- recording a proven unrelated baseline failure while continuing unaffected
  proof;
- recording repository discoveries with the execution evidence.

The expected file list is not a strict whitelist for adjacent private
implementation and verification support. Explicit prohibited material paths
remain prohibited.

### Material

Stop before implementation when correctness requires:

- changing a public, wire, generated, or persisted contract;
- adding or changing a migration or destructive data behavior;
- adding a dependency or Cargo feature;
- changing authentication, authorization, tenancy, policy, audit, secret, or
  data-loss semantics;
- redesigning ownership or dependency direction;
- changing requirements or acceptance outcomes.

Report repository evidence, why no in-scope solution remains, and the exact
authority required to resume.

## Execution record

The execution record may append or correct:

- current repository facts;
- internal paths and private symbol names;
- existing or added private implementation/test-support seams;
- equivalent verification commands;
- progress, failures, and evidence.

Record:

```text
date/attempt
classification: local | bounded correction
discovery and repository evidence
requirement, claim, or command affected
action and verification result
```

Do not rewrite the objective, requirement or decision IDs, required behavior,
public or persisted contracts, security semantics, material architecture or
ownership boundaries, prohibited material boundaries, or acceptance criteria.
Stop for a material decision when one must change.

## Failure routing

Use this order:

1. If the diff caused the failure, fix it and rerun.
2. If the defect is small and in touched code, fix and report it.
3. If the command or filter is defective, establish equivalent proof and
   rerun.
4. If private plumbing or test support is missing, add the smallest in-owner
   support and continue.
5. If local repository setup is missing, provision it and rerun.
6. If the failure is unrelated, prove that classification and continue all
   unaffected work and checks.
7. If a material change or unavailable external authority is required, finish
   unaffected work and return `BLOCKED`.
8. Otherwise continue localization and diagnosis.

Compiler, test, formatter, Clippy, rustdoc, process, or tool failures are
diagnostic results. They do not terminate execution by category.

Do not return partial work, create a remediation plan, or wait for another
prompt while an in-scope recovery path remains.

## Repository-managed environments

Missing local services and local test variables are reversible mechanics when
the repository defines them.

Inspect:

- the relevant `mise.toml` task body;
- its `depends` setup and migration tasks;
- its checked-in `env` values;
- invoked scripts and documented local emulators.

Then start the service, run migrations, invoke the owning task, or supply the
same checked-in local-only values to an equivalent focused command.

For example, a focused Postgres test must not stop merely because
`WYRD_DATABASE_URL` is absent from the current shell when its canonical `mise`
task defines the value and depends on Postgres setup.

Never invent or expose production secrets. An external credential, protected
service, or user-only permission is unavailable authority only when the
repository provides no local substitute and required proof cannot be
established otherwise.

## Equivalent verification

A corrected command is equivalent only when it:

- proves the same acceptance criterion;
- exercises the same relevant code, feature, and target;
- preserves required negative, integration, and environment behavior;
- is no weaker in assertions or dependency coverage;
- records why the original recipe was defective.

Examples:

- add `--lib` when an unrelated explicit integration target is invalid under
  the intended default-feature proof;
- correct a stale test filter to the current test name;
- invoke the canonical `mise` task that supplies Postgres setup instead of a
  raw Cargo command missing its environment.

Do not call a unit test equivalent to a required integration or user-journey
test. Do not silently drop features, assertions, negative flows, or consumers.

## Implementation and test integrity

Follow `AGENTS.md` and the applicable language references. In particular:

- preserve operation order, validation, transactions, concurrency,
  cancellation, side effects, errors, and invariants;
- prefer existing concrete owners and repository patterns;
- avoid speculative abstractions, dependencies, features, and cleanup;
- write tests alongside changed behavior;
- map every test to an acceptance criterion;
- preserve the repository's required rustdoc and struct-centered Rust style.
- keep tenant SQL behind `TenantConn` or the explicitly authorized
  `OperatorPool`; a `TenantConn` callee never commits or rolls back;
- rely on Postgres RLS rather than duplicating tenant predicates on
  tenant-scoped queries;
- append audit at each independently durable transition, preserving Oracle's
  WAL-before-rows exception and Forge's fenced operator capability;
- resolve, screen, and pin user- or tenant-supplied URL destinations before
  server-side fetches so DNS rebinding cannot bypass SSRF policy.

Never pass a check by:

- removing or weakening assertions;
- ignoring, disabling, or deleting a required test;
- adding sleeps instead of deterministic synchronization;
- mocking away required behavior;
- swallowing errors or changing semantics to fit an incorrect test;
- adding an unjustified lint allowance;
- editing generated artifacts instead of their source.

An actual conflict between a required test and an approved material contract
is a material blocker.

## Focused verification

Run the smallest complete affected surface sequentially:

1. narrow test or reproduction;
2. broader affected owner or package task;
3. applicable integration, contract, codegen, typing, migration, docs, or
   boundary checks;
4. repository-required format and lint checks;
5. `git diff --check`;
6. final diff inspection.

Prefer repository `mise` tasks after inspecting their implementation and setup.
Use direct Cargo for a narrow pure test or focused filter when no suitable task
exists. Run Cargo-backed work sequentially across agents sharing a target.

Use default features unless the affected behavior explicitly requires optional
features. Do not use `--all-features` for focused verification unless the
affected behavior requires the full feature union. Broad feature-union gates
belong to integrated verification.

When a failure is proven unrelated, continue if it does not prevent required
proof. If it prevents a mandatory acceptance outcome and no equivalent proof
exists, return `BLOCKED` only after every unaffected item is complete.

## Final diff audit

Inspect tracked and untracked changes. For every changed file, confirm:

- the requirement or verification need that requires it;
- it is inside the established owner or explicitly approved scope;
- no unrelated formatting or cleanup entered the diff;
- public-contract, dependency, feature, migration, and security effects are
  unchanged unless explicitly approved;
- generated outputs came from their source;
- tests prove behavior without weakened integrity;
- no debug artifacts, real secrets, or machine-specific values remain.

Durable execution notes must follow [Execution record](#execution-record).

## Completion standard

`COMPLETE` requires every acceptance criterion and required proof to pass, no
prohibited change, sequential verification, a clean diff audit, and recorded
bounded corrections.

`BLOCKED` requires evidence that no in-scope solution remains without a
material decision or genuinely unavailable authority. Difficulty, elapsed
time, partial progress, context compaction, a recoverable command failure,
missing repository-managed setup, or a convenient handoff point do not qualify.

Use this report:

```markdown
## Task Result

Status: COMPLETE | BLOCKED

### Summary
<Implemented outcome>

### Acceptance Criteria
- AC1: PASS | FAIL | UNVERIFIED — <source and test evidence>

### Files Changed
- `path` — <requirement or acceptance criterion>

### Tests Added or Updated
- <test> — <behavior proved>

### Verification
Commands executed sequentially:
1. `command` — PASS | FAIL

Features enabled:
- `crate`: default | `<exact features>`

### Execution Updates
- None | <local or bounded correction, evidence, and result>

### Material Deviations
- None | <approved deviation and impact>

### Blocking Work
- None | <blocked item and exact authority required>

### Risks and Notes
- None | <non-blocking evidence>

### Diff or Commit
- <reference when available>
```
