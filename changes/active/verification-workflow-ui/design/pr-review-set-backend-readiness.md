# TASK-005 backend readiness

Status: finalized

Historical 2026-11-19 prerequisite investigation. TASK-005A subsequently delivered
the request/retention output, and authorization-model supplied trusted-parent
reads. Approved [UI spec revision 8](../spec.md) and the
[resumption design](resume-005-007.md) supersede the old retained-read blocker
and identify TASK-007's bounded status projection correction. The original
findings below remain historical evidence, not current activation instructions.

## Approved decision — 2026-11-19

The maintainer approved the prerequisite scope, then selected the recommended
explicit Wyrd PR-wide request through HTTP, SDKs and MCP. Specification revision
7 defines its tenant/PR/key replay identity, existing Card/read and Verifier/read/run
request permissions, declaration-owned execution authority and complete retained
membership through existing verification records. TASK-005A implements it;
TASK-005 consumes its delivered output. No GitHub Re-run hook, UI rerun button
or migration is authorized. Findings below describe inspected implementation,
not a claim that the approved extension is delivered.

Inspected 2026-11-19 at working-tree HEAD `7cf5bc4ab`. This is static design
research, not an independent task-review verdict or runtime qualification.
Existing unrelated working-tree edits were preserved.

## Problem and required outcome

Before implementing [TASK-005](../tasks/TASK-005-pr-review-sets.md), the maintainer
needs the backend to support the whole accepted journey: open a PR's latest
review set, select earlier sets, open exact members, request another set at the
same commit, and then advance the head. Membership and policy must be accepted
server facts, including members whose execution cannot run.

The current [specification](../spec.md), REQ-012 and AC-007, requires that journey.
Revision 7 adds REQ-019/020 as a bounded backend exception. TASK-005 remains
proposed until TASK-005A supplies the admission dependency and journey proof.

## Verified findings

### Same-commit rerequest is missing

[`GithubConnection::pull_request`](../../../../crates/wyrd/wyrd-server/src/components/github/pull_requests.rs:297)
replays a non-deactivated current generation when `reviews` matches. The
[`reviews` predicate](../../../../crates/wyrd/wyrd-sql/src/queries/verifier_executions/pull_requests.rs:399)
compares base **branch**, head SHA and head repository. It does not compare the
new delivery ID. Another delivery ID at the same source therefore does not
request a new occurrence. Moving only the base commit is also not a new source.

[`PullRequestEvent::observable`](../../../../crates/wyrd/wyrd-server/src/components/github/pull_requests.rs:107)
observes lifecycle/source events, not a fresh-review request.
[`webhook`](../../../../crates/wyrd/wyrd-server/src/components/github/service.rs:707)
dispatches PR events and installation/access events; it has no Check rerequest
admission branch. Closing/drafting and then reopening/readying can admit the same
source again because the previous generation was deactivated. A bare reopened
event while the generation remains active is not a bypass.

This matches the [Trigger authority](../../../../architecture/wyrd-design.md:2651).
It is a missing prerequisite for AC-007, rather than evidence that the existing
source-replay predicate violates its backend contract. Removing source replay
would turn ordinary event retries into duplicate verification.

### Refused members are not staged by the inspected input handoff

[`reserve_pull_request`](../../../../crates/wyrd/wyrd-sql/src/queries/verifier_executions/pull_requests.rs:759)
records admission authority/binding refusals through `REFUSE_SQL` as terminal
`errored` queue rows. These rows have no `next_attempt_at = infinity` reservation.
[`lock_pull_request`](../../../../crates/wyrd/wyrd-sql/src/queries/verifier_executions/pull_requests.rs:528)
therefore reads them with `reserved = false`.

[`hand_off`](../../../../crates/wyrd/wyrd-server/src/components/github/pull_requests.rs:457)
returns all member IDs internally, but stages input records only for members
whose `reserved` flag is true. An all-refused generation returns before staging
any input record. The caller stages superseded/principal-fenced cancellations;
it does not stage these admission refusals as PR member input records. Thus the
claim that every admitted member's PR record reaches Bifrost is false for this
inspected path. A generation containing only refusals can be absent from an
input-based history; a mixed set can appear to have fewer members.

The existing
[`revoked_accountable_principal_settles_a_durable_refusal` test](../../../../crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:1080)
asserts errored PostgreSQL rows and no runnable claims. It does not assert
retained Bifrost membership or history visibility. This finding was not reproduced
by running the test in this investigation.

### Grouping is feasible; complete authorized history remains to be proved

For staged members,
[`ResultPayloadBuilder::inputs`](../../../../crates/wyrd/wyrd-server/src/verification/results.rs:412)
stores `PullRequestActivation` in `execution_inputs.inputs` with origin
`pull_request`. The pins include provider host/repository/PR, base/head,
delivery and generation; the row also carries exact execution, binding, Verifier,
subject and owner correlation. This supports grouping real records without
inventing membership from SHA or timestamps. A dedicated history endpoint is
not inherently required.

However, `hand_off` stages per execution after reserving the whole set in SQL.
The existing
[`interrupted_handoff_completes_the_same_fanout_after_restart` scenario](../../../../crates/wyrd/wyrd-server/tests/integration/pg_github_pull_requests.rs:1005)
explicitly covers interrupted fanout. Seeing a group of input rows alone does
not certify that every reserved member is already visible. Do not label an
observed subset as a complete set or let delayed older input replace latest.

PR pins are inside a
[sensitive input column](../../../../crates/vala/vala-bifrost-redux/src/tables/verification/execution_inputs.rs:56).
Exact domain execution reads also enforce
[Verifier and Card permissions](../../../../crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs:521).
TASK-005 must demonstrate permitted grouped queries, authorization before counts,
exact frozen policy joins, bounded history, and latest resolution independently
of the ordinary Verify time filter. Neither grouping feasibility nor an admin
query establishes those guarantees for the intended reader. This is a proof
dependency, not a confirmed demand for a new read API or relaxed permissions.

## Agreed implementation proposal

Deliver a bounded backend prerequisite before activating TASK-005:

1. Define one explicit PR-scoped fresh-review request with a durable request
   identity. A distinct request at the same current source reserves a new
   generation under the policy selected at acceptance; its retries reuse the
   accepted generation and members. Resolve request replay across later source
   changes too, so an old retry cannot supersede newer work. Preserve the existing
   per-PR lock, current provider read, connection revision fence, policy pins,
   authority checks, supersession and normal queued execution handoff.
2. Reuse the existing Scribe/input/event owners to retain every member, including
   admission refusals, with its generation and exact policy. Refused members
   remain terminal and never become runnable. Establish how readers distinguish
   an incompletely visible admission from its full membership; first test whether
   existing retained facts suffice before proposing any additional metadata.
3. Prove current/history queries through the actual permitted Bifrost surface.
   Add no dedicated list service if the existing query surface meets the contract.
   Then record delivered request semantics and journey evidence in TASK-005.

For a clear PR-wide operation, the selected input is an explicit Wyrd request
under the existing GitHub/admission owner, projected through ordinary HTTP,
SDK and MCP surfaces. It avoids conflating a single Check retry with a whole
Repository policy review. Revision 7 authorizes that operation and its normal
projections. No UI rerun button follows from this agreement.

A GitHub-native input was considered and not selected:
GitHub documents `check_run` and `check_suite` `rerequested` events in its
[Checks guide](https://docs.github.com/en/apps/creating-github-apps/writing-code-for-a-github-app/building-ci-checks-with-a-github-app)
and [suite rerequest API](https://docs.github.com/en/rest/checks/suites#rerequest-a-check-suite).
That choice must settle exact PR targeting, single-Check versus whole-set behavior,
obsolete-head refusal and redelivery identity. A suite/commit association alone
must not substitute for Wyrd's exact PR/generation identity.

## Observable proof before activation

Use existing managed Postgres, `GithubAppMock`, `WyrdTestServer` and public client
journey owners, not fabricated browser membership:

- Admit two bindings, request another set at the unchanged head, retry that
  request, and advance the head. Expect two distinct same-head generations,
  stable retry member IDs, then a new-head generation with correct supersession.
- Retry the older explicit request after the new head; it cannot create or
  reactivate obsolete work. Concurrent retries and restart preserve identity.
- Change selected policy between requests: each set retains the policy and
  membership accepted for that request; ordinary redelivery keeps the old pins.
- Admit mixed runnable/refused and all-refused sets. Public retained queries
  include every authorized member with its actual status and no invented verdict.
- Interrupt handoff and recover. Latest/history must not silently report partial
  membership as complete or regress to older delayed records.
- Prove allowed, denied and foreign reads before counts and membership exposure;
  history outside a default Verify time range remains explicitly selectable.

The existing lifecycle, policy-reselection, interrupted-handoff, refusal and
tenant-isolation tests are reuse points, not execution evidence from this review.
No commands or test names for new tests are prescribed before implementation.

## Authority and handoff

Approved revision 7 supersedes revision 6's backend exclusion only for
REQ-019/020. Trigger, security and Bifrost authority are reconciled in this
documentation update before production changes. Existing verification records
retain request identity and complete frozen membership. Exact wire names,
metadata layout and query mechanics remain implementation-owned. No migration
or new table is authorized; if one is needed, return SPEC_REVISION_REQUIRED.
Keep the existing UI design and no-aggregate-verdict rule. Implementation proof
remains outstanding; design approval does not make TASK-005 ready.

No production code, migration, commit or runtime test was changed or executed.
