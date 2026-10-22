# TASK-001 acceptance verdict

**FIX_REQUIRED**

Three independently validated bounded findings remain. TASK-001 has not passed its completion gate. No specification revision is required; the corrections enforce approved behavior in existing owners.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry`.
- Base: `f38dff80327bb771975e2cdd8fbed283ab145159` (approved packet commit, before implementation).
- Candidate: `0a29bae4f4ff20e98d2c7937f3943877ac31bc62` (evidence commit).
- Reviewed range: `git diff f38dff803...0a29bae4f4ff20e98d2c7937f3943877ac31bc62`; 435 files, complete cumulative TASK-001 implementation and evidence.
- Approved authority: [specification revision 12](../../spec.md) and normative supplements.
- Original task: [TASK-001](../../tasks/TASK-001-flat-card-identity.md).
- Initial source was clean; candidate remained unchanged through discovery, validation and verdict. Only review reports and this remediation task were written. No source mutation, commit, merge, push or deployment occurred.
- First review; no prior findings or remediation closures.

## Independent review completeness

Every reviewer received the same [navigation map and immutable subject](subject.md), expanded it against source, and read the full cumulative range independently. Discovery agents did not receive sibling conclusions or an intended verdict. Separate fresh agents filled every role; the orchestrator reconciled the validated ledger without validating its own findings.

| Role | Report | Result | Proposed IDs |
|---|---|---|---|
| behavior-rev | [Behavior](task-review-behavior.md) | FAIL | BEH-001, BEH-002 |
| invariant-rev | [Invariants](task-review-invariants.md) | FAIL | INV-001, INV-002 |
| repo-rev | [Standards](standards-review.md) | FAIL | REPO-001 |
| maintainer-rev | [Maintainer](maintainer-review.md) | FAIL | MNT-001 |
| system-rev | [Resilience](system-review.md) | FAIL | SYSTEM-001 |
| domain-rev, persistent data/concurrency/durability | [Data](domain-review-data.md) | PASS | Empty |
| domain-rev, security/RBAC/tenant authorization | [Security](domain-review-security.md) | FAIL | SEC-001 |
| ponytail-rev, independent validation | [Validated ledger](findings-validation.md) | FIX_REQUIRED | FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3 |

[Claim comparison](claim-comparison.md) groups discovery by cause. No followup-rev was required: no material conflict, unreviewed reachable owner, or repeat-remediation uncertainty remained. The validator resolved the identified VersionListQuery scope question directly from base/current source. It rejected expanding strict-query policy to that endpoint and rejected requiring an artifact fixture for refusal-before-delete proof. Nonblocking wording/test-organization and unchanged-debt observations are excluded from this verdict's finding ledger.

## Reconciled acceptance matrix

PASS below means the independent source audit plus available recorded evidence supports the obligation; it does not claim runtime reproduction during this review.

| Obligation / preserved boundary | Source evidence | Verification evidence / limit | Result |
|---|---|---|---|
| REQ-018, AC-006: tenant/kind/name/version identity, exact CardUid | Flat migration; reference/row/query owners and RLS | Registry proof distinguishes kind and tenant; SQL 205/205 recorded | PASS |
| Version allocation, exact-pin conflict, replay/deduplication | Existing persist_node/resolve_existing, canonical hashes, tenant/kind/name line lock | Registry/declaration regression proofs inspected, recorded owner lanes | PASS |
| Relationship missing/foreign/inactive/stale-target refusal and exact UID assertions | Existing resolution and active FOR SHARE recheck; shared authorized read and client UID checks | Relationship/race/delete and HTTP negative proof source inspected | PASS |
| INV-001: tenant separation and permissions | Caller-derived tenant, TenantConn/forced RLS, existing grants/audits | Recorded SQL/server/SDK tenant and permission negatives; no widening found | PASS |
| REQ-019: flat metadata/reference/text and config | Strict CardRef/Metadata/config, flat display/parser, removed scalar defaults | Focused reference/config/loader and Python refusals recorded | PASS |
| AC-006: old HTTP selectors refused | By-ref query DTOs discard removed space before GET/DELETE | Existing HTTP proof omits the query-negative path | **FAIL: FIND-TASK-001-1** |
| REQ-032, AC-010: actionable stable HTTP validation | Bare declaration/list extractors bypass Wyrd mapper; strict by-ref adapters require same projection | HTTP proof permits generic client status/plain text | **FAIL: FIND-TASK-001-2** |
| REQ-020: no required replacement namespace; map precedence retained | Existing map defaults and flat identity owners | Config/loader map proof and source inspection | PASS |
| Loader-local paths, inline/external references | Existing path projection, slot visitors and typed server resolution | Loader/journey proof source and recorded results | PASS |
| Declaration atomicity, artifact integrity and recovery | Existing transaction, exact UID blobs, manifests and reconciler ownership | Fault/lifecycle test source inspected; no new crash experiment | PASS |
| REQ-027/029/031, AC-009: fresh schema, immutable applied migrations, no conversion/aliases | One forward migration, old routes removed, no dual reads/backfill/hash rewrite | Fresh fixture runs recorded; no populated-old-database upgrade claimed | PASS |
| Rust SDK public workflow | Shared Cards/state/pull owners, thin SDK | Focused 1/1 and wider recorded evidence; source inspected | PASS |
| Python public workflow/signature/stub parity | Native/shared owners and authored public stubs | Focused journeys/unit/type results recorded; kwargs diagnosis source-supported | PASS |
| TypeScript public workflow/native/declaration parity | Thin N-API/wrapper and generated types | Focused journey and type/native checks recorded | PASS |
| CLI/MCP/schema consumer closure | Flat flags/refs/routes/schema/guides through existing owners | Existing CLI/MCP proof source and recorded checks; HTTP exceptions above | PASS apart from HTTP findings |
| REQ-032, INV-006, AC-010: common server owner, consumer/architecture closure | Existing graph/defaults/registry/client/runtime workflows, examples, owning design and UI inspection | Codegen/examples/type/docs checks recorded; final docsite remains TASK-008 | PASS apart from HTTP findings |
| Repository changed-item rustdoc gate | Material changed helpers/DTOs/read and write owners omit required docs/Errors/Panics | Compiler/lint does not establish this gate; bounded inventory in validated ledger | **FAIL: FIND-TASK-001-3** |
| Non-goals and task boundaries | No account redesign, physical layout change, namespace substitute or compatibility machinery | Independent cumulative diff inspection | PASS |

## Validated finding ledger

The full source/producer/caller traces, individual discovery dispositions and correction inventory are preserved in [findings-validation.md](findings-validation.md). Only independently confirmed or revised findings are retained.

| Stable ID | Status / classification | Exact changed location | Consequence and selected correction |
|---|---|---|---|
| FIND-TASK-001-1 | REVISED / INCORRECT | `crates/wyrd/wyrd-server/src/components/cards/routes.rs:630,637`, receiving by-ref DTOs | Obsolete space selector can read or commit deletion of the flat Card. Reject unknown fields in these two DTOs; preserve service/SQL/lifecycle owners. Discovery BEH-001, INV-001, SYSTEM-001, SEC-001. |
| FIND-TASK-001-2 | REVISED / INCORRECT | Same routes, declaration/list/by-ref extractors at 425/305/142/614 | Removed-field refusals lack stable Wyrd problem responses. Use existing adapter rejection mapping and Validation/WYRD_SPEC_400_VALIDATION for data/query shape refusal; preserve syntax/content-type/body-limit responses and existing service-level nested-ref errors. Discovery BEH-002, INV-002; overlaps SEC-001 correction. |
| FIND-TASK-001-3 | REVISED / VIOLATION | Changed-item inventory in validation report, including loader parse, graph/authoring/CLI/server/SQL owners and test helpers | Explicit rustdoc merge gate unmet. Document the bounded material changed items in place, including accurate Errors/Panics/field/workflow contracts; no behavior or checker change. Discovery REPO-001, MNT-001. |

## Verification limits

The implementation task records passing focused spec/config/loader/registry/Rust/Python/TS/CLI/HTTP proofs and static/type/codegen/owner checks. Reviewers inspected what those tests prove and the source paths they exercise. These results were not independently reproduced. No Cargo build/test, live provider, database, crash injection or migration/reset ran during this review. `git diff --check` passed; candidate identity was repeatedly checked.

The full CLI/Rust SDK/Python integration failures remain recorded as OIDC discovery, absent production UI build and saved-login permissions limitations. Their causes were not independently reproduced or fixed, and identity_e2e remains compiled-but-unrun evidence. The concrete HTTP findings follow deterministic decode-to-response/commit paths and are not explained by those environment limits. No claim is made that every integration suite passed or that the fresh baseline supports populated pre-change upgrades.

## Handoff

One self-contained [TASK-001-R1 remediation](TASK-001-R1-close-http-refusals-and-rustdoc.md) packages all retained findings for `$wyrd-implement`, including diagnosis, minimal owners, preserved behavior, complete rustdoc inventory and focused closure proof. Review does not implement it. The next task review must retain this verdict/ledger/remediation and reassess original base through the new cumulative candidate.
