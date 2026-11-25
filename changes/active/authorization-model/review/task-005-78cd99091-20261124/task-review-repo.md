# TASK-005 repository and merge-seam review

Subject: base `f7e0d67b2e007a1262fed34d45e5ebd2629e9bee`, candidate `78cd99091c81e5a7d706ac6f54a8222e62705e0a`. HEAD remained the candidate. Reports only were written. Approved authority is authorization-model revision 12, REQ-021/AC-016, its two revision-12 Persistence rows, and TASK-005. Root/nested AGENTS, spec-driven development, maintainer style, agent surfaces, TESTING, and relevant identity/browser/access architecture apply. No `.codegraph/` exists.

## Boundary and navigation

The task's first-parent commits through `cc5cea627` own the console redesign, provider display names, principal read contracts, SDK/MCP/CLI projections, and connected tests. Merge `8cc9b03d3` brings PR 244: separate authorization carryover remediation, Bifrost write-pipeline and benchmark changes, setup/bootstrap changes, and corresponding evidence. Later `b7b63ea14` adopts the verification-workflow branch's development environment; `34130f505` adjusts one paused-clock capacity test. Candidate task evidence records the merged state.

Principal projection and merged provider-helper behavioral coverage belongs to the domain reviewer; UI workflows to the primary reviewer; mock alignment to the UI reviewer. This report covers repository/gate obligations, storage authorization, hot-path scope, and ancillary development/test changes. It does not accept PR 244's independent Bifrost task: its existing `TASK-003-R1-review-2026-11-23/verdict.md` expressly accepts code remediation while retaining final performance qualification as pending.

## Source-backed checks

| Boundary | Evidence and result |
|---|---|
| Migration authorization | REQ-021 and spec lines 1018–1019 expressly authorize editing unshipped `20260601000001_auth.sql` in place. Its actual delta adds only nullable `auth_users.display_name` and `auth_roles.description` plus explanatory comments. No new table/index/migration or RLS change. The handoff's “no migration” means no new migration, not no migration-file edit. PASS. |
| Authority reconciliation | `f1efb9a5f` changes the `/me` paragraph before `1a4fefbad` implements projections. Candidate `architecture/wyrd-design.md:1040` permits the caller's own email while excluding another principal's. PASS. |
| Public projection ownership | Wire fields originate in `wyrd-spec`; generated TS `card-types.ts` and exported `PrincipalStatusCounts` reflect them. Python uses its existing typed serde wrapper/stubs. No task-owned Cargo dependency change; incoming workspace-hack/lock changes belong to PR 244. Actual served-schema assertions need domain review: a green suite name does not establish retained assertions after merge. |
| Development environment | `mise.toml:1334` starts the server and obtains fresh credentials before starting the UI, preserves process-group cleanup, and uses bash-3-compatible polling. `scripts/postgres/dev-server.sh:30` uses setup's own human admin with a valid 17-character password, rejects empty credentials, and keeps credential-file mode 0600. `scripts/dev/example-app.sh:23` pins `3222cf1359f4a2e43081127e6d8b1089eaeebd67`, with existing dirty-checkout protection and installed-wheel digest verification using portable `shasum`. `ui-pages.integration.test.ts:2272` onward tests failed UI cleanup, setup refusal, real password sign-in, ready-before-batch, retained observation outputs and subsequent batches. Implementation reports the connected lifecycle journey passed after merge. No concrete regression found. |
| Paused-clock repair | `34130f505` changes only `sweep.rs:1269`'s teardown test. `stepped` holds automatic advancement while its fake processes run, but previously manually advanced to 500 seconds before the ownership probe completed. The new zero-second `probe` handshake waits for the fake Docker network invocation before advancing. Existing VALID, cleanup-confirmed, 530-second completion and teardown-duration assertions remain. Diagnosis is consistent with source; no production timeout widening or weakened assertion. Exact test rerun was not performed here. |
| TDD | Approved spec Scope “Revision 12: admin console” explicitly waives test-first ordering/recorded failures for this task. Passing runtime proof remains mandatory. No historical RED violation is inferred. |

### Hot-path wait audit

TASK-005 adds/moves no wait on a defined verification execution, direct verify, gateway invocation, Oracle query, or Scribe append path. Its new waits are admin-directory `count_assignable_principals`, role-description write/read, provider callback `refresh_display_name`, and replacing the self-profile username lookup with `user_profile`; these are administration or authentication workflows. Existing owners/transactions remain in place. Presentation/BFF reads consume existing admin endpoints. Consequently there is no TASK-005-added hot-path wait needing a sanctioned-wait match, nor a TASK-005 performance benchmark obligation. The merged Bifrost hot-path edits retain their own review/qualification scope; this task review cannot certify that separate task by association. Repository instructions also prohibit adding benchmarks solely from inherited generic hot-path guidance.

## Fresh verification

Executed against candidate HEAD with no tracked-source changes:

- `mise run fmt:check`: exit 0.
- `mise run py:format:check`: exit 0; 185 files already formatted.
- `mise run py:lints`: exit 0; Ruff checks and authored-stub checker passed.
- `mise run py:typecheck`: exit 0; all checks passed.
- `mise run ts:format:check`: exit 0; 79 files, no fixes.
- `mise run ts:lints`: exit 0; 79 files, no fixes.
- `git diff --check`: exit 0 for the working tree.

Two initial attempts to pass several task names to one `mise run` were interpreted as arguments to the first task and failed on nonexistent argument paths. Separate task invocations above corrected the command invocation; those setup errors were not source failures. No formatter was run in write mode.

Range `git diff --check f7e0d67b2...78cd99091` reports one extra EOF blank line in incoming PR 244's Bifrost benchmark evidence Markdown. The recorded working-tree whitespace pass does not establish a clean range. This report-only whitespace creates no behavioral finding or style remediation round.

Rust Clippy, TS typechecking/N-API generation, codegen, Postgres, browser suites and broad `test:wyrd` were not rerun here. Task/user receipts report codegen, principal/identity UI, OpenAPI/platform suites, UI and docs gates passing after merge; these remain implementation receipts, not independently reproduced execution. No receipt for `mise run lints`, `ts:typecheck`, or `ts:napi:check` is present in the supplied TASK-005 record. No absence of a receipt is asserted to prove that a command was never run.

## Proposed blocking finding

### REPO-1 — EVIDENCE_GAP: required final verification remains incomplete

- Authority: TASK-005 Verification and handoff, lines 508–514, explicitly requires `test:wyrd`, Rust lints, and affected SDK format/lint/type/N-API checks; root AGENTS says a red gate is not waived because another change caused it.
- Location/evidence: TASK-005 lines 455–461 and current handoff record `test:wyrd` at 3032/3034, one clock-race test subsequently repaired, and `pg_verification_routes::staging_refusal_recovers_the_same_enqueue_identity` returning a 504 once under the full lane. No clean final lane receipt is supplied. A passing isolated selector proves a different execution condition, not that the required lane is green. The candidate contains the clock fix but no subsequent full pass is recorded. Remaining missing gate receipts are identified above; fresh checks resolve formatting/Python/TS-lint portions.
- Starting condition/trigger: accepting TASK-005 on this integrated candidate using its reported final verification.
- Expected: mandatory affected checks complete successfully, with evidence tied to the reviewed candidate; any unexpected refusal/timeout has a source-supported diagnosis and correction if needed.
- Actual/consequence: the explicitly required final aggregate still has an unresolved failure and remaining required check receipts are incomplete. This is a qualification gap, not proof that TASK-005's console code caused the 504 or a claim about that timeout's root cause.
- Smallest correction: complete the missing required verification at the integrated candidate. For the reported 504 use tracing and the existing verification-route/outbox owners to diagnose a reproducible failure; change production or test code only if a diagnosed defect requires it, preserving refusal/retry identity assertions and timeout boundaries. Record a clean required `test:wyrd` receipt plus outstanding Clippy/TS type/N-API results. No new administration capability, benchmark, migration, widened timeout, ignored test, or architecture edit is authorized by this finding. The final aggregate may be executed at integrated change verification if acceptance authority explicitly reallocates that gate; no such reallocation is recorded here.

No other blocking repo finding proposed. Independent validation must determine the retained ID and reconcile this explicit task-gate obligation with the focused-review workflow.
