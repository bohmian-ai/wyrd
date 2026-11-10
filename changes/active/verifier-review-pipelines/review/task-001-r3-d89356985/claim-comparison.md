# Claim comparison (for independent validation)

No intended verdict. Independent discovery reports are proposals, not retained findings. Source and authority remain unchanged. The orchestrator groups the causes below; the validator must independently trace every proposal and assess both materiality and correction burden.

| Shared cause / obligation | Discovery proposals | Disposition to validate |
|---|---|---|
| Canonical terminal production, durable custody and terminal recovery | INVAR-R3-17/26; SYS-R3-17; SEC-R3-01; closure report when complete | Preserve prior FIND17 for original producer/custody gaps; accepted-failure consumer regression may share same invariant/correction or merit a new ID only if genuinely distinct. Trace all writers and recovery consumer; do not create one finding per caller. |
| Final-attempt reconciliation under unavailable source or reserved capacity | SYS-R3-19; closure report | Prior FIND19 incomplete despite healthy recovery passing. Validate attempts and release/refund semantics, original deadline, no additional computation or new policy. |
| Frozen checkpoint byte charge omits variable settlement/outcome | INVAR-R3-22; MNT-R3-001 | Prior FIND22 preservation of accounting. Validate ordinary captured data producer and Outbox admission use; conservative accounting suffices, no new governor or inline encoding. |
| Mandatory surviving changed-item rustdoc | STD-R3-001 | Prior FIND23; inspect each item and actual gate; batch local docs with behavioral correction, no style-only round. |
| Removed protobuf slot not reserved | STD-R3-002 | New local explicit R3 safe-retirement obligation. Validate obligation/materiality and minimum static reservation, no protocol capability or runtime test required. |
| SQL-head removal and exact accepted-prefix recovery | Data report empty finding ledger | Candidate uses existing generic fences, no replacement persisted execution head. Validate coverage including absence vs invisible exact revision. Preserve test assertion limits; do not invent new fencing. |

Follow-up not selected at discovery comparison: proposals converge on already assigned owner boundaries; a unique finding alone does not require another reviewer. The independent validator must request focused follow-up if a material conflict, untraced correction boundary or validator-only blocker remains.

Correction burden deserves particular scrutiny for terminal custody. A finding cannot prescribe a vague durable retry owner, invent a new finalizer/store/SQL payload authority, delay immediate revocation, or silently change cancellation semantics. Prove the smallest correction within current approved owners. If an unavoidable new material contract decision is required, name the precise conflict and route SPEC_REVISION_REQUIRED rather than author an invented remediation.

Authority exceptions: withdrawn FIND18 remains withdrawn; superseded stronger FIND20 is not reopened; rare stale-worker interval accepted. Successful direct final-record writes retain fast-return exception; separately governed checkpoint boundaries remain. TASK007 and new suspension/approval policy are excluded. Human-directed USER-PG-001/USER-DURABILITY-001 retain separate identities.

## Follow-up added during validation

The validator identified a material correction-boundary uncertainty for canonical cancellation custody. A fresh focused follow-up now exists at followup-review.md. It traces the commit-to-enqueue crash gap, all actual terminal producers and existing delivery owners, and the narrow prohibition on an execution event projection backlog. Its conclusion is an input to independent final validation, not an orchestrator-retained finding or approved mechanism. No new defect identity is allocated for this shared FIND17 cause.
