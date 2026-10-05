# Scribe journey and staged-member retry recovery

- Change: `scribe-journey-retry-recovery`
- Specification: `SPEC-scribe-journey-retry-recovery`, approved revision 1
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Scribe retry now classifies existing local evidence before staging new work. A failure after staging preserves usable progress, while a failure before registration retains the authoritative source. Live retry and restart recovery apply the same evidence rules, reject incomplete or contradictory state, and keep acknowledged rows visible exactly once.

The private-peer journey reaches its intended valid HTTPS reachability failure rather than failing earlier on malformed setup. Retry preserves row and batch identity, does not duplicate WAL or published state, and leaves physical qualification behavior unchanged.

Acceptance closed through deterministic post-staging retry, pre-registration recovery, contradictory-evidence refusal, replay and restart equivalence, exactly-once visibility, private-peer failure qualification, and the existing Scribe production lanes. No new recovery subsystem or alternate durability authority was introduced; no material deviation remains.

Current owners and evidence:

- [Bifrost design](../../../architecture/bifrost-design.md).
- [Scribe implementation](../../../crates/vala/vala-bifrost-redux/src/scribe).
- [Retry and recovery journey](../../../crates/wyrd/wyrd-testing/tests/bifrost/scribe/source_boundary_recovery.rs) and [Scribe journey family](../../../crates/wyrd/wyrd-testing/tests/bifrost/scribe).
