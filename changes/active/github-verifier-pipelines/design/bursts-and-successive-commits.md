# PR bursts and successive commits

Status: exploring. The user approved the baseline PR supersession/queue flow;
operational details recommended here are design proposals unless marked existing.

## Problem and intended outcome

The user has experienced 41 PRs within a few minutes. This is a concrete workload
to support, not grounds for claiming an unmeasured throughput target. Every
eligible distinct PR must be durably accounted for; excess work waits within
bounded resource use. Successive revisions of one PR should not consume capacity
reviewing every obsolete intermediate head or publish stale results as current.

The user clarified that DAG duration is workload-defined; no fixed end-to-end
completion time is a requirement. Design for reliable acceptance, bounded resource
use and continued progress. Observe queue wait separately from execution time.

## Existing Wyrd behavior, inspected rather than benchmarked

- `verification/submissions.rs`: SubmissionSink batches acknowledged submissions
  into Postgres. Its process outbox allows 16,384 reserved submissions and uses
  four writer connections. This is a volatile handoff bound, not durable webhook
  capacity or proof of throughput. A hard-kill handoff gap must be recovered from
  the proposed durable GitHub ingress state, not ignored after sending 2xx.
- `verification/mod.rs::RuntimeLimits`: default is 64 claimed/running Verifier
  executions per process. Further executions remain queued in Postgres.
  `boot/mod.rs` composes defaults clipped to server shutdown drain; no operator
  execution-slot setting was found in this composition. Expose the existing
  bound through owning config if operators need to size PR workloads.
- `verification/claims.rs::ClaimLoop`: reserves capacity before claiming, drains
  batches, rotates across due tenants and polls to discover other replicas' work.
  This is tenant rotation, not strict per-repository FIFO or a hard tenant quota.
- `wyrd-sql/.../verifier_executions.rs::claim`: oldest due work, SKIP LOCKED,
  database-clock leases, deadline established at first claim and retained on
  reclaim. Replicas compete for durable rows without claiming the same lease.
- `VerifierExecutionQueue::cancel`: cancels unfinished execution, fences its
  lease, notifies the holder and keeps terminal delivery owed. Existing runner
  confirmation/renewal stops stale work. This supplies cancellation mechanics,
  not an existing PR-head selection policy or arbitrary remote-call revocation.
- Existing hosted workspace/Python and engine block limits are separate from
  whole-execution capacity. A multipass Verifier expands model/tool demand;
  41 PRs are not necessarily 41 independent provider calls.

## Recommended burst handling

### Durable acceptance and bounded execution

1. Authenticate the bounded webhook and commit its integration delivery record
   before responding successfully. Do no extraction/model work in the request.
2. A bounded integration handoff resolves authoritative PR state and admits
   current review work through the existing SubmissionSink and execution queue.
   The delivery/review state is integration coordination, not another verification
   queue or execution engine. Retry a crash between ingress and queue commit with
   the same logical identity; do not regenerate executions.
3. Claim only available existing worker capacity. Keep other executions durable
   in Postgres and show queued/running status. No spawned task/model call per
   incoming delivery and no resident full source bundle for every waiting PR.
4. Respect existing block/workspace/code budgets and provider retry indications.
   Coordinate an installation's provider cooldown across replicas rather than
   letting every process immediately retry the same throttled installation.
   Provider retry failures remain bounded/observable; no unbounded retry promise.
5. Reuse shared repository snapshots and exact accepted review evidence. Ranked
   context/evidence acquisition is performed once per review execution then reused
   by its reviewer steps, not rebuilt for each Agent pass.

If 41 PRs select V Verifiers each, there can be 41 × V execution admissions.
Review plans can multiply model calls further. Capacity is sized from measured
observed execution/resource use, extraction memory and model/API allowance.
There is no universal review duration: configured DAGs may have different paths,
fanout and durable waits. A growing runnable backlog is an operator capacity
signal, not a reason to promise a fixed review completion time. Do not equate the existing default of 64 with safety for
64 full review pipelines. Keep limits in operator/server configuration, not
user Repository TOML. Additional existing verification-hosting replicas provide
capacity only when provider budgets, storage and Postgres allow it.

### Repository cache: bounded first-use concern, not a new infrastructure requirement

The initial recommendation overstated duplicate cold preparation as a general
41-PR problem. Ordinary PR context calls reuse the existing snapshot; they do
not rebuild it. Keep existing snapshots, publication CAS and bounded execution
as the baseline. Do not require a new distributed preparation lease or scheduler
without evidence that duplicate first-use work is material.

Actual library flow in the inspected checkout:

1. On first use without a snapshot pointer, context calls refresh. ColdBuild
   pins the default-branch commit/tree, lists files, streams that commit's archive
   in memory, verifies/indexes eligible source with bounded fallback blob reads,
   collects co-change history, and publishes a source-free snapshot plus pointer.
2. The snapshot stores extracted declaration/index facts and history, not a
   checkout or persisted full source tree. Consumer storage adapters persist it.
3. Subsequent PRs load those facts and apply their own in-memory head overlay,
   fetching changed/drifted blobs and selected source context under bounds.
   Each PR then publishes its own pinned review context. It does not mutate the
   shared default-branch snapshot or reindex the entire repository per commit.
4. Explicit refresh after default-branch changes updates the snapshot from a
   commit comparison, reuses unchanged facts and indexes changed blobs. Same-tree
   refresh reuses the existing snapshot. Non-forward/truncated/oversized changes,
   changed attributes or absent snapshot objects can trigger a bounded rebuild.
5. Context does not auto-refresh an existing snapshot simply because default
   branch advanced; it overlays within its bounds and reports coverage.

Concurrent calls when no pointer exists can still duplicate initial preparation;
CAS prevents conflicting pointer publication, not duplicate pre-publication work.
That is a narrow first-use/rebuild race, not evidence that a warmed repository
with 41 PRs performs 41 repository builds. Measure that case with the current
library before adding special coordination. A failed/incomplete cold build can
also leave no pointer, with scoped context and later rebuild attempts; preserve
coverage reporting rather than claiming all cache calls are complete.

Sources: `review.rs::ReviewPipeline::context`, `snapshot.rs::ColdBuild::build/fetch`,
`update.rs::Update::run`, `pull_request.rs::PrContext::publish`, and Snapshot fields.

### Provider delivery failure

GitHub recommends responding within 10 seconds and asynchronously processing
webhooks. GitHub does not automatically redeliver failed deliveries. Consequently
HTTP 429/503 by itself is not a recovery strategy. Recommend bounded scheduled
recovery of failed App deliveries through GitHub's delivery/redelivery API,
reusing normal signature/dedup/admission processing after redelivery. The polling
interval, retention window and outage objective need operator policy. Never
acknowledge a delivery that was not durably accepted.

Sources: [webhook practices](https://docs.github.com/en/webhooks/using-webhooks/best-practices-for-using-webhooks),
[failed deliveries](https://docs.github.com/en/webhooks/using-webhooks/handling-failed-webhook-deliveries).

## Successive-commit policy

Approved behavior is recorded in
[successive commits and pipeline ownership](approved-code-review-support.md#successive-commits-and-pipeline-ownership).
Cancel running obsolete review, skip replaced queued heads, and compare the new
head with the last completed usable review; partial work never advances that
baseline. The generation-key/coordination/race details below remain implementation
proposals until their public and durable contracts are approved.

Use one current logical review generation per tenant + stable external repository
identity + PR number. The key must not include a Repository Card version: selecting
another policy version must not split one PR into unrelated supersession domains.
A generation freezes the selected policy, PR base/head and its per-binding jobs.
Different Verifiers within the current generation may run concurrently.

- A new source revision replaces unfinished older-generation work automatically.
  If the old generation is queued, cancel it before expensive work. If running,
  signal existing execution cancellation and stop future tool/model work as
  cancellation propagates. Keep accepted evidence and history.
- If A is running and B then C arrive, cancel A and discard queued B in favor of
  C. No arbitrary debounce timer is required initially; coalescing pending work
  already eliminates obsolete heads. A future quiet-period setting needs evidence
  that remaining cancellation waste justifies added latency/configuration.
- Completed old reviews remain historical; do not rewrite them as cancelled.
  New results apply to the pinned new source. Never copy an old approval onto
  a new head; waiting old-generation executions/continuations are superseded too.
- Cancellation is cooperative. An already sent remote request may finish or be
  billed. Correctness also requires an identity/generation check before new
  admission and before external delivery, not an assertion of instant zero overlap.
- Every Check is bound to its generation and head. A stale completion cannot
  update the new generation's Check or represent the current PR as passed.
  Late provider writes to an old-head Check remain old-head effects; there is no
  atomic transaction with GitHub that can retract an already in-flight request.
- Reconcile out-of-order deliveries against provider state and protect the
  per-PR generation transition against concurrent/stale reconciliation. Git SHA
  lexical order and webhook arrival time do not establish source ordering.
- Delivery dedup and logical review identity are different. Redelivery of one
  event creates no new review; an explicit valid rerequest intentionally starts
  a new generation even at the same head, using the selected policy at that new
  acceptance. Retry of that rerequest retains its generation.
- Freeze once the logical review is accepted; do not silently change its pins
  inside extraction. If source moved before a pinned acquisition completed,
  supersede/reconcile rather than report extraction success for different code.

The user approved supplying both the complete current PR diff and incremental
changes since the last completed usable review, together with previous-review
context. The DAG chooses how to use them. Cancelled/unreviewed intermediate heads
do not advance the baseline. Missing/unusable history, rewritten history or
changed policy falls back to full review. No old model verdict is automatically
reused as a judgment of a new head. Exact output fields remain unapproved.

## What the independent library supplies

Inspected checkout d8c772d:

- `ReviewPipeline::context/refresh/read`: reusable source-free repository snapshot,
  PR overlay and ranked context; default-branch refresh is incremental. PR context
  is distinct for pinned source identities.
- `PrContext::run_at`: verifies a supplied base/head pair and returns
  PullRequestMoved without publication when the provider no longer matches it.
  This is extraction consistency, not a PR-job cancellation manager.
- Async calls cancel by dropping their futures; durable stages are retryable per
  their contracts. `GitHubError::RateLimited` includes retry_after and the client
  bounds waits/attempts. It does not supply a deployment-wide installation budget.
- No queue, worker loop, model review scheduler or current-generation policy.
  Those are Wyrd's consumer responsibilities under the library's authority.

## Mitari comparison — inspected local implementation

`mitari-platform/app/main.py::_process_pr_event` (around lines 8926–8958)
explicitly skips automatic verification on synchronize/new commits. It posts a
neutral Check telling the developer to rerun through GitHub or the app. The
synchronize branch does not cancel or replace the previous run. Opened/reopened
can auto-run subject to its eligibility/billing checks. This is a different
product policy from the automatic successive-head review selected for Wyrd.

Mitari's `app/github/webhook_intake.py` has a durable database intake/outbox,
canonical identity by installation/repo/PR/head, bounded front-door/admission
executors and a recovery loop. It separates accepting PR work from the slower
verification execution path. These are reusable design lessons, not permission
to port its Python worker architecture into Wyrd or a throughput measurement.

## What CodePal establishes, and what it does not

Snap describes a shared context build for downstream workflows and focused
re-review for each new commit, including resolving findings whose files leave the
diff. Its public article does not specify cross-commit cancellation, pending-head
coalescing or queue capacity. Speculative-pass cancellation inside one review is
a different mechanism. Do not attribute the proposed Wyrd supersession protocol
to CodePal.

Source: [Snap Engineering, CodePal](https://eng.snap.com/codepal).

GitHub Actions documents replacing pending work and optional cancellation of
running work in a concurrency group. This supports the recommendation as a common
CI pattern; Wyrd is implementing its own existing-queue integration, not adopting
GitHub Actions as an execution dependency.
Source: [GitHub concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).

## Focused verification to require during implementation

Use the real Postgres queue and WyrdTestServer with controlled provider/model
responses; this is a bounded journey, not synthetic host stress:

- Accept 41 distinct PRs promptly, with multiple Verifiers, workers held at a
  small configured concurrency. All current jobs remain accounted for, workers
  stay within bounds and a second tenant makes progress. Measure ack latency,
  queue age and controlled-test progress separately from execution duration;
  no fixed production DAG completion SLA is required.
- Kill between ingress acceptance and admission; replay recovers each logical
  current job without duplicates. Reject/lose a delivery and prove provider-side
  recovery rather than assuming automatic retry.
- Exercise concurrent first-use and warm-cache PR acquisitions and measure
  provider work with existing limits/CAS. Add coordination only if measured
  duplicate cold work warrants it; warm runs must reuse the snapshot.
- Drive A-running/B-queued/C-new, reverse webhook order, race old completion,
  throttle a provider and revoke access. Only C can publish a current result;
  old evidence remains readable and other PRs/tenants continue.

Operational visibility: accepted/deduplicated deliveries, unrecovered delivery
failures, queue depth/oldest age, running work, time to first claim, completion
latency, superseded/cancelled work and provider throttling. Reuse current telemetry
owners; add only missing signals needed to operate the promised behavior.

No runtime test or benchmark was executed during this design research. Existing
queue capabilities are source-backed reuse evidence, not proof that the new
GitHub integration or the 41-PR journey already works.
