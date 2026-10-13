# TASK-001 r7 cross-round root-cause ledger

## Method

Every r7 proposal was traced from producer to consumer and compared with the
r1-r6 stable findings and remediation commits. “Shared root” is used only where
one correction at the same owner closes both symptoms; sharing a module or
subject is not enough.

## Current r7 proposals

| Proposal | Prior relationship | Decision | Source path | Root correction and symptoms closed |
|---|---|---|---|---|
| `BVR-R7-BEH-001`, `INV-R7-001`, `ORACLE-R7-001`, `SYS-R7-001`, `VARIANT-R7-002` | `FIND-TASK-001-18`; numeric/depth fixes `7c2c60f3e`, `5230a70ab`; R6 JSON correction `679502cc0` | **Shared root / incomplete closure.** | `from_json_text` -> `append_raw`; at depth 65 -> `scan_numbers`; each pending container is deserialized from its complete remaining `RawValue`. Buffered writes and synchronous Oracle call the same owner. | Replace repeated nested-suffix deserialization with one syntax-authoritative traversal at `EncodedVariant`; preserve exact token rules, last-key-wins, invalid/numeric/depth order, and all callers. Closes write-path CPU amplification, Oracle executor monopolization, and the R6 depth-classification remediation gap. |
| `VARIANT-R7-001` | `FIND-TASK-001-25`; R6 correction `679502cc0` | **Shared root / incomplete closure.** | `append_raw` records a numeric violation but appends a null; `from_json_text` sizes the resulting builder before selecting the violation. | At the JSON builder owner, record and skip a refused numeric member. This restores the approved actual-built-byte rule and closes manufactured size priority without inventing size for rejected content. |
| `VARIANT-R7-003` | `FIND-TASK-001-16`; R6 raw-range correction `679502cc0` | **Rejected as a code defect.** | `object_field_slots` sorts starts and binary-searches successors, so it is `O(f log f)` but remains iterative, size-bounded, and non-amplifying. | No production correction. The false linearity prose is retained separately under `STD-R7-001`. |
| `STD-R7-001` | `FIND-TASK-001-4`; authority correction `fec6d5cf4`, evidence record `6147cc617` | **Shared authority-truth root / reopened.** | Active Bifrost authority and R6 evidence say raw validation is linear; `object_field_slots` is `O(f log f)`. Revision-13 requires bounded processing, not a sorting-free implementation. | Correct the existing authority and R6 evidence to “iterative, size-bounded, and non-amplifying”; do not rewrite the safe simple algorithm to preserve accidental prose. |
| `REUSE-R7-001` | Adjacent to `FIND-TASK-001-16` and historical duplicate-wrapper `FIND-TASK-001-20`; introduced by `679502cc0` | **Independent remediation-induced reuse root.** | `variant_bytes_to_json` calls `EncodedVariant::validate`; that owner calls full `Variant::try_new`, then the renderer calls identical `Variant::try_new` again. FIND-16 required one gate but did not require discarding its validated value; FIND-20 deleted a different public wrapper. | Make the private borrowed validation owner return the validated dependency `Variant`; reuse it in the renderer and let non-rendering callers discard it. Retain panic containment and one renderer. |
| `BVR-R7-BEH-002` | `FIND-TASK-001-24`; raw numeric correction `679502cc0`, OTLP unit `df74821aa` | **Shared numeric-domain proof root / incomplete proof.** | OTLP projection reaches the shared non-finite refusal and record-local partial-success handling, but only the in-process log projector test exercises the new behavior. | Extend the existing log OTLP journey through collector response, publication, and query. This closes user-boundary ACK and sibling-persistence proof without another harness or per-signal implementation. |
| `SDK-R7-001` | Same history as `BVR-R7-BEH-002` | **Rejected contract conflict; consolidated only for proof.** | Revision 13's “same exact numeric domain as JSON,” the r6 validated decision, approved R6 packet, and active Bifrost authority all authorize finite-only raw Float/Double. The broad IEEE sentence does not override that narrower canonical-input rule. | No specification revision or preservation path. Its missing journey evidence is already covered by `BVR-R7-BEH-002`. |
| `PERSIST-R7-001` | Related invariant to `FIND-TASK-001-17`; extension fix `7c2c60f3e` | **Independent physical-reconciliation root.** | FIND-17 fixed incoming declared-field identity. Existing-table reconciliation instead drops field metadata through `field_shape_matches`/`arrow_type_shape_matches`, allowing Variant storage Struct to match atomic Variant. | At the existing recursive field comparator, require `is_variant` parity at every field while retaining approved aliases and field-ID exclusion. Closes top-level and nested physical mismatch acceptance. |
| `PERSIST-R7-002` | Historical lineage `FIND-TASK-001-3`; fork and journey commits through `9a65d5469` | **Shared lineage-proof history, newly identified omitted scenario; source defect not established.** | The exact original task requires missing-lineage injection/no commit. The current Wyrd journey proves only successful rewrites/GC; the fork unit proves only the predicate, not its placement before output/handoff/commit. | Add the specified negative phase to the existing managed-rewrite integration seam. Closes failure ordering, no-snapshot/no-handoff, and possible-output accounting proof; no production change unless the proof fails. |
| `STD-R7-002` | R6 evidence record `6147cc617`; historically related to evidence-factuality `FIND-TASK-001-21` but a different rule | **Independent verification-record root.** | Named new/modified Rust tests lack their mandatory exact focused commands; whole-crate and regex commands do not satisfy the repository's evidence rule. | Run and append exact selectors for every named changed Rust test; retain broader lanes. |
| `STD-R7-003` | R6 review artifact predates the remediation range checked by `6147cc617`; historically related to task-evidence truth, not runtime | **Independent cumulative-gate root.** | The full base-to-candidate `git diff --check` reports one extra blank line at EOF in r6 `standards-review.md`; the recorded command began at `b4ea01848`. | Delete the one blank line and record the full cumulative diff-check. |

## Prior stable findings

| Prior finding | Remediation history | R7 relationship and closure |
|---|---|---|
| `FIND-TASK-001-1` | `45b61e80c`, later range decision `e450046c6` | JSON token precision is closed. The deep-JSON traversal uses the same owner but does not reopen numeric representation. |
| `FIND-TASK-001-2` | `347747ee3`, `87b736e2e`, consolidation `dab737003` | Universal built-in admission remains closed. R7 issues are predicates, resource behavior, or proofs inside/after that boundary. |
| `FIND-TASK-001-3` | `fb2415572`, `83954156c`, fork pins through `9a65d5469` | Runtime lineage mechanism remains source-correct; `PERSIST-R7-002` reopens the historical proof root because the original mandated negative journey was never present. |
| `FIND-TASK-001-4` | `9e39241ac`, `79f60eec3`, later `fec6d5cf4` | Reopened only for `STD-R7-001`: R6 synchronized authority with a false linearity claim. The correction is wording, not a new validator algorithm. |
| `FIND-TASK-001-5` | `0181ff52a`, generalized by `929ce7771` | Closed with finding 12 by the general catalog envelope; no r7 relationship. |
| `FIND-TASK-001-6` | `963782b7d` | Validated construction remains closed. Returning a private borrowed validated dependency value for renderer reuse does not restore the deleted public invalid-state surface. |
| `FIND-TASK-001-7` | `963782b7d` | Closed TypeScript documentation placement; unrelated. |
| `FIND-TASK-001-8` | `86f570328` | Closed duplicate Bloom ownership; unrelated. |
| `FIND-TASK-001-9` | `0d562a891`, `111e7bdce` | Closed Rust documentation sweep; current authority wording is finding 4's semantic-truth root. |
| `FIND-TASK-001-10` | `1612506e2`, `b0acb1901` | Closed Rust declaration style; unrelated. |
| `FIND-TASK-001-11` | revision 11 `93979daa1`, correction `e450046c6`, journeys `8a3e63168` | Accepted i64/u64 terminal domain remains closed. The rejected `SDK-R7-001` does not reopen it. |
| `FIND-TASK-001-12` | terminal commits through generalized `929ce7771` | Closed with finding 5; Oracle r7 review confirms error identity remains intact. |
| `FIND-TASK-001-13` | Explicit user rejection | Remains omitted; no r7 proposal revives it. |
| `FIND-TASK-001-14` | `7c2c60f3e`, `0e37748f3`, validation `5230a70ab`, proof `8121e0405`/`db65d41cc` | Closed. R7 gateway/Scribe/persisted reviews confirm whole-or-absent behavior and its boundary/publication proof. |
| `FIND-TASK-001-15` | `7c2c60f3e` | Closed schema-before-value ordering; unrelated to JSON member sizing. |
| `FIND-TASK-001-16` | `7c2c60f3e`, `5230a70ab`, `ef92074f0`, final raw correction `679502cc0` | Shared-range amplification and renderer bypass are closed. `REUSE-R7-001` is a new duplicate created by the final gate; `VARIANT-R7-003` is rejected as a runtime reopening. |
| `FIND-TASK-001-17` | `7c2c60f3e` | Declared Arrow extension identity is closed. `PERSIST-R7-001` is independent because existing Iceberg-table reconciliation uses a separate comparator that discards field metadata. |
| `FIND-TASK-001-18` | `7c2c60f3e`, `5230a70ab`, R6 correction `679502cc0` | Reopened/consolidated by the deep-JSON proposals: classification is correct, but the below-depth numeric scan reparses nested suffixes and leaves reachable superlinear work. |
| `FIND-TASK-001-19` | r4 compiled CLI journey | Closed; no r7 CLI projection gap. |
| `FIND-TASK-001-20` | r4 deletion of `EncodedVariant::to_json` | Closed. `REUSE-R7-001` is a different same-path duplicate inside the retained private validation/renderer flow. |
| `FIND-TASK-001-21` | task corrections through `5230a70ab`, lifecycle correction in R6 | Active lifecycle state is closed. `STD-R7-002/-003` are new evidence-protocol defects rather than stale task facts, although they live in review artifacts. |
| `FIND-TASK-001-22` | separate gateway declarations `5230a70ab` | Closed; r7 gateway review passes. |
| `FIND-TASK-001-23` | unauthorized skill edits removed by `5230a70ab` | Closed; unrelated. |
| `FIND-TASK-001-24` | revision 13 `86e199d2b`, raw scan `5230a70ab`, complete finite-domain correction `679502cc0` | Runtime numeric-domain correction is closed. `BVR-R7-BEH-002` retains only the missing real OTLP journey; `SDK-R7-001`'s contract conflict is rejected. |
| `FIND-TASK-001-25` | identified in r6, corrected by `679502cc0` | Reopened by `VARIANT-R7-001`: size is now selected first, but rejected numeric placeholders incorrectly contribute to the bytes being sized, contrary to the same approved actual-built-byte rule. |
| `FIND-TASK-001-26` | identified in r6, corrected by `679502cc0` | Closed. Strictly increasing resolved names are enforced; no r7 proposal reopens duplicate-key semantics. |

## Consolidated correction roots

1. **JSON traversal:** one `EncodedVariant` traversal closes the duplicated
   deep-JSON CPU findings without per-caller guards.
2. **JSON actual-built size:** refused numeric members add no placeholder bytes,
   completing finding 25's precedence rule.
3. **Raw validation ownership:** return and reuse the already validated
   dependency value; do not validate it twice in the renderer.
4. **Physical Variant identity:** recursively preserve extension identity in
   existing-table reconciliation.
5. **Proof:** extend the existing OTLP-log and managed-rewrite journeys; add no
   harnesses or production hooks unless those proofs falsify source.
6. **Artifacts/evidence:** correct the accidental linearity claim, record exact
   test commands, and make the cumulative whitespace gate green.

No correction belongs in SDK-specific validators, Oracle timeout handling,
Scribe WAL/ACK ownership, the public Variant contract, RBAC, tenancy, or a new
parser/rendering abstraction. No specification revision is required.

## Result

**RESOLVED.** The r7 proposal union consolidates to the six root groups above;
the raw-object algorithm and OTLP contract-conflict claims are rejected in
their broader forms, while their authority/proof portions are retained at the
smallest existing owners.
