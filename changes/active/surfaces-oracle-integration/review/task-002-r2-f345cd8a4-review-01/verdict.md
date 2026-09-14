# TASK-002-R2 Review Verdict

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Review worktree: `/tmp/wyrd-task-002-r2-review-f345cd8a4`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior reviews: `task-002-f66a33769-review-01` and `task-002-r1-4d9d74b34-review-01`
- R2 remediation: `task-002-r1-4d9d74b34-review-01/TASK-002-R2-close-r1-review-findings.md`

The complete cumulative base-to-candidate range was reviewed. The detached
candidate remained clean and fixed at the candidate commit throughout both
waves. Current uncommitted Forge changes were excluded.

Current user authority permits every commit on the branch as TASK-002 candidate
content and permits the package-local Python SDK release profile. No finding is
retained merely because a commit concerns another domain or because that
profile exists.

## Acceptance matrix

| Obligation | Evidence | Result |
|---|---|---|
| Shared client and first-class SDK convergence; one public `Bifrost` facade | Cumulative source, public exports, generated surfaces, and recorded language journeys | PASS |
| Stable client error identities and safe `{field, reason}`, `{transport}`, and `{}` details | One shared projector plus Rust, Python, and TypeScript assertions | PASS — closes `FIND-TASK-002-1` |
| Seven Bifrost OpenAPI refusal bodies and media types match runtime | Route annotations, generated OpenAPI, focused source test, and codegen evidence | PASS — closes `FIND-TASK-002-3` |
| Canonical reading guide uses current Python and TypeScript facades | Public docs use `AsyncBifrost` and `Bifrost.connect`; snippet typing and docs evidence | PASS — closes `FIND-TASK-002-14` |
| Every materially relocated fallible/cancellable Rust workflow has required rustdoc | Several live workflows still lack mandatory `# Errors` and applicable cancellation/partial-progress contracts | FAIL — `FIND-TASK-002-7` |
| Cumulative changed Rust uses module-top imports and bare signature/field types | Qualified signatures and one function-scoped import remain | FAIL — `FIND-TASK-002-8` |
| Required cumulative diff hygiene proof | Exact base-to-candidate `git diff --check` reports two blank lines at EOF | FAIL — `FIND-TASK-002-15` |
| Shutdown closes admission and waits for every admitted direct Arrow write | Direct writes are not joined to the producer drain and can outlive successful shutdown | FAIL — `FIND-TASK-002-16` |
| Query stream settlement is terminal and at most once after a failed healthy drain | Drain failure overwrites `Settled` with `Broken`, allowing a second cancellation/poll cycle | FAIL — `FIND-TASK-002-17` |
| Candidate commit metadata follows contributor-attribution policy | Twenty-two commits carry prohibited AI `Co-Authored-By` trailers | FAIL — `FIND-TASK-002-18` |
| Security, tenancy, audit, migration, and persistent-data behavior | Full domain traces; greenfield migration authority resolves migration objections | PASS |
| TypeScript empty explicit URL behavior | Bifrost projections share the same transport-down behavior and structured details; no approved cross-capability classification obligation requires Cards' config result | PASS / not a finding |
| User-authorized cumulative scope and package profile | Explicit current user authority | PASS |

## Review-wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | Scope drift, profile, cumulative diff hygiene |
| Repository standards | FAIL | Rustdoc, imports, profile, diff hygiene, commit trailers |
| Public SDK/contracts | PASS | None |
| Security/RBAC/tenancy | FAIL | Migration/system identity and OIDC scope proposals |
| Stream lifecycle/durability | FAIL | Direct-write shutdown and settlement re-entry |
| Persistent data/audit | FAIL | System identity/WAL and Forge scope proposals |
| Structured Ponytail validation | FIX_REQUIRED | Six retained findings |

User authority and approved greenfield migration requirements caused the scope,
profile, migration, OIDC, and Forge proposals to be rejected. Exact
dispositions are preserved in `findings-validation.md`.

## Validated finding ledger

| Stable ID | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-002-7` | REVISED / REOPENED | VIOLATION | Add missing `# Errors` and applicable cancellation/partial-progress rustdoc to the validated live workflows. |
| `FIND-TASK-002-8` | REVISED / REOPENED | VIOLATION | Replace remaining qualified governed types with module-top imports and move the WAL test import to module scope. |
| `FIND-TASK-002-15` | CONFIRMED | VIOLATION | Remove two extra terminal blank lines and prove the exact cumulative diff is clean. |
| `FIND-TASK-002-16` | CONFIRMED | INCORRECT | Make `WriterPool` refuse later direct sends and wait for every admitted direct send before shutdown succeeds. |
| `FIND-TASK-002-17` | CONFIRMED | INCORRECT | Keep `QueryResultStream` terminally settled after a completed drain-failure/status cycle so cancellation and polling do not repeat. |
| `FIND-TASK-002-18` | CONFIRMED | VIOLATION | Establish an equivalent immutable candidate without AI co-author trailers, after explicit caller authorization for the history operation. |

## Prior-finding closure

| Finding | Result |
|---|---|
| `FIND-TASK-002-1`, `FIND-TASK-002-3`, `FIND-TASK-002-14` | CLOSED |
| `FIND-TASK-002-7`, `FIND-TASK-002-8` | REOPENED |
| `FIND-TASK-002-2`, `-4`, `-5`, `-6`, `-9`, `-10`, `-11`, `-12`, `-13` | Remain closed |
| `FIND-TASK-002-15`, `-16`, `-17`, `-18` | NEW |

## Verification limits

- Wave 2 independently established the retained findings from source, full
  function bodies, caller paths, history, authorities, and the cumulative diff.
- The recorded Rust, Python, TypeScript, codegen, docs, lint, and boundary
  passes were inspected but not rerun in Wave 2. They do not exercise the two
  newly identified lifecycle interleavings.
- Exact cumulative `git diff --check` fails at the two retained locations.
- A Wave 1 focused rerun did not execute after temporary build output exhausted
  disk space; recorded focused evidence and source inspection remain available.
- The Bifrost aggregate remains prohibited and was not run.
- Rewriting commit identities is not authorized by this review; remediation of
  `FIND-TASK-002-18` requires explicit caller authorization before execution.

## Verdict

**FIX_REQUIRED**

Five source corrections are bounded within existing owners. The commit-metadata
correction is also required, but review performs no history-changing operation.
