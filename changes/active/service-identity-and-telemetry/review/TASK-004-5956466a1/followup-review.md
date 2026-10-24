# Focused documentation authority follow-up

**RESOLVED.** Retain both B-3 documentation subclaims, with the narrow corrections below. This is source-only review of candidate `5956466a1299a99521ed9264b2a711d3f1957e3e` against prior candidate `f75ada758827266ac0b25a05d6a7b530f86d845a`; no behavioral finding or additional write set is proposed.

## TableVersion test panic sections

**Disposition: mandatory omission under the repository's unusually explicit rule, rather than an optional general Rustdoc preference.** Root `AGENTS.md:152–154` requires every new or changed Rust item to document `# Panics` when it can panic and makes missing rustdoc a merge blocker. The topic owner `agent_docs/rust.md:235–240` explicitly includes tests and states that possible panics have `# Panics`. Neither authority exempts ordinary test assertions.

At `crates/wyrd-spec/src/vala/ids.rs:575–585`, the named canonical-round-trip test contains `expect` calls and equality assertions. It panics when construction/serialization/deserialization unexpectedly fails, the canonical round trip differs, or the maximum has a successor. At `:592–612`, the rejection test contains assertions that panic when an invalid spelling or zero is accepted. These are actual failure paths present in the functions, not hypothetical production/parser panics. This conclusion does not claim that today's implementation fails those tests.

The R3 task's FIND-14 instruction (`review/TASK-004-f75ada758/TASK-004-R3-complete-version-boundaries.md:90`) asks for parser syntax/range errors and test intent/invariants, preserving assertions and adding “no imagined Panics.” That instruction limits invented panic claims; it does not expressly waive the higher repository rule for real assertion/expect paths. The prior validation at `findings-validation.md:103` likewise says “no speculative Panics.” The current remediation changes only rustdoc on these two tests, but they were themselves newly introduced by R2 and remain the exact items whose documentation FIND-14 requires completing. No unrelated older tests enter scope.

Smallest correction: append one `# Panics` section to each existing test's documentation. For the first: “Panics if a positive version fails construction or JSON round-trip, its wire spelling changes, or the maximum has a successor.” For the second: “Panics if a noncanonical JSON version or zero is accepted.” Preserve all code/assertions. The parser at `ids.rs:448–465` now accurately documents its real syntax/range refusal under `# Errors`; no parser `# Panics` omission is established.

## Displaced ingress helper documentation

**Disposition: confirmed mandatory omission caused directly by R3 insertion.** The newdiff inserts `Dispatched` and `unclaimed_retry` between the existing two-line `table_version_conflict` rustdoc and its function. Consequently `crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:921–923` all attaches to `Dispatched` at `:924`; `table_version_conflict` at `:961` has no attached rustdoc. This also makes the enum's opening description incorrectly call an outcome a typed refusal.

Smallest correction: move exactly the two existing lines at `:921–922` immediately above `table_version_conflict`. Leave “Outcome of one preparation and dispatch under one table version.” attached to `Dispatched`, together with its existing variant docs. No new helper or further section is necessary: the helper constructs and returns a `ScribeError` value; it does not itself return a fallible `Result`, so an `# Errors` section is not required for that construction.

No source edits, builds, tests, or unrelated discovery were performed. The remaining risk is documentation gate compliance only; this report does not establish a runtime failure.
