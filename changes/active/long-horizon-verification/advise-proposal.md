Keep the `Verifier` a versioned function over a **closed evidence bundle**. Expand two surrounding capabilities: a Wyrd-owned session resource that accumulates evidence from any source, and the existing async executor with durable checkpoints and bounded fan-out. This optimises for trustworthy, recoverable judgments under load: conversation duration affects evidence assembly; expensive analysis affects scheduling. Neither should make a Verifier stateful.

1. **Repository facts that change the starting proposal**

   The scouts found several useful foundations—and several gaps:

   | Repository fact | Consequence |
   |---|---|
   | The engine is `vala-eval`, not `vala-verify`. Its `RunLedger` keeps outputs in memory; the execution deadline cancels the running DAG. [Engine](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/crates/vala/vala-eval/src/blocks/engine.rs:126) | Durable step recovery is new work. |
   | Async enqueue, polling, Postgres claims, renewable leases and fenced settlement already exist. [Routes](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/crates/wyrd/wyrd-server/src/components/verification/routes.rs:211), [queue](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:221) | Extend the existing executor; do not introduce another queue or workflow engine. |
   | `202` acknowledges memory staging. Scribe acknowledgement precedes another in-memory handoff to Postgres; completion can precede retained results. [Submission path](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/crates/wyrd/wyrd-server/src/verification/submissions.rs:1), [authority](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/architecture/wyrd-design.md:1305) | “Queued” currently does not mean durably accepted, and “completed” does not guarantee durable evidence. |
   | Outbox queues are unbounded; Verifier claims lack an execution-count permit. [Outbox](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/crates/shared/wyrd-runtime/src/outbox.rs:82), [claims](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/crates/wyrd/wyrd-server/src/verification/claims.rs:86) | Downstream admission alone cannot protect server memory. |
   | Ordinary Bifrost queries can omit or duplicate acknowledged rows across publication overlap. [Bifrost authority](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/architecture/bifrost-design.md:346) | A successful query is insufficient proof of complete session evidence. |
   | Triggers currently support schedules; bindings reject duplicate attachment of the same Verifier version to one subject. [Trigger](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/crates/wyrd-spec/src/card/trigger.rs:15), [binding authority](/home/thorrester/Documents/GitHub/wyrd-long-horizon-verification/architecture/wyrd-design.md:1252) | Session activation and “two bindings” require explicit contract changes. |

   Everything below is design advice, rather than a description of existing behaviour.

2. **A session owns accumulation, not execution**

   Introduce a tenant-scoped **verification session runtime resource**, not another Card kind. It identifies an accumulation boundary, exact subject versions, and an ordered set of accepted evidence contributions. A session can span conversations, minutes, or application sessions.

   The server owns its lifecycle:

   `open → sealing → sealed`, with explicit expiry/abandonment for unfinished sessions.

   Each append carries a session ID, idempotency key and typed evidence declaration. The server assigns acceptance order; contributor timestamps remain separate. Producer order, server acceptance order and conversational order must not be silently conflated.

   All sources converge on the same evidence assembly boundary:

   - SDK/HTTP/MCP calls supply structured contributions or uploaded evidence references.
   - Tracing adapters supply contributions through that boundary; trace IDs are optional provenance.
   - Bifrost selections are materialised into immutable evidence before acceptance into the closed bundle.
   - Media uses the existing tenant-qualified, digest-addressed evidence upload mechanism.
   - External systems are read through `Source`; Wyrd does not require producers to implement tracing or a particular runtime.

   **Use immutable evidence chunks and a manifest as the authoritative bundle.** Postgres stores lifecycle, membership receipts and manifest references; bulk content belongs in evidence storage. Bifrost retains searchable analytical projections. Do not reconstruct a supposedly complete bundle later from a time-range query.

   Reuse existing upload storage for chunks, batching small contributions before persistence. A durable append receipt means both the referenced content and its membership are committed. A preliminary `202` means staging only.

   Closing atomically stops new membership, records the final accepted sequence and starts manifest finalisation. Concurrent appends either enter before that boundary or receive a closed-session rejection. Retries of previously accepted idempotency keys return their original receipt.

   **Late evidence never mutates a sealed bundle.** The caller opens a successor session referencing the predecessor. A fresh verification can explicitly cover both. Idle closure uses server acceptance time and a persisted deadline; maximum age and size prevent indefinitely open sessions.

3. **Extend durable execution where it already lives**

   Keep execution orchestration in the existing server runner, reusable DAG evaluation in `vala-eval`, and contracts in `wyrd-spec`.

   Preserve the existing execution lifecycle, adding durable records for runnable work, attempts and checkpoints. Workers claim bounded work, renew ownership and publish only under the current lease token.

   Persist checkpoint payloads in evidence storage; Postgres holds small references and attempt state. A checkpoint identity includes the tenant, exact Verifier version, step/item identity, input digests and effective extraction/judging configuration. **Content addressing deduplicates stored outputs; it does not make an LLM deterministic.** Retain the actual judge output and provenance.

   Add bounded `each` expansion over an upstream collection. Persist the item set once; schedule children incrementally. Never create one task, future or Postgres row for every video frame before admission. Segment first, enforce cardinality limits, then page through runnable items.

   Budgets cover elapsed time, active compute, input/output bytes, fan-out cardinality, provider tokens and configured monetary limits. Reserve capacity before dispatch and reconcile usage afterward. Lease loss may repeat a remote call, so execution is at-least-once; checkpoint publication is fenced. Do not promise exactly-once provider billing.

   **Budget exhaustion is an execution reason, not a new subject verdict.** Keep `passed`, `failed`, `inconclusive`; expose reasons such as `budget_exhausted`, coverage and unfinished work separately.

   Aggregates operate over the declared complete item set:

   - Missing or errored members remain visible.
   - Pass rates cannot silently discard unsuccessful attempts from the denominator.
   - A judgment may fail when available evidence decisively proves failure.
   - Passing requires the aggregate’s declared coverage requirement; otherwise it is inconclusive.

   Human review needs explicit assignment, response authorisation, timeout and immutable response evidence. It can reuse durable waiting, but does **not** fall out automatically. Add it after machine execution recovery works; waiting must consume no execution permit.

4. **Load design: separate arrival rate from execution capacity**

   At 300 contributions/second, Wyrd receives roughly **26 million contributions/day**. At 4 KiB each, that is about **100 GiB/day before replication and indexes**. Session count alone is therefore a poor quota.

   The append path performs authentication, authorisation, size/schema validation, idempotency and bounded staging. It does no extraction, provider invocation, broad analytical query or whole-session reconstruction.

   Batch immutable chunks and Postgres membership commits across admitted contributions. A durable receipt waits for persistence; batching reduces transactions, not the amount of membership information. Partition and expire receipts according to an explicit retry/retention contract. A hot session needs its own append-rate limit because its closure boundary necessarily serialises membership.

   Enforce bounded bytes and work at every stage:

   - Per-tenant pending bytes, open sessions, retained evidence and runnable work.
   - Deployment-wide staging and active-work limits.
   - Separate limits for provider requests, extraction, media memory and output expansion.
   - Typed retryable refusal before admitting excess work.

   For perspective, **300 five-minute executions/second require 90,000 concurrent executions** at steady state. Async submission cannot make that sustainable. Quotas must limit execution arrival, and bindings must distinguish cheap per-contribution checks from less frequent whole-session checks.

   Use weighted tenant scheduling and shared resource admission. Local semaphores protect each process; Postgres-fenced reservations protect deployment-wide limits. Existing Forge scheduling supplies a precedent for active-per-tenant accounting. Large fan-outs must yield between batches so one tenant cannot occupy every worker.

   Across `WYRD_TARGET` replicas, retain the existing placement/routing model. Capable runners claim shared durable work; API replicas need not retain executions. Workers hold database connections only for short coordination transactions. Scaling replicas must not multiply tenant quotas.

   Failed uploads can leave unreferenced chunks for garbage collection. Failed membership commits are safely retried. Worker death reclaims leases and resumes checkpoints. A stale worker cannot settle work.

   Keep all internal Scribe publication on `ScribeOutbox`. Its analytical projection must not become the only durable execution record. Bounded outbox behaviour requires an explicit amendment to today’s unbounded-memory policy; audit remains non-blocking, with failed retention logged and counted.

5. **Proposed wire changes**

   | Surface | Proposed delta |
   |---|---|
   | Session resource | Create, append, close and inspect; tenant-scoped IDs, idempotency keys, acceptance receipts and immutable sealed-manifest reference. |
   | Input declarations / slots | Add an immutable evidence-manifest declaration and a typed collection slot. A session resolves to that declaration before evaluation; avoid a special conversation-only slot. |
   | `VerifierSpec` | Retain a pure acyclic computation. Add bounded execution-budget ceilings where they affect authored semantics; deployment policy may tighten them. |
   | Step | Add `each` with a pinned collection selector, maximum item count and requested concurrency ceiling. Existing actions remain item operations. |
   | Checks / decision | Extend existing aggregate facilities with explicit coverage and missing-member semantics. Start with all/any/pass-rate; add quantiles only with a demonstrated score contract. |
   | Binding | Add explicit activation scope and stable binding identity so contribution and sealed-session checks can coexist without ambiguity. |
   | Trigger | Extend activation conditions to contribution acceptance and session sealing. Idle/max-age closure belongs to session lifecycle policy; Trigger activates verification after closure. |
   | Execution status/result | Distinguish staged from durably accepted; expose progress, coverage, budget usage and termination reason. Durable result reference must exist before terminal settlement. |

   Keep provider credentials, queues, worker placement, mutable conversation history and arbitrary workflow definitions out of the Verifier Card. Keep Window/Drift for population and cross-session comparisons; a completed session is ordinary closed Run evidence.

   These are deliberate doctrine changes, especially durable acceptance/result guarantees, binding identity and bounded staging. They should not be presented as implementation details beneath unchanged contracts.

6. **Rejected alternatives**

   - **Streaming/stateful Verifiers:** mix evidence ownership with judgment recovery and make replay ambiguous.
   - **Trace-derived session identity:** forces tracing adoption and excludes other evidence producers.
   - **Ordinary Bifrost queries as closure proof:** current read semantics cannot establish completeness.
   - **Another queue product or workflow engine:** duplicates existing leased execution ownership.
   - **Loops in the DAG:** introduce unbounded execution; bounded collection expansion handles the stated need.
   - **Budget exhaustion means failed verification:** mistakes infrastructure limits for evidence of subject failure.
   - **Checkpoint every function:** adds coordination cost; checkpoint expensive or externally dependent work.
   - **Video extraction first:** creates expensive retries before recovery and resource accounting exist.

7. **First material owner decision**

   **What failure domain must a durable acceptance receipt survive: worker/pod loss, or node/disk loss too?** I recommend node/disk loss for the SaaS guarantee, assuming appropriately durable evidence storage and Postgres. Scribe’s pod-local WAL acknowledgement alone does not establish that guarantee.

   Resolve that first. Next, settle retention/deletion of sensitive session evidence, then whether human judgments count as reproducible evidence or explicitly attributable review. Those decisions affect storage and contracts more than naming the collection slot.

8. **Suggested cohesive sequence**

   1. Harden existing execution: bounded admission, durable acceptance semantics, retained terminal results and recovery of interrupted handoffs.
   2. Add immutable manifests and checkpoints; prove worker-crash recovery with an existing expensive action.
   3. Add source-independent sessions, closure races, late-evidence rules and activation.
   4. Add bounded fan-out, budgets and coverage-aware aggregation.
   5. Add portable media extraction/segmentation, then human review.

   Each increment should prove its client→server→client journey across Rust, Python and TypeScript, plus agent-facing HTTP/MCP where exposed. Include multi-replica recovery and overload rejection before claiming production capacity.

No files changed; tests and benchmarks were not run. Capacity figures are sizing examples, and the proposed durability guarantees exceed today’s documented behaviour.