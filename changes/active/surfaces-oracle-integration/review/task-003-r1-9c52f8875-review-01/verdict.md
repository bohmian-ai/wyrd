# TASK-003-R1 Review Verdict

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `9c52f887595d4e8a04764d3d2836926e5df70950`
- Candidate tree: `e2731ff2d23fc1a0f4b356ccdd79caf3f6c7b93e`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 9 (candidate originally carried revision 8)
- Original tasks: `TASK-002-converge-client-and-sdks.md`,
  `TASK-004-unify-bifrost-data-root.md`, and
  `TASK-003-close-repository-integration.md`
- Reviewed remediation:
  `review/tasks-002-r3-004-003-bbfdf35e-review-01/TASK-003-R1-close-cumulative-review-findings.md`

The commit and tree remained fixed throughout both review waves. The checkout's
later evidence commit and unrelated `verified-change-contract` working-tree
changes were excluded from the implementation subject.

Human-directed amendment, 2026-09-16: the owner confirmed no storage
configuration has been deployed and selected the tested `WYRD_STORAGE_URL`
structure. The earlier storage-reversion finding is removed. Historical Wave 1
reports remain unchanged; the final ledger below reflects this decision.
The owner also selected merge-to-`main` cloud Actions with no weekly cadence
and local `mise.local.toml` real-cloud commands as sufficient candidate proof.
Their explicit override is recorded in approved specification revision 9;
no additional spec approval is required for this decision.
An independently supplied deadline addendum was subsequently validated
directly against the client and server request paths and added as findings
`FIND-TASK-003-R1-8` and `FIND-TASK-003-R1-9`. The original Wave 1 reports
remain historical; no public query deadline change is proposed.

## Acceptance matrix

| Obligation | Strongest evidence | Result |
|---|---|---|
| Delete the stale Bifrost contract types, tool names, and six generated/golden schema families | Candidate source/generator/artifact inspection, exact empty search, independently rerun 11/11 wire tests | PASS |
| Prove every managed Bifrost data-root directory write/sync/remove usable before composition | Locked `BifrostDataRoot::prepare` probe and independently rerun 4/4 root tests | PASS |
| Bound total pending Oracle audit work before spawn while preserving the connection share, non-blocking reads, failure metric, and drain | Source/caller trace and the focused locked-chain journey evidence | PASS |
| Run generic Rust tests in the existing Linux CI owner without duplicating the full gate | Workflow and selector proof; independently rerun `check:ci-selection` 32/32 and 7/7 | PASS |
| Run the six missing non-credentialed journeys through the existing nightly matrix | Exact seven-entry matrix and existing task owners | PASS |
| Keep the owner-selected URL storage configuration aligned across GitHub Actions and Wyrd docs | Storage workflows and self-hosting pages already use `WYRD_STORAGE_URL`; final consistency and documentation closeout remains | FOLLOW-UP — not a retained finding |
| Every surviving local non-credentialed storage test has a reachable owner and nonzero selection | `local_handle_crud` and `backend_contracts` are selected by no mise lane | FAIL — `FIND-TASK-003-R1-2` |
| Do not weaken or broaden an excluded Forge settlement-cancellation proof to make the gate pass | The recovery assertion now accepts a publication refusal by string match | FAIL — `FIND-TASK-003-R1-3` |
| Every retained materially changed Rust item satisfies the repository rustdoc contract | Retained storage journey helpers/tests omit required intent, panic, and interruption documentation | FAIL — `FIND-TASK-003-R1-4` |
| Live-cloud qualification runs on merges to `main`, not weekly | Existing cloud workflow already triggers on `main` pushes; approved revision 9 supersedes the proposed cron finding | PASS |
| Broad local proof is recorded against one immutable corrected candidate | The full gate/journey matrix is tied to `8e266b2b4` plus unidentified dirty content, not the candidate tree | FAIL — `FIND-TASK-003-R1-6` |
| S3, GCS, and Azure real-cloud tests pass locally against the immutable candidate | `mise.local.toml` owns three concrete cloud commands, but exact candidate results are not recorded | FAIL — `FIND-TASK-003-R1-7` |
| Healthy slow client query and artifact transfers outlive the JSON control timeout | The query stream uses a connect-only client, but authenticated raw/PUT and presigned GET/PUT use the total-timeout client | FAIL — `FIND-TASK-003-R1-8` |
| Oracle's accepted query deadline, not the generic HTTP edge, controls pre-response execution | The protected `TimeoutLayer` covers `sync_query` through Oracle preparation and first-batch waiting | FAIL — `FIND-TASK-003-R1-9` |

## Review results

| Review | Result | Material outcome |
|---|---|---|
| Task implementation | FAIL | Four historical proposals; storage-reversion proposal superseded by owner decision |
| Repository standards | FAIL | Historical schedule proposal superseded by owner direction; rustdoc finding retained; stale Oracle WAL/relay prose rejected |
| Client/contracts domain | PASS in original Wave 1 | Original ledger empty and `FIND-TASK-002-19` closed; direct deadline addendum now adds client finding `R1-8` |
| Persistent data domain | FAIL | Historical storage-configuration proposal superseded by owner decision |
| Repository integration domain | FAIL | Cloud-evidence proposal revised to local proof; storage-configuration proposal superseded |
| Stream/resource domain | FAIL | Historical storage-configuration proposal superseded by owner decision |
| Structured Ponytail validation | FIX_REQUIRED after approved revision 9 and deadline addendum | Seven bounded findings remain; local cloud proof is sufficient when recorded |

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-003-R1-2` | REVISED | MISSING | Select the local handle test; preserve the unique Azure dispatch assertion in the owned Azurite test; delete the orphan test binary |
| `FIND-TASK-003-R1-3` | CONFIRMED | VIOLATION | Restore the Forge assertion and settle the recovered attempt before worker stop without production semantic changes |
| `FIND-TASK-003-R1-4` | REVISED | VIOLATION | Document every retained materially changed Rust test/helper under the repository rustdoc rule |
| `FIND-TASK-003-R1-6` | CONFIRMED | MISSING | Run and record the complete required local matrix on one final immutable candidate/tree |
| `FIND-TASK-003-R1-7` | REVISED | MISSING | Run and record the existing local S3/GCS/Azure `mise.local.toml` tasks on the final candidate |
| `FIND-TASK-003-R1-8` | CONFIRMED | INCORRECT | Use one connect-bounded `HttpTransport` client; apply total timeout only in `send_with_retry`; prove slow authenticated and external GET/PUT |
| `FIND-TASK-003-R1-9` | CONFIRMED | INCORRECT | Bound query ingress/pre-Oracle admission but let Oracle's captured deadline own preparation and first-batch wait; preserve all other edge timeouts |

Full evidence, caller traces, rejected proposals, and decision-complete
corrections are preserved in `findings-validation.md`.
The owner-requested GitHub Actions and Wyrd documentation alignment is a
follow-up in `TASK-003-R2`, not a replacement finding or a storage rollback.

## Prior-finding closure

`FIND-TASK-002-19`, `FIND-TASK-004-1`, and `FIND-TASK-003-1` through
`FIND-TASK-003-3` are closed. Earlier TASK-002 findings remain closed. The
new client transport deadline finding is separate. The stale Oracle WAL/relay
documentation conflict remains a final-change-review limit because this task
explicitly excludes that sweep.

## Verification limits

- Independently passed: cumulative `git diff --check`, `check:ci-selection`,
  all four data-root tests, and all eleven Bifrost wire tests.
- Focused source inspection confirms the five original remediation corrections.
- The Postgres Oracle journey, MCP test, full gate, complete language/journey
  matrix, and credentialed cloud jobs were not independently rerun in Wave 1.
- The supplied broad result is not tied to the immutable candidate tree, and
  exact local S3/GCS/Azure command results on that tree are not recorded.
- The deadline findings are source-validated; their proposed focused regression
  tests have not yet been written or run. The existing preparation pause is
  available in the test-support Oracle, but the in-process server fixture may
  need a narrow binding hook.
- Approved revision 9 replaces revision-8 cloud scheduling and candidate-proof
  obligations. Post-merge cloud Actions results remain operational checks, not
  a prerequisite to accept this candidate.

## Verdict

**FIX_REQUIRED**

Seven bounded findings and one storage-alignment follow-up remain. The owner's
cloud scheduling/proof override is now approved revision 9 and reflected in
the testing reference. `TASK-003-R2-close-r1-review-findings.md` is ready for
implementation; no further approval of this cloud-proof choice is needed.
