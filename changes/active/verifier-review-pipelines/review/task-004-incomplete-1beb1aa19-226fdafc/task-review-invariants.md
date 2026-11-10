# TASK-004 invariant review — FAIL

Subject: base `dfd19aa129830696d68cf8d4a09c144805847345`, committed candidate
`1beb1aa1980cb03495dfb0778a353604670ba935`, authorized WIP tree
`226fdafc56bacc707b9167d1887be9b72f5e6d53`. First review of the replacement;
no prior acceptance carried forward. Read actual cumulative/WIP source, not
implementation summaries or other reviewer reports. CodeGraph preceded source
navigation. The nine principal changed runtime/query files inspected on disk
were byte-compared with the captured tree and matched.

Authority: review-scope, TASK-004, spec revision24 REQ-027/028/030/035–038,
public-contracts repository/Check views, execution-state supplement, root and
SQL AGENTS, workflow, maintainer style, TESTING, and the execution-state sections
of architecture/wyrd-design.md and architecture/bifrost-design.md. TASK005/006
retain extraction/review pipeline and developer-education ownership. This review
does not require those later features to exist in TASK004.

## Grouped acceptance

| Team outcome/invariant | Evidence and assessment |
|---|---|
| Manually judge a repository without fabricating a Card | Request discriminant → VerificationControl pin → Submission.repository → queue subject columns/ExecutionSubject → runner Frozen/custody → typed Bifrost ExecutionIdentity/result. Source preserves the pin after queue admission and leaves Card wire shape intact. Existing repository_verification journey exercises direct and queued repository results plus Card/both/neither behavior; no new run here. **Source-supported implemented subset**, not acceptance of automated handoff. |
| Adopt an exact trusted branch suite before running any member | automation trusted_configuration reads pinned base and hashes fetched paths/blob IDs; reconcile_bindings retains identity/revision on equal content and retires removed selections on that branch. pg_github_repository covers sequential keep/revise/retire. **Fails automated handoff and complete-suite preflight**, INVAR-004-01/02. |
| Replay PR work while preserving current authority | Signed ingress persists before return; delivery dedup is distinct from derived execution key; SQL statements use database-clock leases. **Settlement fencing alone is insufficient**, INVAR-004-03. Existing ordinary submission lanes order one idempotency key, not all deliveries/writers of a PR. |
| Associate each judgment with the right Check and deliver retained results | New Check records are readable, but worker record identity can preserve the wrong execution, INVAR-004-04. Worker only creates queued/configuration database records: no selected-state-to-remote-Check delivery consumer exists in this candidate. **Unfinished PR-to-Check outcome**; do not split every missing terminal mapping/retry/view into separate findings. |
| Stop obsolete/revoked work and preserve independent recovery | Existing execution runner retains Bifrost recovery/terminal ACK owner; the repository subject is carried through refused/cancelled custody without requiring a fake subject Card. GitHub worker explicitly settles rerequests/pushes/installation events as ignored and does not implement close/supersession cancellation. **Unfinished event/revocation outcome**, grouped with completion of the PR journey, not 44 missing features. |

## Material defects in implemented paths

### INVAR-004-01 — Automated admission repins after selecting trusted configuration

Location: `components/github/worker.rs:177–180, 292–311`;
`components/verification/service.rs:724–737` (server source paths below are under
`crates/wyrd/wyrd-server/src/`).

Producer-to-consumer: worker pins S1, adopts S1's base configuration, derives an
execution key from S1/binding revision, then passes only the mutable PR locator
to `VerificationControl::enqueue`. That method calls `pin_pull_request` again
and stores S2 in `Submission.repository`. Worker records the Check using S1.
A push or target edit during configuration fetch/import is an ordinary supported
race: the queued Bifrost result judges S2 while its selected suite, idempotency
identity and Check head describe S1. Two suite members can even receive different
pins. This violates REQ-028/036/037, including exact-head result association and
resolve-once per generation.

Smallest correction: retain the ordinary public enqueue behavior, but let the
internal automated admission reuse the already server-pinned subject and suite
resolution through the existing Submission/queue path. Carry that one immutable
resolution to every execution and Check; retries restore it. Do not add a public
caller-supplied SHA option, another execution engine, or another source store.
The existing `Submission.repository` already carries the required pin. Closure:
change provider PR response between adoption and admission; assert all execution
subjects, binding provenance and Checks retain the first selected identity.

### INVAR-004-02 — A later invalid suite member leaves an admitted valid subset

Location: `components/github/automation.rs:508–553`;
`components/github/worker.rs:283–311`;
`components/verification/service.rs:720, 728, 965–1002`.

`adopt` resolves exact Card rows using `get_card_spec_by_ref`, which checks blob
availability, not canonical Verifier readiness or `verifier:run`. It commits all
bindings, then dispatch loops sequentially through enqueue. With two registered
Verifier selections and an executor granted run on only the first, adoption
accepts both (the Card-row read does not enforce that grant), enqueue commits
the first, and the second refuses with 403. A later window-evidence Verifier
likewise fails `resolve` only after the first has been queued. Worker settles
the delivery unprocessed on this nonretryable error; it creates no suite-level
configuration failure. This directly violates TASK004's complete-suite preflight
and no-valid-subset behavior, not a speculative atomicity requirement for
arbitrary concurrent permission changes.

Smallest correction: before committing/admitting any suite member, apply the
existing VerificationControl authorization and canonical target/readiness plus
supported-input compatibility rules to every resolved selection. Return the
ordinary configuration failure if any member fails; then use the same pinned
admission path for the valid suite. Reuse existing rules rather than treating
Card lookup as readiness or inventing a second validator. Preserve ordinary
missing-evidence inconclusive semantics. Closure: two selections, second denied
or window-incompatible, yield one configuration failure and zero execution
admissions; complete valid suite still admits normally.

### INVAR-004-03 — Expired delivery owners can still reconcile and initiate admission

Location: `components/github/worker.rs:113–137, 177–180, 194–199`;
`components/github/automation.rs:543–553`;
`crates/wyrd/wyrd-sql/src/queries/github_deliveries.rs:38–67`.

The delivery claim commits before processing. Its token is used only by finish
and release; neither adopt/reconcile nor execution admission receives it. Pause
worker A after its old trusted base has loaded, expire/reclaim its delivery,
and let B reconcile/admit a newer snapshot. Resumed A can overwrite the branch's
current binding selection/revision and initiate old admission even though its
event lease is gone. Distinct deliveries of the same PR can also be claimed by
different replicas immediately: SKIP LOCKED protects one delivery row, not PR
ordering. A failed final settlement detects the loss after consequential writes.
The existing queue execution lease only protects an execution after admission;
it cannot fix this producer-side authority loss. This violates explicit stale
claim/admission and common PR ordering obligations in TASK004/REQ-028.

Smallest correction boundary: reuse existing claim/lease/cancellation and queue
admission machinery, carrying the source claim/current PR generation into the
durable handoff and checking it at reconciliation/admission commit boundaries.
Make all ordinary/rerequest writers use that same authority. Polling a token once
before a long await or adding a finish-only predicate does not fence admission.
No scheduler, global lock, lease-history store, or transaction across provider IO
is required. Closure: block A before reconciliation/admission, expire/reclaim,
advance B, resume A; A must produce neither a binding rollback nor a new execution.
Include the sibling delete writer in that same branch-authority proof.

### INVAR-004-04 — Check identity drops execution-key distinctions and preserves an obsolete association

Location: `crates/wyrd/wyrd-sql/src/queries/github_checks.rs:23–42`;
`crates/wyrd/wyrd-sql/migrations/20261109000400_github_checks.sql:33`;
`components/github/worker.rs:294–311, 334–352`.

The execution key includes binding revision/digest, base SHA and PR number.
Check uniqueness is only tenant/host/repository/head/generation/name; generation
is always the default 1. Re-recording intentionally returns the old row unchanged.
For a PR with unchanged head, advance its base's configuration and reopen/ready
it: binding reconciliation and execution_key produce a new required execution,
but record_check finds the old same-name/head record and retains its old base and
execution_id. The worker ignores that returned association and logs the new
execution as queued. This is independently wrong even if INVAR-004-01 is fixed:
inspection and eventual delivery can never represent the new judgment. A new PR
using an existing commit is another affected producer of the same collision.
REQ-028/030/036 require stable association with the selected logical review, not
silently reusing an unrelated execution because the commit happens to match.

Smallest correction: use the same selected durable logical-review/generation
identity for execution replay and Check association, including distinctions
that trigger a new judgment; preserve same-generation replay and terminal
monotonicity. Reuse the existing Check/generation control records; do not add a
result copy, remote history service, or unconditional conflict update that would
rewrite a completed Check. Closure: same head with changed base/config creates
the correctly associated review/Check; replay of that review preserves it and
never rewrites the preceding terminal result.

## Scope and correction burden

The smaller task recommendation is to prove deployment App configuration plus
both tenant onboarding entries and selected-repository control first. Keep the
repository execution/automatic PR-to-Check work as one separately scoped ordinary
Verifier journey before TASK005 extraction and TASK006 pipeline proof. That split
is a task-packet recommendation, not authority to delete approved durability,
permissions, pinned configuration or recovery guarantees. No new product decision
is needed to correct the four defects above; they violate explicit existing
requirements and can use existing owners.

Skipped running Cargo, SQL, SDK, lint and codegen commands because orchestration
owns build serialization; inspected tests provide limited existing proof only.
Risk: the unfinished worker is wired into runtime, so the admission/association
defects are active behavior, not hypothetical future code. No source/task edits.
