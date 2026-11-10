# TASK-004 persistent integrity and concurrency review

Result: **FAIL**. Proposed source-local findings: DATA-01 through DATA-04.

Subject: base `8f1bb789e4d1ad4f481a9cefba2a0eddd82b7ef9`, candidate `487ccb0b60a2a3b40b54ee9e12099880d042ba99`. First complete task review; no carried evidence. Scope: persistent identities, tenant-qualified parentage, transactions, concurrent admission, lease/effect fencing, bounded queries and binding revisions. Permission policy is assigned separately.

## Authority and source coverage

Read the complete task-review skill and review-scope record; original TASK-004 and evidence ledger; applicable spec REQ-024–030 and durability requirements; public-contract repository/administration sections; execution-state supplement entry/queue-lease sections; root instructions and SQL AGENTS; architecture authority map; security-posture tenant/data isolation and Postgres binding; Bifrost execution retention/checkpoint boundary; coordination clock; change workflow, maintainer guide, and testing guidance. CodeGraph was the first code-navigation operation.

| Boundary | Producer → owner → consumer inspected |
|---|---|
| Installation/repository identities | GitHub service/provider and webhook → new migration and `queries/github.rs` → active integration/repository lookup and automation |
| Durable accepted work | verified webhook → `insert_github_delivery` → `CLAIM_DELIVERIES_SQL`, PullRequestIntake, shared ClaimLoop and settlement |
| Branch revisions and logical execution | pinned provider PR/trusted selection → binding reconciliation, admission and supersession → `enqueue_pinned`, existing Submission/VerifierExecutionQueue and review execution reference |
| Queue subject extension | VerificationTargets and Submission → queue insert/claim/status/delivery mappings → existing retained execution read; schema Card/repository XOR |
| Check effects | retained state/configuration failure → CheckPublisher and leased SQL → pinned-head library Check create/list/update and acknowledgement |
| Bounded work/reads | due-tenant lists, LIMIT claims, repository/binding/Check keyset SQL and indexes → server/ClaimLoop; backlog assertions |
| Existing machinery comparison | unchanged ClaimLoop; `leases.rs` held-lease confirmation/cancellation; Operator worker attempt timeout and fenced settlement |

External pinned library source inspected at `9fdf6dc`: RequestLimits/transport and webhook rerequest decoding. No other reviewer reports were read.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| One active integration per tenant and unique active host/installation | Migration partial unique indexes, conditional activation and credential replacement CAS | Association proof and constraints | PASS |
| Default-empty allowlist and tenant/host/repository identity | Repository composite PK and false defaults; reconcile never grants allowed; forced RLS with both predicates | Access proof assertions/owning source | PASS within data scope |
| Durable parent/child tenant identity | Association/delivery FKs include tenant; Check→review FK omits it | No tenant-parent negative FK assertion in new SQL test | FAIL DATA-04 |
| Retry resumes one deliberately requested rerun generation | Each attempt passes rerun=true; admission increments latest generation | Event proof covers a healthy rerun, not partial-admission replay | FAIL DATA-01 |
| Every event writer serializes the same PR | Claim grouping recognizes only top-level pull_request.number | SQL seed contains ordinary pull_request payloads only | FAIL DATA-02 |
| Current replica alone commits admissions/effects | Fixed 120s leases, no renewal/whole-work deadline; intake mutations lack claim token; create ignores false fence | Late-state SQL assertion retains a held token; no expiry/takeover assertion | FAIL DATA-03 |
| Unchanged/revised/removed binding behavior remains branch-scoped | Transaction advisory branch lock, current-family unique index and conditional revision update | Branch proof plus SQL source | PASS for serialized calls; stale intake covered by DATA-03 |
| No provider IO while holding a transaction | Provider/config acquisition precedes admission transaction; commit precedes submission; publisher commits before effects | Full production bodies and stalled-repository proof | PASS |
| Delivery retains execution identity independently | Review-specific enqueue idempotency key, execution reference and retained get_execution; no completed-queue FK | Retained-state proof/shared queue diff | PASS for ordinary retry; rerun replay fails DATA-01 |
| Scoped identities and bounded pages/claims | Logical identity/PR indexes, claim LIMITs, keyset SQL, demand-driven ClaimLoop | SQL assertions: claim bound, ordinary-PR uniqueness, pages 100/100/50, foreign cursor empty, EXPLAIN no Seq Scan | PASS for tested geometry; DATA-02 defeats broader serialization claim |
| Decided Check state is monotonic | ADVANCE ranks state; completion/conclusion changes only unfinished checks | SQL late in_progress preserves completed failure/version 3 | PASS locally; remote stale effects fail DATA-03 |
| Default-push maintenance stays pending | Unique pending insert, no completing consumer | Event pending assertion | PASS |

## Findings

### DATA-01 — Retrying a rerequest creates another generation

Classification: INCORRECT. Priority: P1.

Obligation: REQ-028 explicitly distinguishes a deliberate new generation from webhook redelivery. TASK-004 requires replay without another logical execution and retry without rerunning verification.

Locations: `crates/wyrd/wyrd-server/src/verification/repositories.rs:267-269,426-473,490`; `crates/wyrd/wyrd-sql/src/queries/repository_reviews.rs:414-423`.

Every processing attempt of one durably accepted rerequest passes `rerun=true`. The original delivery identity never reaches admission. Admission commits generation N and its Check at line 428, before principal resolution, engine submissions and execution-reference commits. Process loss after an engine admission, a database error recording its execution id, or a later binding's retryable failure causes that same delivery to execute again. Admission reuses the current review only when `!review.rerun`, so retry creates N+1, supersedes N and submits with a different `repository-review-{review_id}` idempotency key. Existing queue idempotency correctly cannot merge those different identities. Partial multi-binding admission reruns already admitted members too.

Consequence/materiality: one supported user rerequest can produce multiple executions/Checks and cancel its earlier valid work across the normal crash/retry boundary. Healthy rerun tests do not prove durable redelivery semantics.

Smallest correction: associate the accepted rerun delivery with its admitted generation/reviews durably in the existing admission transaction. Replay that delivery must recover those same reviews and resume missing submissions; a distinct deliberate rerequest must still create a new generation. Reuse review-specific engine idempotency. Merely changing the enqueue key would conflict with newly allocated review identities or collapse separate deliberate reruns.

Focused proof: fail/interruption after review commit/first engine admission but before settlement; reclaim the same delivery and assert exactly one additional generation, one execution per binding, original execution ids retained and no extra cancellation. Cover a partially submitted multi-binding suite.

### DATA-02 — Rerequests bypass per-PR serialization before leases expire

Classification: INCORRECT. Priority: P1.

Obligation: REQ-028 monotonic current-PR admission/supersession and task concurrent claim guarantees.

Location: `crates/wyrd/wyrd-sql/src/queries/repository_reviews.rs:35-59`; consumers `repositories.rs:262-269,362-428`.

Both the claim CTE and active-lease exclusion derive PR identity from `payload #>> '{pull_request,number}'`, falling back to delivery_id. The pinned library's `webhook.rs:363-378` decodes valid rerequests from `check_run.pull_requests` or `check_suite.pull_requests`; those bodies have no top-level pull_request object. Two rerequests for the same PR, or rerequest and synchronize, therefore occupy different claim groups and can run concurrently in one batch or on replicas, with unexpired leases.

Reachable failure: A reads current H1 for a rerequest, then waits for its trusted archive. PR advances to H2 and B handles synchronize; its ordinary-PR claim key does not conflict with A's delivery-id key. B admits H2. A resumes, obtains the branch transaction lock, admits H1 and supersedes B's current H2 reviews. Refetching provider identity only at the start does not prevent that permitted stale-read race. Configuration-refusal selections do not take the branch reconciliation lock at all.

Consequence/materiality: a supported mixed-event sequence cancels newer work and presents the older revision as current. This is independent of DATA-03: no expiry, outage or stale lease is required. The SQL test seeds ordinary pull_request payloads, so its per-PR uniqueness assertion misses this producer.

Smallest correction: enforce canonical tenant/host/repository/PR mutation authority for every event path calling review/settle_reviews, including each PR of a rerequest array. Keep provider IO outside SQL transactions and unrelated PRs progressing. Teaching the current grouping only the first array member leaves multi-PR rerequests unprotected; a global scheduler is unnecessary.

Focused proof: overlap a delayed rerequest that pinned H1 with synchronize/current H2 using ordinary fixtures and two workers; assert H2 is not superseded by H1. Include multiple PRs in one rerequest and verify unrelated PR progress.

### DATA-03 — Reclaimed workers continue admissions and remote Check writes

Classification: INCORRECT. Priority: P1.

Obligation: REQ-028 monotonic generation admission, REQ-030 one reconciled Check/no terminal regression, and task reuse of current delivery/effect fencing.

Locations: `repositories.rs:71` (LEASE_MS), `927-939`, `365-473`, `1197-1249`, CheckPublisher process; SQL `repository_reviews.rs:777-792` and advance/deliver/settle token predicates.

Both queues grant fixed 120-second leases. Neither process implementation renews/confirms its claim nor bounds whole-work duration; ClaimLoop cancels only during shutdown drain. Intake does not pass its lease token into handle/review, binding reconciliation, supersession or enqueue. Only final delivery settlement is fenced. The library defaults allow three attempts, 60-second request timeouts and 60-second retry waits (`github.rs:235-263`); sequential token/PR/archive work can exceed the lease without violating any provider bound.

Two affected paths share this missing authority boundary:

- A pins H1, exceeds its lease during trusted acquisition; replacement B adopts H2. A subsequently commits H1 admission and supersedes/cancels H2. A branch lock serializes commits without checking current ownership.
- A Check worker pauses before effect; B reclaims and reconciles/creates/delivers its run. A resumes with the old no-run snapshot. mark_check_create_ambiguous returns false for A's replaced token; line 1240 discards that boolean and unconditionally calls create. A makes a second remote run although its later acknowledgement cannot update SQL. The known-run update path likewise performs no current-ownership check before sending stale state.

Consequence/materiality: supported slow provider/restart/replica paths duplicate Checks, cancel newer work or issue stale queued/running updates after terminal delivery. This is not merely an unavoidable request already sent before takeover: the code initiates new mutations after authority was lost, even after an explicit false fence result. Local SQL state ranking cannot prevent remote regression.

Smallest correction: apply existing held-lease confirmation/cancellation and bounded-attempt patterns at repository queue owners, require live claim authority for intake mutation and before outbound effects, cancel on loss/expiry and honor the failed create fence. Preserve ambiguous-effect reconciliation and short transactions. Lengthening leases or fencing only final settlement cannot repair unbounded accumulated work. Reuse existing lifecycle concepts rather than introduce another scheduler; repository-specific SQL remains with its owner.

Focused proof: force expiry/reclaim while old intake is paused before mutation; release it after newer admission and assert no stale supersession/submission. Pause a Check worker before create, let a replacement deliver, release the old worker and assert one remote run/no terminal regression. Explicitly exercise the false create-fence branch.

### DATA-04 — Check parentage is not tenant-qualified in the database

Classification: VIOLATION. Priority: P2.

Mandatory rule: security-posture Postgres tenant binding requires tenant-scoped parents to expose `(data_tenant_id, id)` and children to carry the tenant key in their foreign keys; tenant identity is checked at storage transitions.

Location: `crates/wyrd/wyrd-sql/migrations/20261109000000_github_repository_verification.sql:205-223,249-252`.

repository_checks stores its own tenant but references only repository_reviews(review_id); the parent has no composite unique target. RLS validates the child's tenant column, not its parent's tenant. A child carrying tenant A and tenant B's review id satisfies this FK (referential checks do not establish tenant relation through parent RLS). Persistent mismatched parentage is representable. Its tenant-scoped consumer join then cannot see the parent, so claimed Check rows disappear from the projection while attempt/lease state updates occur.

Current admit_repository_review produces matching parent/child rows; this report does not claim the HTTP API accepts arbitrary foreign review ids. The finding is the explicit required durable invariant, whose absence is not waived by today's single writer. It is a mandatory architecture violation rather than speculative API hardening.

Smallest correction: expose the composite parent unique key and reference `(data_tenant_id, review_id)`, as association/delivery FKs already do in the same migration. Preserve independent execution lifetimes; add no completed-queue-row FK. Respect migration immutability when applying the correction.

Focused proof: existing SQL Postgres harness accepts same-tenant parent/child and rejects mismatched-tenant parentage through the real tenant login with FK failure. No new harness is needed.

## Verification limits and source state

Static source/SQL review only. No Cargo, migrations, Postgres or live provider commands launched; orchestration owns serialized runs. The orchestrator subsequently reported Rust repository journeys 9/9 and OpenAPI 24/24 passing. Those healthy paths do not close the focused replay/takeover gaps above. No GHES compatibility claim.

HEAD rechecked at `487ccb0b60a2a3b40b54ee9e12099880d042ba99`. Existing modified skills and untracked change packets preserved. Only this report written. A first report-write attempt used missing `python` and made no changes; retried with system python3.

Skipped unrelated permission policy and broad runtime checks. Risk: findings are statically demonstrated and require the separate independent validator before the final task verdict.
