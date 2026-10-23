# Discovery claim comparison

Subject: base `4d0f7005888a899910008399d1b5e15e2a3207f9`, candidate `9e56fc340cb5fd769d3b34f47433e2719339fdfb`.

All seven independent discovery reports are present. This is routing and grouping, not source validation or a verdict. No independently accepted FIND IDs exist yet.

| Proposed common obligation/cause | Discovery claims |
| --- | --- |
| Custom schema composition changes resource/root/dialect semantics | BEH-001, INV-001 |
| TypeScript unknown parser exceptions escape normal SDK error conversion | BEH-002, INV-002, MNT-001 |
| Shallow handwritten data-contract check disagrees with reference/composition-aware compatibility | BEH-003, INV-003 |
| Code execution proof omits explicitly required malformed-output and exact-boundary cases | INV-004 |
| Payload-derived map keys appear in validator/decoder diagnostic paths | SEC-001 |
| New local Rust test type and field lack mandatory rustdoc | REPO-001 |

System and persistent-data reviewers propose no findings within their scopes. Those scoped results do not conflict with schema, diagnostics, local SDK conversion, declaration or proof claims. Both implementation reviewers acknowledge unrecoverable historical RED evidence and some documentation/proof limits; those observations need reconciliation, not automatic source findings.

## Follow-up decision

No fresh focused follow-up is required before validation. Discovery does not contain materially conflicting source claims or an unresolved, unreviewed reachable path. The suspected stdout limit was investigated and withdrawn after tracing its actual ownership (print capture, not the returned Verdict). Reviewers agree the narrower Bifrost authority limits elevated payload-read gating to gateway-call payloads; none proposes a new verification-result permission. The final validator must independently resolve the scope and safety of every remaining proposed finding, including unique claims and the dialect/composition manifestations, and reconcile all acceptance rows.

The first attempt's BLOCKED verdict records a candidate update, not a finding verdict. This attempt independently rechecks the full cumulative updated range. The final validator must use the updated committed source and inspect all seven updated reports, not rely on earlier report agreement.
