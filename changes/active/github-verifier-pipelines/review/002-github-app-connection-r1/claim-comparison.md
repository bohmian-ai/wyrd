# Discovery claim comparison

All seven selected discovery reports are available. These are proposals for independent source validation.

| Proposed violated behavior or rule | Discovery IDs |
|---|---|
| Token handoff may survive a committed access fence | B-001, INV-1, REP-GH-04, MNT-004, SYS-01, SEC-1, PG-1 |
| Stored provider identity may be interpreted against a different configured GitHub host, including the outstanding browser transaction seam raised by INV-2 | B-002, INV-2, REP-GH-03, MNT-003, SYS-03, SEC-2, PG-2 |
| A public RSA key passes startup configuration validation | B-003, MNT-001 |
| The OAuth exchange asks for a REST media type and expects JSON | B-004, SEC-3 |
| Delayed suspension can overwrite current provider state | INV-3, SYS-02, SEC-4 |
| A repository removal can miss an in-flight verified selection | PG-3 |
| Private, operator-configured GitHub Enterprise host fails production outbound screening | SYS-04 |
| A transaction-bearing browser setup response lacks the required cache directive | SEC-5 |
| HMAC is implemented locally despite an existing resolved dependency | MNT-002 |
| Changed Rust items lack required or accurate documentation | REP-GH-01, MNT-005 |
| SQL tests can succeed without exercising PostgreSQL | REP-GH-02, PG-4 |
| Real-server tests lack the required source markers | REP-GH-05 |
| Published setup commands lack connected proof | REP-GH-06 |
| Exact focused command and failure-diagnosis evidence is incomplete | REP-GH-07, B-005 |

One focused follow-up examines the overlapping webhook/association timing claims. In particular, first association has no existing slot to route to, while reconnect and suspension already share a stored revision. The review needs to establish which interleavings are material and whether one existing boundary can close them. The other proposals have concrete source locations and proceed directly to independent validation.

The follow-up resolved that uncertainty. It traced two separate existing-slot failure mechanisms and identified the pre-association routing limit as outside the established ordering guarantee. It reported no additional proposal. Independent validation receives the full source trace in [followup-review.md](followup-review.md).
