---
id: TASK-001-R3
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 32
parent_task: TASK-001
remediates:
  - FIND-TASK-001-5
  - FIND-TASK-001-15
  - FIND-TASK-001-17
---

# TASK-001-R3 — Final acceptance closure

## Authority and subject

- Approved spec: `changes/active/verified-change-contract/spec.md`, revision 32
- Original task: `changes/active/verified-change-contract/tasks/TASK-001-verifier-contract-and-registration.md`
- Original base: `5293546f33b3a5fd9de529098e23ea70d472c412`
- Reviewed candidate: `9d7b6266206f15136f306b66c06571066fc6bd13`
- Validated evidence: this directory's `findings-validation.md` and `verdict.md`

Use `$wyrd-implement` for the source and evidence corrections.

## Intended outcome

TASK-001 exposes one Verifier vocabulary everywhere it owns, its canonical
Bifrost registry documentation matches the six supported built-ins, and every
named proof is recorded through the repository-pinned toolchain.

## Diagnoses and required corrections

### `FIND-TASK-001-5` — residual retired Card vocabulary

The CLI accepts only an eval-backed Verifier and the public Eval engine uses
Verifier references, but several live diagnostics and public docs still say
“Eval card,” “Drift cards,” or “Drift/Eval Card.” A caller following those
surfaces is directed toward Card kinds the same candidate rejects.

Correct only the validated public descriptions in the CLI Eval loader, Vala
Eval crate/state/result docs, permission resource docs, and `SourceSpec` module
docs. Use precise eval-backed/drift-backed Verifier wording. Preserve stable
errors and resource variants, existing API/type/field names such as
`load_eval_card` and `eval_ref`, engine behavior, and TASK-002-owned observation
documentation. Do not add aliases, rename APIs, or sweep private test strings.

### `FIND-TASK-001-15` — stale built-in count

TASK-001 correctly removed two Eval built-ins, leaving six, but two rustdocs in
the canonical Bifrost table owner still say eight. This makes the retired-table
contract ambiguous despite correct runtime behavior.

Change only the two validated count descriptions from eight to six. The
existing registry and tests already own the behavior; add no helper, count
abstraction, or new test.

### `FIND-TASK-001-17` — named proof command form

The registration route, grouped restart tests, and Oracle journey have credible
passing outcomes, but their committed focused evidence uses raw Cargo rather
than the mandatory repository-pinned `mise exec -- cargo nextest run --locked`
form. Broad `mise run` lanes do not replace this explicit requirement.

Rerun only those three proof groups through their existing repository-managed
Postgres/migration wrappers, replacing the raw Cargo invocation with
`mise exec -- cargo nextest run --locked` while retaining their exact packages,
targets, features, selectors, profiles/ignored flags, environment setup, and
`--retries 0`. Repeat the restart and Oracle groups three times as previously
required, and append the exact commands and results to the existing R2 evidence.
Do not rerun broad lanes unless the documentation source corrections make an
owning lane necessary or another failure is encountered.

## Preserved behavior and constraints

- Preserve approved specification revision 32 and every closed finding.
- Preserve registration behavior, stable errors, tenant isolation, no-write
  refusals, canonical identity, table retirement, OpenAPI closure, restart
  lifecycle assertions, and the Oracle six-field zero invariant.
- Do not change public API names, resource variants, Card kinds, schema shapes,
  persistence, concurrency semantics, dependencies, or test harnesses.
- Do not hand-edit generated artifacts or accept a failed test by retry.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-5` | The listed live CLI/runtime/contract docs describe Drift and Eval only as Verifier implementations, without API or behavior changes. |
| `FIND-TASK-001-15` | The canonical six-entry registry is documented as six in both validated locations. |
| `FIND-TASK-001-17` | Every named R2 proof group is recorded passing through its exact `mise exec -- cargo nextest run --locked` command and existing environment owner. |

## Focused and broader proof

Prove documentation closure with scoped searches over the validated files,
then run:

```bash
mise run fmt
mise run lints
mise run docs:check
git diff --check
```

Run the registration, restart, and Oracle named tests with the exact pinned
commands required above. Record every command and result; do not substitute a
positional filter or a broad lane. If any source beyond documentation changes,
run its narrowest owning `mise` lane.

## Non-goals

- Additional product, SDK, runtime, Bifrost, OpenAPI, or registration work.
- Removing the harmless unreferenced `DriftSpec` OpenAPI component.
- Renaming retained public APIs or permission resources.
- Reworking tests, fixtures, or CI beyond the exact proof-form correction.
- Changing repository-wide process policy as part of TASK-001.

## Implementation evidence

Commits `d01158ad0..fea2021da` on `verified-change-contract`, applied on top of
the reviewed candidate `9d7b62662`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-001-5` | `crates/wyrd/wyrd-cli/src/eval/run.rs` — the four parse/metadata diagnostics now name an "eval-backed Verifier card"; `crates/vala/vala-eval/src/lib.rs`, `crates/vala/vala-eval/src/orchestrator/state.rs`, `crates/vala/vala-eval/src/results.rs` — the engine, `RunState.eval_ref`, and `RunIdentity.eval_ref` docs name an eval-backed Verifier Card; `crates/shared/wyrd-runtime/src/permission.rs` — `Resource::Evals`/`Resource::Drift` docs name eval-backed/drift-backed Verifier Cards; `crates/wyrd-spec/src/card/source.rs` — the module doc names a consuming Verifier Card. Description text only: no error variant, code, API name (`load_eval_card`, `eval_ref`), resource variant, or engine behavior changed, and no TASK-002-owned observation doc was touched. | `grep -nic 'eval card\|drift card\|drift/eval card'` over the six files → 0 each. `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(eval_local_records::)' --retries 0` → 2 passed. `mise run fmt`, `mise run lints`, `mise run docs:check`, `mise run codegen:check` → clean. | PASS |
| `FIND-TASK-001-15` | `crates/vala/vala-bifrost-redux/src/tables/mod.rs:751,861` — `builtin_fqns` and the physical-layout contract test now say six, matching `BUILTIN_TABLES: [BuiltinTableDefinition; 6]`. No helper, constant, or test added. | `grep -ni 'eight' crates/vala/vala-bifrost-redux/src/tables/mod.rs` → empty. `mise run fmt`, `mise run lints` → clean. | PASS |
| `FIND-TASK-001-17` | Evidence-only: the three named R2 proof groups were rerun unchanged except for the pinned command form, and the R2 record's three cells plus a new addendum carry the exact `mise exec -- cargo nextest run --locked` commands. | Registration route → 1 passed. Restart pair ×3 → 2 passed each run, no retry. Oracle journey ×3 → 1 passed each run, no retry. Exact commands are recorded in `review/TASK-001-r2/TASK-001-R2-verifier-contract-closure.md`; a scoped search finds no raw `cargo nextest` invocation for these proofs. | PASS |

### Broader verification

| Command | Result |
|---|---|
| `mise run fmt` | clean |
| `mise run lints` | clean, `-D warnings` at `--all-features --all-targets` |
| `mise run docs:check` | 60 pages, contrast AA verified |
| `mise run codegen:check` | All checks passed |
| `git diff --check` | clean |

`codegen:check` was run because `Resource` and `SourceSpec` documentation
changed; it confirms no generated OpenAPI, schema, MCP, or `.pyi` drift. No
broad test lane was rerun: the source change is documentation-only and the
three named proof groups were rerun directly.

### Non-goals and scope

No product, SDK, runtime, Bifrost, OpenAPI, or registration behavior changed.
The unreferenced `DriftSpec` OpenAPI component remains. No public API,
permission resource, test, fixture, or CI definition was renamed or reworked.
No repository-wide process policy was changed. Git identity is unchanged and
no commit carries an AI co-author or session trailer.
