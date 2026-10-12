# TASK-001 r5 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Original base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Latest remediation range: `bb6ae8070e276c20011e675ba1a804f0356e51ea..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior remediation: `changes/active/bifrost-variant/review/TASK-001-r4/TASK-001-R4-close-final-variant-contract-gaps.md`

The complete cumulative base-to-candidate range, the round-4 remediation
range, all earlier TASK-001 review/remediation artifacts, the current source,
the pinned dependency behavior, and the available verification evidence were
reviewed. The candidate commit and tree remained unchanged throughout review;
the untracked r5 review directory is outside that tree.

## Independent review results

| Review | Result | Material outcome |
|---|---|---|
| Behavior | FAIL | Numeric-below-depth and malformed-below-depth precedence remain incomplete. |
| Invariants | FAIL | Numeric precedence, nullable-child semantic completeness, and active evidence remain incomplete. |
| Repository standards | FAIL | Unapproved `wyrd-implement` policy drift and stale active-task state remain. |
| Maintainer | FAIL | Required `requested_model` children were weakened and the R4 packet contradicts revision 12. |
| System resilience | PASS | No additional crash, recovery, cancellation, outage, or blast-radius finding. |
| Reuse | FAIL at discovery | Both proposals were independently rejected: neither TASK-002 mechanism is a duplicate in the immutable subject. |
| Variant / Arrow | FAIL | The two depth-precedence gaps remain. |
| Scribe ingest / IPC | PASS | Schema ordering, no-ACK/no-write behavior, and copied IPC handling passed this domain review. |
| Persisted schema / Iceberg | FAIL | `requested_model` incorrectly inherited nullable children; other durability paths passed. |
| OTLP / built-in tables | FAIL | Gateway schema weakening and incomplete nullable-child semantic validation remain. |
| SDK / CLI / MCP | PASS | Variant rendering and late terminal behavior passed. |
| Security / tenancy | PASS | No authorization, RBAC, tenant, or partial-row finding; prior `SEC-R4-001` remains rejected. |
| Mandatory root-cause follow-up | RESOLVED | Current proposals were grouped and a reachable raw Decimal16 authority gap was identified. |
| Structured Ponytail validation | FIX_REQUIRED after human decision | Seven stable findings retained; two reuse proposals rejected; revision 13 resolves the raw-decimal decision by refusing non-canonical decimals. |

All required reports are present in this directory. No discovery conclusion is
accepted merely by agreement; `findings-validation.md` is the independently
validated source of truth.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Iceberg v3 creation, hidden row-lineage preservation, Forge rewrite/GC, final fork pins, and native Bloom sizing | Candidate source, prior cumulative evidence, fresh persisted-schema review, and pinned fork proof agree. | PASS |
| Canonical Variant type, extension identity, fingerprint, built-in layouts, Oracle registration, and shared rendering | The cumulative implementation and focused tests establish the primary contract. | PASS, subject to the retained compound and decimal findings below |
| Exact accepted numeric meaning through every terminal | JSON i64/u64 and `3.0` paths pass, but raw non-canonical Decimal16 values are admitted instead of receiving revision 13's numeric-range refusal. | **FAIL — `FIND-TASK-001-24`** |
| Locked JSON numeric-before-depth order | The round-4 sibling-order repair stops at the first over-depth subtree and can miss a higher-priority numeric token inside it. | **FAIL — `FIND-TASK-001-18`** |
| Bounded malformed-byte validation before depth selection | The new preflight prevents recursive stack exhaustion, but returns depth before malformed encoding below the cap. | **FAIL — `FIND-TASK-001-16`** |
| Nullable built-in Structs preserve absent-parent nulls and complete present domain values | Producers and published reads cover valid states, but raw canonical Arrow admission accepts partial summary, bucket, and resolved-model states. | **FAIL — `FIND-TASK-001-14`** |
| Gateway requested/resolved model schemas preserve their distinct typed contracts | One shared helper makes children nullable for both fields although only `resolved_model` needs nullable persisted leaves. | **FAIL — `FIND-TASK-001-22`** |
| Built-in schema/Variant refusal before ACK/WAL | Undeclared/type/extension ordering and isolated Variant refusal pass; retained findings identify narrower value-order and semantic-shape gaps. | PASS except `FIND-TASK-001-14`, `-16`, and `-18` |
| Interactive/distributed late catalog errors and no partial collected results | Shared `QueryCatalogError`, HTTP/gRPC terminals, and SDK behavior passed independent source and focused test review. | PASS |
| Rust, Python, TypeScript, MCP, HTTP, gRPC, and compiled CLI Variant projection | Shared renderer and compiled CLI journey preserve native JSON, exact `u64::MAX`, and raw `3.0`. | PASS |
| Security, tenancy, authorization, and sensitive-column boundaries | Independent security review found no reachable bypass or broadened task obligation. | PASS |
| Active task/remediation evidence accurately identifies the candidate and authority | TASK-001 and R4 retain stale lifecycle/revision instructions and a deleted error-owner citation. | **FAIL — `FIND-TASK-001-21`** |
| Cumulative task scope contains only approved or explicitly excepted work | The earlier task-review skill edits remain an approved exception; later global `wyrd-implement` edits are not covered. | **FAIL — `FIND-TASK-001-23`** |
| No duplicate implementation owner | Raw-row preparation extends the existing `BatchBuilder`; the one Scribe implementation was explicitly directed. | PASS; reuse proposals rejected |

## Validated finding ledger

The full decision-complete ledger and closure proofs are in
`findings-validation.md`.

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-14` | REVISED / CONSOLIDATED | INCORRECT / REGRESSION | Keep nullable persisted children but enforce complete-present/null-absent summary, bucket, and resolved-model values through the existing table validator seam. |
| `FIND-TASK-001-16` | REVISED / CONSOLIDATED | INCORRECT | Validate the complete bounded raw encoding iteratively, selecting malformed encoding before retained depth without restoring recursive stack risk. |
| `FIND-TASK-001-18` | CONFIRMED / CONSOLIDATED | INCORRECT | Continue a non-building numeric scan below the first over-depth JSON container so numeric range remains higher priority. |
| `FIND-TASK-001-21` | REVISED / CONSOLIDATED | VIOLATION | Bind TASK-001 to revision 13, close R4 against revision 12, and make lifecycle, nullable-child instructions, and error-owner evidence factual. |
| `FIND-TASK-001-22` | CONFIRMED / CONSOLIDATED | REGRESSION | Restore non-null children for required `requested_model`; keep nullable children only for `resolved_model`. |
| `FIND-TASK-001-23` | CONFIRMED | VIOLATION / DRIFT | Remove only the unapproved mirrored `wyrd-implement` policy edits; retain the explicitly allowed task-review edits. |
| `FIND-TASK-001-24` | CONFIRMED / DECISION COMPLETE | INCORRECT | Enforce revision 13 at raw Arrow admission: accept only canonical scale-zero Decimal16 for `i64::MAX + 1..=u64::MAX`; refuse every other Decimal16 with the existing numeric-range error. |

`FIND-TASK-001-13` remains omitted as the explicitly approved rejected
scope finding. Findings 15, 17, 19, and 20 are closed by the round-4
remediation. Findings 14, 16, 18, and 21 retain their IDs because this round
found incomplete closure at the same roots. New findings begin at 22.

## Validated root-cause groups

1. `FIND-TASK-001-18` is the remaining descendant case at the existing JSON
   numeric-selection owner; the sibling-order symptom is closed.
2. `FIND-TASK-001-16` is the remaining compound case at the raw-byte safety
   owner; stack-exhaustion prevention is preserved.
3. `FIND-TASK-001-14` groups three remediation-induced semantic gaps created
   when persisted children became nullable: verification summaries, metric
   buckets, and gateway `resolved_model`.
4. `FIND-TASK-001-22` is a separate declaration regression: the required and
   optional gateway model fields incorrectly share one child-nullability shape.
5. `FIND-TASK-001-21` is one active-artifact lifecycle root across TASK-001 and
   the implemented R4 packet.
6. `FIND-TASK-001-23` is independent global workflow-policy scope drift.
7. `FIND-TASK-001-24` shares numeric-domain lineage with prior finding 11;
   revision 13 resolves it by applying the JSON exact-number domain to raw
   Arrow admission rather than adding an arbitrary-precision terminal.

The historical `FIND-TASK-001-5`/`-12` distributed-error root remains closed
by the general `QueryCatalogError` envelope. The two current reuse proposals
were rejected because no second owner exists in the immutable subject and the
Scribe adoption was explicitly human-directed.

## Prior-finding closure

| Prior findings | Closure |
|---|---|
| `FIND-TASK-001-1`, `-11` | Closed for approved JSON input: raw-token classification and the revision-11 i64/u64 domain preserve accepted JSON integers exactly. Finding 24 is a new raw-Arrow decimal authority gap. |
| `FIND-TASK-001-2` | Closed for the missing universal built-in trust boundary; current findings are narrower validation/order defects inside that boundary. |
| `FIND-TASK-001-3` through `-12` | Closed, including standard Iceberg lineage, architecture sync, general distributed error identity, invalid public helpers, Bloom ownership, documentation, and import/style corrections. |
| `FIND-TASK-001-13` | Rejected under the explicit user scope exception; no stable finding issued. |
| `FIND-TASK-001-14` | Absent-parent published child leakage is closed; semantic completeness of newly nullable children remains under the same root. |
| `FIND-TASK-001-15` | Closed: earlier schema checks complete before Variant traversal. |
| `FIND-TASK-001-16` | Hostile-depth stack exhaustion is closed; malformed/deep precedence remains. |
| `FIND-TASK-001-17` | Closed: canonical extension name and absent/empty metadata are required at every nesting level. |
| `FIND-TASK-001-18` | Sibling key-order dependence is closed; numeric content below the depth boundary remains. |
| `FIND-TASK-001-19`, `-20` | Closed: compiled CLI proof exists and the dead rendering wrapper is deleted. |
| `FIND-TASK-001-21` | Revision/pin/placeholder facts were partly corrected; active lifecycle/instruction and owner contradictions remain. |

## Verification evidence and limits

- Candidate/tree identity and cumulative/latest-remediation `git diff --check`
  passed repeatedly during independent review.
- Fresh focused queue, table, Scribe, Oracle, terminal/client, CLI, gateway,
  security, fork, and boundary checks passed where reviewers ran them.
- The implementation record reports the affected TASK-001 journeys and
  repository gates green.
- Existing proof does not exercise a numeric token below depth 64, malformed
  encoding below that boundary, partial nullable-child Structs,
  requested-model nested nullability, or raw high-precision Decimal16 terminal
  exactness.
- The final validator did not rerun the broad PostgreSQL, object-store,
  cross-language, or complete pinned-fork suites; their recorded green results
  do not close the source-proven gaps.

## Post-validation human decision

The user directed the review to choose the smallest recommendation from the
intended behavior and to issue a remediation packet. Approved spec revision 13
therefore applies the existing exact i64/u64/double user contract to canonical
Arrow input and refuses every other Decimal16 before ACK; it adds no new public
numeric representation.

## Verdict

**FIX_REQUIRED**

All seven retained findings are bounded and decision-complete in
`TASK-001-R5-close-remaining-variant-contract-gaps.md`, routed directly to
`$wyrd-implement`.
