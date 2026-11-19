# Independent remediation validation — TASK-005 Codex r2

Recommendation: **PASS**. FIND-005-1 is closed. The independently validated remaining finding ledger is empty.

## Immutable subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Base: `b262696afcd731d94433dfd1d8d0a9b3ab0d64f4`.
Prior candidate: `1f798d132c03cbc2474e1bad4bc36763e8047848`.
Cumulative candidate: `2660af6de2ec7b625ab4564e9519526e240cb020`.
Actual remediation range: `git diff 1f798d132 2660af6de`.
Original task: `changes/active/github-verifier-pipelines/tasks/005-pull-request-extraction.md`.
Approved authority: revision 8 of `changes/active/github-verifier-pipelines/spec.md`.

This fresh ponytail-rev independently checked the fix, affected complete helper bodies and consumers, actual range, prior acceptance coverage and specialist omissions. Read the review skill, r2 applicability and verification records, root/SQL rules, Rustdoc and maintainer guidance, workflow static-proof/remediation rules, original task/spec context, and all six Codex r1 discovery reports plus its findings-validation, verdict, verification and remediation task/evidence. No non-Codex review directory supplied evidence. CodeGraph's inherited attempts did not surface these test helpers; targeted reads supplied the uncovered source.

The range adds six Rust documentation lines and records the Codex r1 reports/remediation evidence. The additional Markdown evidence is accounted for separately from the comment-only Rust change. Production behavior, executable tests, signatures, assertions, manifests, dependencies, architecture, original task and approved spec are unchanged. HEAD independently matched the cumulative candidate before and after source inspection. No rebase invalidates correspondence. No new evidence or dependency change requires reopening accepted runtime behavior.

## Independent applicability and coverage assessment

| Contract/invariant and owner | Evidence and role decision |
|---|---|
| Changed-item documentation; HeldBatch/source_extracted → proof maintainers | Fresh ponytail-rev is required and supplied by this report. Root AGENTS:152–154 and agent_docs/rust.md:233–241 govern field and panic documentation regardless of visibility. Independently checked actual additions against complete bodies and callers. |
| Authored action, registration/direct eligibility, admitted source, output and consumers; X1–X3/X5–X7/X9/X11–X13 | Carry forward r1 task-review-behavior.md's X1–X13 source/assertion matrix, standards-review.md's cross-tier authority coverage and maintainer-review.md's complete existing-owner comparisons, reconciled by r1 findings-validation.md. No changed runtime/public/client/generator owner or abstraction requires fresh behavior/repo/maintainer discovery. |
| Accepted provenance, completion, baselines and recovery; X4/X8 | Carry forward r1 task-review-invariants.md's admitted claim → hosted extraction → ordinary settlement → accepted state → fenced completion → future history trace, system-review.md's restart/failure assertions, and domain-review-data-security.md. Comments do not alter shared state/lifecycle or recovery; no fresh invariant/system specialist is applicable. |
| Tenant metadata batching and source custody; X10/X13 | Carry forward r1 invariant/system/domain reports, which trace one tenant transaction per batch, replies after commit, independent reads, rollback and tenant boundaries. Fresh inspection confirms changed helper docs do not change those transactions or their assertions. No fresh domain/concurrency specialist is applicable. |
| Credentials, tenant source authority and persistent integrity | r1 domain-review-data-security.md covers admitted source through installation access/screened transport, tenant metadata, accepted evidence and fenced history. Its producer/consumer and assertion table supplies boundary evidence, not just a PASS label. No new trust boundary, SQL, schema or credential path requires another sensitive-domain reviewer. |
| Follow-up/new finding validation | No conflict, unresolved failing path or new blocker was discovered. No follow-up is needed. No required selected report is missing. |

The specialist selection/omissions in applicability.md are supported. This is one bounded remediation round, not a new comprehensive discovery pass. The original mandatory rustdoc gate remains enforced; behavioral green checks do not waive it.

## FIND-005-1 closure and correction burden

Prior discovery **STD-005-1**, validated **CONFIRMED / VIOLATION**, concerned three omissions sharing one source: missing mandatory documentation in newly authored proof helpers. The stable finding is **CLOSED**, not rejected or renumbered.

- `crates/wyrd/wyrd-server/tests/integration/pg_code_review_cache.rs:756–760` now documents both fields. `start` first commits the record, opens a transaction and locks that row, then queues a second write and waits for its batch to block. `lock` therefore really retains the row lock, and `write` really represents that second pending write. `release` commits the retained transaction before awaiting the write. The new text explains those distinct roles accurately.
- Both consumers remain intact: `writes_queued_behind_a_busy_batch_share_one_commit_per_tenant` constructs HeldBatch at line 863, checks queued rows are invisible before release, then compares commit identities for shared same-tenant commits and separate tenants. `a_failed_metadata_batch_applies_none_of_its_writes` constructs it at line 937, releases it, requires all failed-batch replies to be Backend errors and the healthy row to remain absent. No fixture or assertion changed.
- `crates/wyrd/wyrd-sql/tests/integration/pg_verifier_executions.rs:4607–4618` now has `# Panics` and names failed reads or no row visible to the supplied TenantConn. That matches the complete body: tenant-scoped `fetch_one(...).await.expect("execution reads")`. Its completion/replay consumers at 4883–4906 still assert `[true,false,true]` and identical replay; stale/rollback/cancel consumers at 4954, 4980 and 5006 still assert false. The helper retains the existing SQL authority and error behavior.

The correction is the minimum local documentation addition selected in TASK-005-R1. Deletion would lose the required documentation or useful proof setup; inlining, new guards or replacing expect would change or duplicate unrelated proof behavior. No abstraction, runtime guard, dependency or additional test is necessary. All three omissions are closed at their existing owners without changing adjacent behavior.

## Reconciled acceptance matrix

`R1` below means sibling `005-pull-request-extraction-codex-r1/` only. `J` is server integration `pg_github_pull_request_extraction.rs`; `C` is server integration `pg_code_review_cache.rs`; `Q` is SQL integration `pg_verifier_executions.rs`. Unchanged source/assertion rows are carried forward from R1 reports, rather than represented as fresh execution in this round. Their original line references remain in those reports; the six new comments shift only portions of C and Q by three lines.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| X1 exclusive authored source/media preservation | R1 behavior X1: spec validation and existing hosted media dispatch | Both/neither HTTP refusals, extract_source/no declaration writes; media binding proof | PASS, carried |
| X2 registration-derived queue_only | R1 behavior X2: cards registration and existing direct flag consumer | Authored false cannot override, media eligible, cold direct refusal/no provider reads | PASS, carried |
| X3 admitted tenant/source authority | R1 behavior X3 and domain source/credential rows | J non-PR refusal/no reads; token mint and absence from details | PASS, carried |
| X4 baseline fact/fallback/fences | R1 invariant X4 and domain baseline/completion rows: BASELINE_SQL, provenance and single/batch settlement | Q P/A/B, eligibility, policy, replay, stale/rollback/cancel/RLS; J failed-history retry | PASS, carried |
| X5 public pipeline, one accepted acquisition | R1 maintainer existing-owner comparisons and behavior X5 | J comparison-read counts across Prompt/workspace/settlement | PASS, carried |
| X6 closed exact output/empty changes | R1 behavior X6: extraction/output projection | J exact pins, moved/fork bases, rename/delete/file kinds and empty changes; output roundtrip | PASS, carried |
| X7 ordinary selective live binding | R1 behavior X7 and system Scribe-outage row | Prompt/mount equal retained output, bystander omission and held-persistence consumption | PASS, carried |
| X8 accepted-state recovery | R1 invariant X8 and system restart row: ordinary RunState/restored settlement | J:1392–1550 retained/recovered Prompt/workspace equality, expired contexts, unchanged GitHub reads and true fact | PASS, carried |
| X9 honest incomplete/refused results | R1 behavior X9 and system provider/model rows | J missing blob/patch, omission accounting and model-size refusal do not pass | PASS, carried |
| X10 bounded tenant group commit | R1 invariant X10/domain metadata rows: CodeReviewMetadata Worker and SQL owner | C lone write, precommit invisibility/shared commit IDs, tenant separation and atomic rollback; affected assertions re-inspected | PASS |
| X11 wire/HTTP/MCP/guidance | R1 behavior X11 and standards TS/docs rows | Same extraction JSON over HTTP/MCP, changes.files/tagged-side docs and static generated parity | PASS, carried with prior limits |
| X12 owning architecture custody | R1 behavior X12 and standards architecture row | wyrd-design/Bifrost custody text agrees with live/accepted evidence and coordination fact | PASS, carried |
| X13 targets/tenancy/no new table | R1 behavior X13, standards SQL and domain schema rows | Existing integration targets, Q tenant isolation and C separate tenant commits; prior boundary checks | PASS, carried |
| R1-1 / AC-R1-1 field rustdoc | C:756–760 names retained row-lock transaction and pending second write | Fresh complete start/release and both consumer inspections | PASS |
| R1-2 / AC-R1-2 panic rustdoc | Q:4607–4618 documents read failure/absent visible row | Fresh complete body/consumer inspection against fetch_one/expect | PASS |
| R1-3 / AC-R1-3 preserve executable proof | Actual range has six Rust doc additions; other changes are review reports/evidence | Range inspection, fresh format and diff checks in r2 verification.md | PASS |

Original non-goals remain excluded: no alternate source injection, acquisition/clone/index owner, report delivery, source editing, splitting service, policy import or extra client extraction surface is added. Revision 8 remains the authority over the stale task body sentences identified and reconciled in R1.

## Final ledger, proof limits and recommendation

**Validated remaining ledger: empty.** FIND-005-1 is closed; no new or retained CONFIRMED/REVISED finding requires remediation. No rejected proposal is converted into optional work and no human-directed addition is assigned a finding ID.

r2 verification.md records fresh `mise run fmt:check` and remediation/worktree whitespace checks passing. This validator also independently ran `git diff --check 1f798d132 2660af6de` successfully and checked HEAD. R1 verification.md supplies the carried-forward 198 selected behavioral tests (24 spec, 2 server unit, 55 connected server, 40 SQL, 77 vala-verify), plus dependency and tenant checks, with exact commands and nextest identities. These were not rerun in r2. Prior limits remain: no full Clippy, rewriting codegen/docs generators, benchmark or production GitHub credential execution. No new production wait or hot-path work exists in the remediation.

Recommend **PASS** with no remediation task. The mandatory documentation gate is satisfied and source-backed acceptance remains valid for unchanged behavior. The two-unsuccessful-remediation convergence threshold has not been reached; no further review scope is prescribed. This validator wrote only this report and made no source/test/doc/task/spec change or commit.
