# TASK-002 R2 Task Implementation Review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Detached review worktree: `/tmp/wyrd-task-002-r2-review-f345cd8a4`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior reviews: `task-002-f66a33769-review-01` and `task-002-r1-4d9d74b34-review-01`
- R2 remediation: `task-002-r1-4d9d74b34-review-01/TASK-002-R2-close-r1-review-findings.md`

The complete cumulative `861f8d86c..f345cd8a4` range was reviewed. The detached
candidate remained clean and fixed at `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
throughout the review.

## Review Findings

### Critical

None.

### Important

#### TASK-R2-001 — DRIFT: unrelated server-auth, tenant-identity, Forge, and Scribe changes entered the cumulative task candidate

- **Violated obligation:** TASK-002 limits its server work to the public Bifrost
  contract/consumer seams, and TASK-002-R2 explicitly excludes persistent
  state, server authz/audit behavior, lifecycle concurrency, and cleanup outside
  the validated client modules and two documentation snippets. A task-review
  `PASS` also requires that no unrelated change enter the base-to-candidate
  range.
- **Exact location:** commits `7fc756f09` (`crates/vala/vala-bifrost-redux/src/scribe/wal.rs:956-1022`),
  `5cfe7b6b9` (`crates/wyrd-spec/src/ids.rs:147` plus two SQL migrations and
  Bifrost catalog/Scribe/auth consumers), `3e8b6efb3`
  (`crates/vala/vala-bifrost-redux/src/forge/worker.rs:815,1261-1269,1644`),
  `afdc3b798` (`crates/shared/wyrd-auth-oidc/src/config.rs:48` and
  `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:118`), and `5d21c9dd5`
  (auth TLS test-client construction and dependency changes). These commits are
  ancestors of `f345cd8a4` after the prior reviewed TASK-002 candidate
  `4d9d74b34` and before the four R2 implementation commits.
- **Evidence:** the commits change WAL recovery, the durable system-tenant UUID
  and migrations, Forge test-support coordination, production trusted-issuer
  resolution, and auth test TLS setup. None closes
  `FIND-TASK-002-1`, `-3`, `-7`, `-8`, or `-14`; none is named in the R2
  implementation evidence; and the R2 non-goals expressly exclude these
  boundaries.
- **Observable consequence:** the immutable cumulative subject no longer proves
  TASK-002 in isolation. Acceptance would silently approve security,
  persistent-data, audit/WAL, and concurrency changes that this task neither
  authorizes nor supplies its required domain-specific acceptance evidence for.
- **Required testable correction:** provide a new immutable TASK-002 candidate
  whose `861f8d86c..candidate` range contains the cumulative TASK-002/R1/R2 work
  but excludes these separately owned commits. Do not delete their work from
  the integration branch; review them under their owning tasks. Prove the new
  range with `git log` and `git diff --name-status` before rerunning the task
  review.

#### TASK-R2-002 — VIOLATION: the Python SDK adds an ignored per-crate release profile

- **Violated obligation:** AGENTS.md section 4 says, “Do not add ... per-crate
  profile blocks.” TASK-002-R2 also prohibits new configuration outside its
  five bounded corrections.
- **Exact location:** `sdks/wyrd-sdk-python/Cargo.toml:67-71`, introduced by
  commit `35ba64fe3` between the prior candidate and R2.
- **Evidence:** the member manifest adds `[profile.release]` with LTO,
  codegen-unit, stripping, and debug settings. Cargo reports that profiles for
  this non-root package are ignored and instructs that profiles belong at the
  workspace root. The original task and all five R2 findings require no release
  profile.
- **Observable consequence:** the candidate violates an explicit repository
  rule while creating dead configuration that does not affect the Python
  extension build as claimed.
- **Required testable correction:** delete this five-line member profile. Add no
  workspace profile unless a separately approved repository-wide requirement
  calls for one. Confirm Cargo no longer emits the ignored-profile warning.

#### TASK-R2-003 — VIOLATION: the cumulative candidate fails its required whitespace check

- **Violated obligation:** TASK-002 and TASK-002-R2 both require
  `git diff --check`; task review must inspect the complete base-to-candidate
  range rather than only the clean worktree.
- **Exact location:**
  `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/findings-validation.md:183`
  and
  `changes/active/surfaces-oracle-integration/review/task-002-r1-4d9d74b34-review-01/verdict.md:88`.
- **Evidence:** `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
  reports `new blank line at EOF` at both locations. A clean `git status` or an
  unqualified `git diff --check` only checks uncommitted changes and therefore
  cannot prove the immutable cumulative candidate.
- **Observable consequence:** the recorded verification table claims a pass for
  a check that fails on the actual reviewed range, so the completion evidence is
  not credible and the task's explicit verification obligation remains open.
- **Required testable correction:** remove only the two terminal blank lines and
  rerun the exact base-to-candidate `git diff --check` command on the new
  immutable candidate.

### Suggestions

None. Optional improvements and unrelated pre-existing debt were excluded.

## Open Questions

None. The TypeScript empty-`serverUrl` behavior recorded as a limit does not
violate the approved task: it still produces the required structured runtime
`WyrdError`, while R2 requires preserving details for errors that actually
reach the canonical projector rather than introducing new validation behavior.

## Acceptance Matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-004–REQ-009, INV-001: preserve Card registration/loading, references, `CardRef`, Eval/Drift, catalog, Audit Card, and `WyrdState` behavior | Cumulative Card, storage, hydration, saga, and state ownership remains under `wyrd-client`; contract shapes remain in `wyrd-spec` | Previously recorded Rust/Python/TypeScript Card, CLI, HTTP, MCP, and state journeys | PASS |
| REQ-016–REQ-018, REQ-040, REQ-041, INV-004, INV-013, INV-024: one shared implementation and three thin SDK roots | `wyrd-client` remains the sole shared owner; Rust is a thin re-export; Python/TypeScript retain foreign-runtime projections | Recorded client-tier, PyO3-scope, builds, typechecks, dependency trees, and language journeys | PASS |
| REQ-019, REQ-061, INV-005: `Bifrost` is the sole public table/query/write/lifecycle facade | Public facade and SDK/CLI consumers remain converged; sibling query owner stays private | Prior focused facade and language lifecycle evidence remains applicable | PASS |
| REQ-019A: one explicit shared credential field | Shared `ClientConfig::credential` and SDK constructor projections remain unchanged by R2 | Existing client and language tests | PASS |
| REQ-020, REQ-021, AC-004: Python sync/async and TypeScript journeys preserve authoring, conversion, Arrow, lifecycle, description, and errors | Public packages remain rooted under `sdks/`; R2 changes only error assertions and current docs examples | Recorded Python and TypeScript unit/build/typecheck results and prior integration journeys | PASS |
| REQ-056: stream framing, terminal, settlement, bounded ownership, and shutdown/drain behavior | Prior R1 corrections remain in the shared query/facade owner | Prior focused stream/shutdown tests and language journeys | PASS |
| REQ-057, REQ-058, AC-019: exact MCP catalog and preserved CLI query behavior | MCP/CLI continue through the shared facade | Previously recorded exact MCP discovery and CLI journeys | PASS |
| REQ-022–REQ-025: stable public error identity and language projections | `From<&WyrdClientError>` now emits only `{field, reason}`, `{transport}`, or `{}` and both foreign boundaries reuse the existing projector | Recorded focused Rust test plus Python config-details and TypeScript transport-details tests | PASS |
| FIND-TASK-002-1 closure | `crates/shared/wyrd-client/src/error.rs` preserves safe per-variant details without exposing raw transport failure text | Recorded exact Rust proof, two Python assertions, and TypeScript assertion | PASS |
| FIND-TASK-002-3 closure, REQ-059, AC-002 | All 40 non-success/default response annotations across seven operations declare `application/problem+json`; generated OpenAPI agrees | Recorded exact OpenAPI test and `codegen:check`; source test also rejects parallel `application/json` | PASS |
| FIND-TASK-002-7 closure | Commit `c0a12085c` adds intent-level rustdoc throughout the previously identified relocated client/test items | Recorded cumulative added-item scan reports zero undocumented items; recorded lints pass | PASS |
| FIND-TASK-002-8 closure | `RegistrationReceipt`, `GetCardResponse`, and `UploadCompleteRequest` are module-top imports and used as bare names | Source search has zero prohibited qualified-path hits; recorded lints pass | PASS |
| FIND-TASK-002-14 closure, INV-005, INV-010 | Reading guide uses Python `AsyncBifrost` and TypeScript `Bifrost.connect`; public docs contain no `BifrostQueryClient` | Recorded docs check and direct Python/TypeScript snippet typechecks | PASS |
| REQ-043, REQ-044, INV-010, INV-019: retired aliases, bootstrap, audit verification, and typed observation reads remain absent | No R2 implementation commit restores these surfaces | Source/diff inspection and prior static checks | PASS |
| INV-006, INV-013, AC-010: client-tier/PyO3 boundaries remain valid | Dependency ownership is unchanged by the five R2 corrections | Recorded `check:client-tier` and `check:pyo3-scope` | PASS |
| Five Postgres lanes retain the canonical isolated lifecycle | Existing `mise.toml` outer/inner ownership remains in the candidate | Previously recorded five lane and inventory passes | PASS |
| TASK-002-R2 non-goal: no persistent-state, server authz/audit, lifecycle-concurrency, or out-of-scope cleanup | Separate system-tenant/migration, WAL recovery, Forge coordination, issuer-resolution, and auth TLS commits occur between R1 and R2 | Complete commit and diff inspection | **FAIL — TASK-R2-001** |
| Repository constraint: no per-crate profile blocks and no speculative configuration | Python SDK member manifest contains an ignored release profile unrelated to the task | Cargo emitted the ignored-profile warning during the attempted focused rerun | **FAIL — TASK-R2-002** |
| Required cumulative formatting/whitespace proof | Two prior review artifacts end with a newly added blank line | Exact cumulative `git diff --check` fails at both files | **FAIL — TASK-R2-003** |
| Other non-goals: no compatibility alias, second client/projector/transport, new feature/dependency for R2, UI work, merge/push/deploy, or Bifrost aggregate | The four listed R2 implementation commits add none of these | Complete R2 diff inspection and recorded verification | PASS |

## Prior-Finding Closure

| Finding | Result |
|---|---|
| `FIND-TASK-002-1` | CLOSED |
| `FIND-TASK-002-3` | CLOSED |
| `FIND-TASK-002-7` | CLOSED |
| `FIND-TASK-002-8` | CLOSED |
| `FIND-TASK-002-14` | CLOSED |
| Previously closed `FIND-TASK-002-2`, `-4`, `-5`, `-6`, `-9`, `-10`, `-11`, `-12`, `-13` | Remain closed in source; no TASK-002-owned R2 implementation regresses them |

## Verification Notes

- Reviewed the complete cumulative diff, commit list, current source, public
  language projections, route annotations, generated OpenAPI, docs snippets,
  prior verdicts/findings, and R2 evidence.
- The recorded R2 passes are credible for the five bounded corrections, except
  that the recorded unqualified `git diff --check` does not prove the cumulative
  candidate and fails when run with the required commit range.
- An independent rerun of the focused `wyrd-client` test was attempted, but the
  detached worktree exhausted local disk while compiling and the test did not
  execute. Its temporary `target` output was removed with `cargo clean`; no
  source changed. This environmental failure is a verification limit, not a
  behavior finding, because the exact test has recorded passing evidence and
  its assertions/source were independently inspected.
- The task-prohibited Bifrost aggregate was not run.

## Overall Result

**FAIL**

All five R2 remediation findings are closed, but the cumulative candidate is
not an acceptable isolated TASK-002 result: unrelated sensitive-domain work is
inside the reviewed range, an ignored per-crate profile violates repository
rules, and the exact cumulative whitespace check fails.
