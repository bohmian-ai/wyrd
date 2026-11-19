# TASK-004 invariant review

Result: **FAIL**. Proposed source-local IDs: `INV-004-01` through `INV-004-04`.

Subject: `8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9...487ccb0b60a2a3b40b54ee9e12099880d042ba99`.
This is the first review, with no carried-forward acceptance. HEAD remained the
candidate. Source was not edited. The two modified skill files and untracked
change packets identified by the review scope remain untouched. Only this report
is written by this reviewer.

## Scope and authorities

Read the review scope and original task, its obligation/evidence ledger, relevant
approved spec requirements (particularly REQ-027/028/030 and the branch-revision
rules), the public-contract and durability context, root instructions, SQL
directory instructions, spec-driven-development rules, maintainer guide,
architecture authority map, verification lifecycle authority in
`architecture/wyrd-design.md:1844-1894`, and testing guidance. Unapproved wait
proposals are not used as authority.

CodeGraph was the first code-navigation operation. Expanded the shared map over
repository intake/Checks, SQL transitions and migration, verification service,
shared claim loop, execution cancellation/authority/runner, GitHub administration,
and the pinned library's Check protocol/request bounds. Traced producers through
consumers, including sibling writers of each reviewed SQL authority. No other
reviewer's conclusions were received or used.

The new SQL records have legitimate control lifetimes distinct from retained
execution state. RepositoryWorker reuses ClaimLoop, BackgroundAuthority,
VerificationControl admission and VerifierExecutionQueue cancellation. Check
protocol uses the pinned library. No second verification engine or retained-state
reader is necessary here. The defects below concern missing invariants at these
existing seams, not a recommendation to replace those workflows.

## Acceptance matrix

`R` means `crates/wyrd/wyrd-server/src/verification/repositories.rs`; `Q` means
`crates/wyrd/wyrd-sql/src/queries/repository_reviews.rs`; `RV` means
`sdks/wyrd-sdk-rust/tests/integration/repository_verification.rs`.
Verification cited here is assertion inspection plus the task's recorded passing
runs, not a rerun by this reviewer.

| Obligation | Implementation evidence | Exact assertion evidence and limits | Result |
|---|---|---|---|
| I-03/K-04: distinguish accepted delivery dedup from ordinary logical review dedup | Delivery primary key in migration:181; Q REVIEW_IDENTITY_SQL:471-477; R submit:481-509 uses review-derived IdempotencyKey | RV:1490-1560 asserts same-ID acknowledgement 200, new-ID acknowledgement 202, `again == settled`, and unchanged Check-create request count | PASS for ordinary events |
| E-03: one intentional rerun adds one generation; retry resumes that generation | R:267-269 passes unconditional `true`; Q:414-423 increments on every such call | RV:1901-1930 asserts one rerequest has same subject, different execution, old Check cancelled; does not interrupt/retry that rerequest | FAIL: INV-004-01 |
| B-01: retain unchanged revision; revise changed configuration/dependency content and preserve selection provenance | R:595 hashes config bytes but R:653-654 replaces that hash for successful bindings; Q:264-281 compares supplied digest | RV:2225-2246 asserts identical config on another base keeps bindings; RV:2261-2289 asserts a changed Verifier UID retains binding ID and changes UID. Neither checks changed config with unchanged UID or persisted configuration digest | FAIL: INV-004-02 |
| B-01: branch-scoped identity and retirement | Q:248-301 locks repository/branch and scopes updates by target_ref; branch-family unique index in migration:152-154 | RV:2205-2221 asserts distinct same-family bindings on main/release; RV:2281-2296 asserts release bindings unchanged when main removes/revises its selection | PASS for branch scoping, subject to freshness below |
| B-02/E-01/E-02: current PR lookup defeats late old deliveries; metadata ignored, drafts wait, closed cancels | R:254-259 ignores metadata-only edit; R:342-359 reads current PR, handles closed/draft; R:431-434 supersedes | RV:1851-1881 drains late old delivery then asserts unchanged latest execution/nonterminal state; RV:1882-1900 does same for metadata; RV:1970-2020 asserts closing admits nothing and execution cancels; RV:2022-2069 asserts draft has zero Checks then ready admits one | PASS for serialized healthy paths; FAIL for overlapping intake: INV-004-03 |
| Claims/fences prevent older admission from cancelling or retargeting newer work (AC-011/032) | Q:38-69 derives serialization key from `pull_request.number` or delivery ID; R:935-938 calls handle without lease; admission/supersession have no ownership or freshness condition | Q pg test:1123-1175 asserts bounded claims, distinct PR identities and only `stalled-1` held, using only pull_request-shaped bodies. RV event assertions run sequentially after observed completion | FAIL: INV-004-03 |
| Closed/superseded executions cancel through existing queue; completed history remains exact | R supersede:519-537 invokes queue.cancel; Q:491-520 completes only nonterminal Checks; execution cancel owner:3335-3352 only cancels unfinished work | RV:1837-1858 and :1995-2020 observe cancelled old execution; Q pg test:1255-1281 asserts late in_progress keeps completed/failure/version 3 | PASS for connected executions under current intake |
| Disablement cancels affected active work and prevents recovery continuation | GitHubAdministration::retire:770-789 changes integration status only; runner continues using accepted principal/execution authority | RV access tests prove new work is refused/evidence retained, not cancellation of an already-running execution after disable | FAIL: INV-004-04 |
| K-01: terminal status/verdict mapping, completed-without-result waits | R project:1013-1079 | R tests:1371-1424 assert every status-without-result, passed→success, failed→failure, inconclusive→action_required | PASS |
| K-03: local Check state never regresses, independent retry/reconciliation | Q ADVANCE_CHECK_SQL:689-703 and deliver:810-841; R write:1197-1256 reconciles external_id before creating after ambiguity | Q pg test:1255-1281 asserts completed/failure/v3 survives late running; RV:1462-1490 asserts exactly one completed remote run per name and `check_update_lost()`; RV:1650 onward asserts held-write rotation preserves execution associations | PASS for exercised serialized delivery/lost-reply cases; does not establish arbitrary overlapping external-effect fencing |
| S-01: delivery consumes retained execution state without rerunning; source uncertainty is not absence | R:1125-1146 calls shared VerificationControl::get_execution; service:255-300 reads selected retained result; no private payload storage | RV:3071-3130 asserts old PG relations absent, unavailable source leaves one Check with same execution and no action_required; RV:3136-3180 asserts same execution yields one success Check and stays completed | PASS |
| E-04: default-push maintenance remains pending | R:287-308 inserts default-only maintenance; Q:553-580 idempotent insert; no completion consumer | RV:2074-2104 asserts exactly main/release SHA/pending maintenance after drain | PASS |
| Q-01: bounded claimed/page sizes, no provider IO inside SQL transaction, unrelated progress | Q bounded LIMITs and scoped SQL; R commits before provider and engine waits; shared ClaimLoop retains capacity | Q pg assertions:1138 (`round.len() <= CLAIM`), :1164 (OTHERS+1), :1208-1210 (`[100,100,50]`, expected order); RV stalled-repository journey asserts other repository succeeds and oldest transaction is younger than the stall | PASS for stated bounded/progress obligation |

## Proposed findings

### INV-004-01 — A retried rerequest creates another generation

Classification: **INCORRECT**. Obligation: REQ-028 explicitly distinguishes a
deliberate rerun from webhook redelivery; E-03 requires a new generation for the
intentional request, not each intake attempt.

Location: R:267-269 and Q:414-423, with R:426-475 as the affected admission/link
consumer. The rerun bit is reconstructed from event kind on every attempt. No
delivery/rerequest identity is associated with the generation. Q deliberately
bypasses reuse whenever that bit is true.

Reachable failure: admit a rerequest, commit its review/Check, then lose the process
before intake settles. On restart the same durable delivery is reclaimed. It
passes `rerun=true` again, creates generation N+1 and supersedes N. If N already
submitted, verification is cancelled/repeated; if it did not, additional empty
generations/Checks still appear. A multi-binding rerequest that successfully
submits its first member and encounters a transient admission failure on the next
has the same failure without process loss. Submission-level idempotency cannot
help because its key incorporates the newly allocated review ID.

Sibling writers checked: `admit_repository_review` has the one production caller
in R; `set_review_execution`, `conclude_repository_check` and supersession are
the following stages. Delivery primary-key dedup does not prevent reclaiming its
running/pending row and the settlement happens only after all these stages.

Consequence/materiality: normal retry/restart duplicates an explicitly requested
review and can cancel its first accepted execution. This is the exact durability
case the distinction is intended to protect.

Smallest correction: bind the intentional rerun's existing durable delivery
identity to its selected generation and resume it on reclaim; retain the existing
review-derived execution IdempotencyKey. A second user rerequest must still get a
new generation. Do not disable retries or suppress all rerequests after the first.
This is a bounded extension of the existing intake/review coordination, not a new
scheduler or engine. Closure proof: interrupt after review admission and after
first execution submission, reclaim that same rerequest and assert unchanged
review/generation/execution IDs; a distinct rerequest then advances once.

### INV-004-02 — Successful binding revisions do not record configuration content

Classification: **MISSING**. Obligation: spec:823-830 explicitly says unchanged
configuration blob/dependency digests preserve revision and changed
configuration/dependency content creates a new revision. Task binding
reconciliation and B-01 require exact configuration provenance.

Location: R:595 and :653-654. Although the configuration SHA is computed,
successful selections return only `SHA256("{key}\n{uid}")`. This value is stored
under `config_sha256`, copied into review identity and used by Q:264-281 to decide
whether revision changes. The actual successful configuration SHA is lost.

Reachable failure: two trusted root wyrd.toml blobs select the same registered
Verifier (for example, one changes the selection's local-vs-registered spelling,
or adds another Verifier). The existing selected member retains the same key and
UID, so its purported configuration digest and revision are identical despite a
changed configuration blob. Immutable Card identity does not describe which
configuration blob selected it. Tests change the UID for their revision case and
therefore do not expose this.

Sibling consumers checked: reconcile_repository_bindings compares this digest;
NewReview/REVIEW_IDENTITY_SQL and repository_reviews persist and deduplicate it;
no other producer supplies the missing successful-config digest.

Consequence/materiality: the explicit frozen revision/provenance contract is
false, so binding/review state cannot distinguish trusted configuration changes
that preserve a selected UID. This is a required integrity boundary rather than
a cosmetic column naming issue.

Smallest correction: carry the already-computed pinned configuration identity
into the successful binding/review fingerprint, retaining exact resolved
Verifier/dependency identity. No new parser, dependency store or abstraction is
needed. Closure proof: change config bytes while preserving an existing selected
Verifier; assert actual configuration fingerprint changes and binding ID stays
stable while its revision advances. Identical bytes at another base must still
reuse the revision, and other branches remain untouched.

### INV-004-03 — Intake ordering does not cover rerequests or stale claim holders

Classification: **INCORRECT**. Obligation: REQ-028, AC-011 and AC-032 require
older work never to cancel/retarget newer work and stale configuration not to
undo current branch bindings.

Location: Q CLAIM_DELIVERIES_SQL:38-69; R:935-938; R:341-434. The queue serializes
`pull_request.number`, falling back to delivery_id. A check_run rerequest carries
its PRs in `check_run.pull_requests`, so it has a different serialization key from
ordinary events for those same PRs. The lease token is used only by final delivery
settlement, not reconciliation/admission/supersession/submission. No current
generation/freshness condition protects those writes; SUPERSEDE_REVIEWS_SQL
unconditionally supersedes every current review outside keep.

Reachable failure without waiting for lease expiry: ordinary intake A reads the
old current PR, then spends time acquiring its pinned base tree. PR moves. A
rerequest B for the same PR is claimable concurrently because its SQL key is the
delivery ID; B reads the new PR and admits/submits its new review. A resumes and
reconciles the old selection, then supersedes/cancels B. Its one earlier provider
lookup does not make that write current. A second route is ordinary intake that
outlives the fixed 120-second lease: another replica reclaims/advances the PR,
while the old holder continues because no ownership check is passed into handle.
The library permits three attempts, one-minute calls/waits (github.rs:250-259),
and intake also waits for declaration/verification admission; there is no
whole-handler deadline or renewal that makes this route unreachable.

Sibling writers checked: branch reconciliation's advisory lock serializes just
its transaction, not acquisition freshness; review admission and supersession
do not consume delivery lease/current generation; set_review_execution later
links without superseded/lease condition; queue.cancel correctly cancels the ID
it is given but cannot know that its producer selected an obsolete review.
ClaimLoop owns dispatch/drain and does not renew this repository lease.

Consequence/materiality: current accepted verification is cancelled by obsolete
intake, and bindings can be restored to an older selection. This directly breaks
the out-of-order protection demonstrated only sequentially in the present tests.

Smallest correction boundary: preserve the existing shared ClaimLoop, but make
per-PR admission authority cover all event kinds that mutate that PR, and require
the current claimed authority at the reconciliation/admission/supersession
commit boundary; a reclaimed holder must not publish later submission links or
leave an uncancelled superseded execution. Rerequests listing multiple PRs must
respect the same per-PR authority. A single additional current-PR GET alone is
insufficient because the two workers can still cross between it and commit.
No global lock or long SQL transaction over provider IO is justified. Closure
proof: hold old intake after its PR snapshot and request the new revision via
rerequest; prove it respects the same per-PR ownership. After ownership is
transferred and the newer revision admitted, release the old intake and assert
newer bindings, execution and Check remain current. Prove stale mutations are
refused while unrelated PRs still progress.

### INV-004-04 — Disabling an integration leaves its active executions running

Classification: **MISSING**. Obligation: spec:838-841 requires disabled integration
to cancel work; the lifecycle authority in wyrd-design:1863-1866 says
cancellation/revocation immediately stops computation and retains delivery only.

Location: `components/github/service.rs:770-789` and
`queries/github.rs:227-242`. retire only updates the integration row and commits.
It never invokes the existing execution cancellation owner. It neither revokes
the execution Service principal nor marks associated review executions cancelled.

Reachable failure: start the existing slow Verifier fixture via a PR; wait until
its execution is running; disable that integration. Its accepted queue/engine
work continues until completion/timeout. Intake refuses subsequent deliveries
and CheckPublisher cannot obtain an active integration, but neither condition
stops the existing runner. The accepted authority is Wyrd principal authority;
BackgroundAuthority::unattended:303-337 does not bind it to a GitHub integration
status. Runner's repository arm projects the retained subject, not a continuously
checked integration cancellation policy.

Sibling writers checked: disable/remove both use retire; repository access change
uses its separate flag-only change method; principal revocation has its own
working VerifierExecutionQueue cancellation path; PR supersession already calls
that queue owner and shows the needed cancellation mechanism is available.

Consequence/materiality: an administrator's explicit disable fails to halt
affected computation, including potentially expensive provider work, contrary to
the accepted lifecycle contract. Blocking later Check publication is insufficient.

Smallest correction: in the disable/remove control boundary cancel only affected
unfinished repository executions via existing queue cancellation semantics,
preserving completed retained evidence, unrelated executions of the same Service
principal, and independently owed terminal delivery. Use the existing review→
execution association rather than revoking the principal or adding runner polling
as a substitute. The intake/link race in INV-004-03 must not admit an execution
after this cancellation boundary. Closure proof: disable/remove while a PR
execution is running; observe cancelled retained execution and no further
computation/recovery, while historical evidence and an unrelated execution remain
available.

## Verification limits

No Cargo/build/test/lint/codegen command was run; orchestration owns those lanes.
Static source, immutable diff identity, existing assertion bodies and recorded
verification were inspected. No live GitHub/GHES compatibility was attempted.
The orchestrator additionally reported review reruns of the Rust repository
journeys (9/9 PASS) and OpenAPI (24/24 PASS); these do not exercise the missing
interruption/overlap cases above.
The proposed failure schedules were traced from production producers through
their SQL/engine consumers; they have not been executed as new regression tests.

The four corrections use existing control/admission/cancellation owners. There
is no request for a new engine, global scheduler, archive protocol, or speculative
hardening. Check publisher overlapping remote-effect behavior was inspected but
no separate remote-regression claim is retained without a focused protocol proof.
