# TASK-001 round 3 retry 1 — findings validation

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/verified-change-contract`
- Base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Cumulative candidate: `9d7b6266206f15136f306b66c06571066fc6bd13`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Prior review authority: both verdicts, validated ledgers, and remediation
  tasks under `review/TASK-001-r1/` and `review/TASK-001-r2/`
- Wave 1 inputs: `task-review.md`, `standards-review.md`,
  `domain-review-contracts.md`, `domain-review-data-durability.md`, and
  `domain-review-security-tenancy.md` in this directory

`HEAD` was exactly `9d7b62662` before and after source inspection. The tracked
candidate remained unchanged. The repository has no `.codegraph/` directory,
so caller and source tracing used `rg`, Git history, and direct source reads.

Applicable authority included `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/references/languages/spec-driven-development.md`, the Wyrd Card
and vocabulary authorities, the Bifrost table/restart/Oracle authorities, the
security and tenancy authority, and the focused Rust/testing references routed
by `architecture/references/README.md`.

## Wave 1 proposal adjudication

### `DDR3R1-1` — REVISED

The finding is reachable and material. `BUILTIN_TABLES` and
`builtin_tables()` were changed from eight entries to six in the cumulative
task diff, while `builtin_fqns` still promises eight and the materially changed
physical-layout owner still describes all eight. `builtin_fqns` is called by
the canonical registry test, and `physical_layout_contract_all_builtins`
iterates the same six-entry registry. The stale numeral therefore contradicts
the exact owner TASK-001 changed to retire `vala.eval.runs` and
`vala.eval.assertions`.

The proposed correction is narrowed: this is a two-word documentation repair,
so no new test or helper is justified. The existing registry tests already
exercise the owner; a scoped stale-numeral search plus the normal format/lint
checks is the smallest credible closure proof.

Retained as `FIND-TASK-001-15`.

### `CR3-C1` — REVISED

The proposed finding is the still-open remainder of `FIND-TASK-001-5`, not a
new finding. `load_eval_card` accepts only
`Spec::Verifier(VerifierImplementation::Eval(_))` and returns a
`CardKind::Verifier` reference, yet its parse and metadata diagnostics still
tell the caller it is loading or running an Eval Card. The retained public Eval
engine types are live consumers: `RunState` is constructed by the orchestrator,
and `RunIdentity` is constructed by the executor, comparison path, local CLI,
and tests. Their public rustdoc still describes the referenced Card as an Eval
Card.

Full scoped tracing found two more live public descriptions owned by the same
TASK-001 vocabulary obligation: `Resource::{Evals, Drift}` still documents
Eval/Drift Cards, and `SourceSpec`'s public module documentation still describes
a consuming Drift/Eval Card. These are public runtime/contract documentation,
not private identifier preferences. The correction therefore includes those
descriptions while preserving the stable `evals`/`drift` permission resources,
`eval_ref` field names, engine behavior, and all TASK-002-owned observation
record documentation. Private test expectation strings and the active change's
forward-looking architecture notes are not part of the correction.

Retained under its prior stable ID, `FIND-TASK-001-5`.

### `RS3-1` — CONFIRMED

Commit `5f14f3c32` is inside the immutable `5293546f3..9d7b62662` range. Its
only source change is an unrelated repository-wide process rule in
`AGENTS.md`, and its body contains the prohibited
`Co-Authored-By: Claude Opus 5 ...` trailer plus a `Claude-Session` trailer.
The AI-attribution prohibition already existed at the base, so the offending
commit cannot bootstrap authority for itself. TASK-001 requires neither this
process change nor its commit metadata, and a clean task verdict expressly
requires no unrelated change in the cumulative candidate.

A later source revert would remove the `AGENTS.md` diff but would not remove
the prohibited trailer from the reviewed commit range. The valid correction is
therefore to present a new immutable cumulative candidate whose original-base
range excludes that commit while preserving TASK-001 and remediation content.
How the caller produces that history remains caller-owned; review does not
rewrite it. This is a bounded repository-compliance correction, not a product
or specification decision.

Originally retained as `FIND-TASK-001-16`.

After review, the user explicitly approved retaining this unrelated commit and
its existing attribution metadata in the TASK-001 candidate. Current user
instruction is the highest workflow authority, so no history rewrite or source
revert is required. `FIND-TASK-001-16` is **RESOLVED / NOT NEEDED** and is not
part of remediation.

### `RS3-2` — CONFIRMED

The committed R2 evidence records the registration-route proof, the grouped
restart proofs, and the Oracle proof with raw `cargo nextest run`. The broad
`mise run` lanes and repeated `--retries 0` outcomes are credible behavioral
evidence, but they do not satisfy the explicit repository rule that every
specifically named Rust test in an implementation report be run and recorded
through `mise exec -- cargo nextest run` with its exact selector and required
environment owner. The same packet correctly uses `mise exec --` for the
composition and OpenAPI named tests, so this is not an ambiguity in the
record.

The smallest correction is evidence-only: rerun the existing route, grouped
restart, and Oracle commands through their current Postgres/migration wrappers,
replacing only raw `cargo nextest` with `mise exec -- cargo nextest`, retain the
exact package, target, feature, selector, ignored/profile, and `--retries 0`
arguments, and append the exact results. No implementation or broad lane needs
to change or rerun unless source changes.

Retained as `FIND-TASK-001-17`.

## Independently validated prior-finding closure

| Prior finding | Validation | Result |
|---|---|---|
| `FIND-TASK-001-2` | The combined Postgres-backed refusal test sends a Workflow with a binding-bearing inline Agent through authenticated HTTP, asserts `400` and `WYRD_REGISTRY_400_INVALID_CARD_SPEC`, and calls the existing no-registration-writes assertion. Sharing the already-started server with the four registry-dependent refusals preserves the exact seam and avoids a duplicate harness. | CLOSED |
| `FIND-TASK-001-4` | Referenced Verifiers and Operators use the existing `CardRef::identity_key()` owner; equal inline Operators use typed `OperatorSpec` equality. The three new cases cover UID presentation and inline equality, and the server refusal asserts `WYRD_PERMISSION_403_DENIED_RBAC` plus no writes. The small linear inline scan is justified for one binding list and carries its explicit ceiling. | CLOSED |
| `FIND-TASK-001-5` | The four files named by R2 were corrected, but live CLI, Eval-engine, permission, and Source public wording still advertises Drift/Eval Cards. | OPEN — revised residual finding retained |
| `FIND-TASK-001-11` | The pointer-identity type and its only two callers are deleted. Both restart tests exercise replacement application and Vala pools with real queries while retaining stopped-node, survivor, WAL, plan, resource, role, and credential assertions. | CLOSED for implementation; command-form evidence is separately covered by `FIND-TASK-001-17` |
| `FIND-TASK-001-12` | The Oracle journey re-reads the existing inspection under a finite 300 × 100 ms bound, preserves all six exact zero conditions, and reports the final snapshot on timeout. | CLOSED for implementation; command-form evidence is separately covered by `FIND-TASK-001-17` |
| `FIND-TASK-001-13` | The R2 diff documents the ledger-named panicking tests/helpers and every new R2 test/helper inspected, including duplicate cases, the OpenAPI walker/test, registration/ingest cases, restart tests, and settlement constants. No waiver or new documentation checker was added. | CLOSED |
| `FIND-TASK-001-14` | The existing OpenAPI owner registers the three missing roots; `VerifierImplementation::schemas` forwards `DriftSpec`'s transitive dependencies; and the focused test walks every `$ref` reachable from the five Verifier/binding/Trigger roots. `DriftSpec` is a source-owned, defined but unreferenced byproduct of the existing `ToSchema` owner, not a second contract or reachable failure. Removing it is unrelated cleanup. | CLOSED |

The Wave 1 task review's empty ledger is valid for functional behavior but not
for total acceptance because it missed the retained documentation and
repository-compliance findings above. The security/tenancy review's empty
ledger is independently supported: caller identity, tenant-scoped resolution,
canonical duplicate identity, stable denial, and no-write behavior remain
closed. No additional security, tenancy, durability, concurrency, schema, or
complexity finding was validated.

## Final validated finding ledger

### `FIND-TASK-001-5` — REVISED — DRIFT — retired Drift/Eval Card wording remains live

- **Wave 1 source IDs:** `CR3-C1`; residual of prior
  `FIND-TASK-001-5`.
- **Violated obligation:** REQ-109, REQ-114, AC-021, and TASK-001 Scenarios 3
  and 4 require CLI and public documentation to describe one registrable
  Verifier model, with Drift and Eval only as implementations.
- **Exact location:**
  - `crates/wyrd/wyrd-cli/src/eval/run.rs:77,83,110,118`
  - `crates/vala/vala-eval/src/lib.rs:1`
  - `crates/vala/vala-eval/src/orchestrator/state.rs:121`
  - `crates/vala/vala-eval/src/results.rs:79`
  - `crates/shared/wyrd-runtime/src/permission.rs:79,81`
  - `crates/wyrd-spec/src/card/source.rs:5`
- **Evidence:** the CLI function accepts and returns only the Verifier kind;
  the live engine, permission, and Source owners still say “Eval card,”
  “Drift cards,” or “Drift/Eval Card.” These public descriptions contradict
  the code and the approved catalog.
- **Observable consequence:** a caller following CLI diagnostics or generated
  Rust documentation is told to reason about a registrable Drift/Eval Card
  that the same candidate rejects.
- **Decision-complete correction:** replace only the stale descriptions with
  precise “eval-backed Verifier Card,” “drift-backed Verifier Card,” or
  implementation-neutral Verifier wording. Reuse the existing types and
  loaders. Preserve stable error variants/codes, `load_eval_card`, `eval_ref`,
  `Resource::{Evals, Drift}`, Eval execution semantics, and TASK-002-owned
  observation-record fields/docs. Do not rename APIs, add aliases, or sweep
  private test messages.
- **Focused closure proof:** scoped case-insensitive searches over the listed
  live CLI/runtime/contract files show no remaining registrable Drift/Eval Card
  claim; run the existing CLI unit/journey owner, `mise run docs:check`,
  `mise run fmt`, and `mise run lints`.

### `FIND-TASK-001-15` — REVISED — VIOLATION — retired-table owner still promises eight built-ins

- **Wave 1 source IDs:** `DDR3R1-1`.
- **Violated obligation:** REQ-116 and AC-022 retire two canonical built-ins;
  `AGENTS.md` §16 requires materially modified Rust documentation to describe
  the actual owner and invariant.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:751,861`.
- **Evidence:** the same cumulative diff changes the canonical registry and
  return type from eight to six, while `builtin_fqns` and the physical-layout
  test still say eight. Both execute over `BUILTIN_TABLES`, which now contains
  six definitions.
- **Observable consequence:** maintainers cannot tell from the canonical table
  owner whether the two retired Eval tables still belong to the supported
  built-in/layout contract.
- **Decision-complete correction:** change only both stale numerals from
  “eight” to “six.” Add no helper, test, constant, or generalized count-aware
  prose machinery.
- **Focused closure proof:** a scoped search finds no “eight built-in” or “all
  eight” claim in the owner; run `mise run fmt` and `mise run lints`. Existing
  registry tests remain the behavioral proof and need no new case for a
  documentation-only correction.

### `FIND-TASK-001-16` — RESOLVED / NOT NEEDED — explicitly accepted candidate history

- **Wave 1 source IDs:** `RS3-1`.
- **Violated obligation:** `AGENTS.md` §13 forbids AI co-author trailers;
  TASK-001 review requires the cumulative candidate to contain no unrelated
  change.
- **Exact location:** commit
  `5f14f3c325cc8c45081fe9ad0543e7794b4a4e0f`; `AGENTS.md:515-527` in the
  base-to-candidate diff.
- **Evidence:** the commit's only source change adds a repository process rule,
  its body contains `Co-Authored-By: Claude Opus 5 ...` and `Claude-Session`,
  and neither is required by TASK-001. The no-AI-trailer rule is present in
  base `5293546f3`.
- **Observable consequence:** the reviewed task range violates contributor
  policy and cannot establish that every cumulative source/commit change
  belongs to TASK-001 or its verification.
- **Resolution:** the user explicitly approved retaining `5f14f3c32`, its
  `AGENTS.md` change, and its existing attribution metadata. No correction or
  closure command is required, and the candidate history must not be rewritten
  for this finding.

### `FIND-TASK-001-17` — CONFIRMED — VIOLATION — named proofs bypass the pinned `mise exec` form

- **Wave 1 source IDs:** `RS3-2`.
- **Violated obligation:** `AGENTS.md` §11, the specification-development
  reference's test-command precision rule, and TASK-001-R2 require every named
  Rust test to use an exact `mise exec -- cargo nextest run --locked` command
  with its environment owner.
- **Exact location:**
  `changes/active/verified-change-contract/review/TASK-001-r2/TASK-001-R2-verifier-contract-closure.md:219,222-223`.
- **Evidence:** those entries record raw `cargo nextest` for
  `referenced_binding_refusals_leave_no_writes`, the two named restart tests,
  and
  `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`.
  The broad `mise run` lanes do not replace the explicitly required named-test
  form.
- **Observable consequence:** the immutable evidence does not prove those
  exact focused executions used the repository-pinned Cargo/nextest toolchain,
  despite recording the criteria as passed.
- **Decision-complete correction:** rerun only those three proof groups through
  their existing repository-managed Postgres wrappers: retain
  `db:migrate:all:inner` and `WYRD_REG_E2E=1` for the registration route,
  retain the restart tests' required `wyrd-server/test-support` feature and
  exact two-test expression, and retain `db:migrate:inner`, `-P journey`, and
  `--run-ignored=all` for Oracle. In each command replace raw Cargo with
  `mise exec -- cargo nextest run --locked`; repeat the restart and Oracle
  groups three times with `--retries 0`, and append the exact commands/results
  to the R2 evidence. Do not rerun broad lanes unless source changes.
- **Focused closure proof:** the committed packet contains each exact
  `mise exec --` command and passing result; a scoped search finds no raw
  `cargo nextest` invocation for these named R2 proofs.

## Verification limits

- This validation was read-only with respect to the immutable source. It did
  not rerun the expensive Postgres, restart, Oracle, or broad repository lanes.
  Their recorded outcomes were inspected separately from command-form
  compliance.
- The focused binding and OpenAPI implementations and their tests were read in
  full. The OpenAPI test's traversal is sufficient for the candidate-owned
  roots; unrelated historical roots are outside TASK-001.
- Untracked review directories are orchestration artifacts outside the
  immutable candidate and do not change its source identity.

## Recommendation

**FIX_REQUIRED**

Retained finding IDs: `FIND-TASK-001-5`, `FIND-TASK-001-15`,
`FIND-TASK-001-17`.

Resolved / not needed: `FIND-TASK-001-16` by explicit user approval.

No retained correction changes approved product behavior, public API,
security policy, compatibility, concurrency semantics, resource ownership, or
persistent-data design. `SPEC_REVISION_REQUIRED` is not warranted. The subject
and required independent reports were available and remained immutable, so the
review is not `BLOCKED`.
