# Discovery claim comparison

All seven required discovery reports are present. No intended verdict is supplied to validation. This table groups proposals by owning invariant, not by votes or location. They remain unvalidated proposals.

| Shared claim | Discovery source IDs | Owning boundary / validation question |
|---|---|---|
| Accepted provider observations fail to fence older reads when no generation is inserted | B1, I1, DATA-2 | Queue-owned ordering: first close/draft with zero rows, repeated deactivation, current-source replay. Preserve immutable frozen input timestamp/bytes; validate each claimed variant rather than assuming all are equivalent. |
| Empty selected binding policy bypasses next-source supersession | B2, I2 | Source transition must be independent of replacement fanout; keep no declaration-time sweep. Closely related to ordering, but distinct trigger and correction outcome. |
| PR GET 403 becomes positive no-work receipt, including rate limits | B3, SYS-006-1 | Provider read error classification -> consequential webhook ACK -> failed-delivery exclusion. Official protocol references are in both reports. |
| In-flight first admission commits after connection access loss | B4, SEC-006-1, DATA-1 | Installation-slot/revision authorization must order with reservation and existing cancellation, including replacement association. Source ordering alone cannot establish access authority. |
| New PR execution inspection lacks repository-read permission | SEC-006-2 | Applicable exact security authority and recorded origin -> shared retained inspection permission check. Validate existing permission semantics and correction scope; a unique finding is not proof or grounds for rejection. |
| Recovery scan ignores cancellation during provider work and consumes shared drain budget | SYS-006-2 | Auxiliary worker -> shared supervisor deadline -> durable Scribe/Bifrost drain. Validate materiality and safe cancellation boundary, preserving already-sent IO reconciliation. |
| Mandatory changed-item panic rustdoc incomplete | REPO-006-1 | Explicit repository gate includes test/binding helpers. Validate actual panic paths and bounded item inventory, without optional style requests. |
| New multi-column SQL tuples violate named mapping rule | REPO-006-2 | Explicit SQL repository rule versus materiality threshold. Current projections match; no runtime misdecode is alleged. Independently decide whether retained as mandatory constraint and whether test-helper extension is justified. |

Maintainer review independently found the complete existing queue/Scribe/SubmissionSink workflow reuse appropriate; no DRIFT proposal exists. No other reviewer claims to disprove the concrete failure paths above. Differences in suggested private ordering mechanics are alternatives for validation, not established behavior disagreement. The validator must select the smallest complete safe correction and reject any unapproved boundary change.

No followup-rev is selected at this stage: no material conflicting source claim or untraced common cause remains in discovery. All unique proposals go directly to independent source/materiality validation. If validation establishes an unresolved material path or introduces a new blocker, obtain separate focused review before retaining it.

Fresh source status after discovery: HEAD still efda8cb2c59c8206b52dbe1d92fcae1ba34be430; tracked source diff empty. Reports only are untracked in this review directory. Verification results are in verification.md.
