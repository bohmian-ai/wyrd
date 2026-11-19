# Full review proposal comparison

All six selected discovery reports are present. Full subject and source/
authority context are recorded in review-scope.md. Findings remain proposals
until the mandatory separate fresh validator checks source and materiality.

| Shared cause | Discovery sources | Proposal and minimum boundary |
|---|---|---|
| TypeScript cold Window sample leaves caller-runtime producer scope before deferred enqueue | INV-FULL-001, MAINT-FULL-001, EV-FULL-1 | Capture the Node producer context at the existing staging/trace handoff and preserve it for eventual enqueue; keep callbacks on Node thread, caller context for startup, warm samples and existing single lifecycle. Existing action-timeline proof covers warm only; extend its actual cold+warm span/link assertion. |
| Published result-retention prose promises complete graceful drain, only abrupt-stop loss, and durable write within 50ms | BEH-001, SYS-1, REPO-FULL-001, EV-FULL-2 | Correct existing concept/FAQ/investigation and coupled generated text to actual staging/linger/retry/rejection/shutdown guarantees; no new outbox/durability implementation or tests mirroring prose. Reports distinguish reserved direct-result producers from unreserved shed paths. |
| Mandatory async/durable item rustdoc omits cancellation/partial-progress/retry | REPO-FULL-002 | Reviewer identifies start_gated and lifecycle/public flush documentation; independently validate exact item/rule/reopening basis and minimum local inherited docs, retaining stable FIND-2 only if warranted. Previous tuple-field/test and original five fixes remain closed. |

There is no unresolved material conflict between current reports, so no
followup-rev is selected merely to corroborate agreement or a unique gate
finding. The validator must independently trace all proposals, reject any
unsupported correction or scope expansion, reconcile prior FIND1–4 closure
under approved revision 7, and validate all specialist selections/omissions.
A newly discovered validator blocker requires separate focused source
validation before retention.

All relevant reviewers agree from source that new waits are SDK first-start
preparation, with warm samples staying synchronous and named server engine
request-wait owners unchanged. Agreement does not substitute for validator
proof of benchmark applicability. Exact cold/warm wait inventories remain in
the invariant/standards/system reports.
