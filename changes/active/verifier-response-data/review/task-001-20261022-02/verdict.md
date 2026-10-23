# TASK-001 verdict

**FIX_REQUIRED** — six independently validated, bounded findings remain.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `4d0f7005888a899910008399d1b5e15e2a3207f9`.
- Candidate: `9e56fc340cb5fd769d3b34f47433e2719339fdfb`.
- Complete reviewed range: `git diff 4d0f7005888a899910008399d1b5e15e2a3207f9...9e56fc340cb5fd769d3b34f47433e2719339fdfb` (66 changed paths).
- Approved spec: [SPEC-verifier-response-data revision 1](../../spec.md).
- Original task: [TASK-001](../../tasks/TASK-001-verdict-formats-and-custom-data.md).
- No prior validated findings or remediation. Attempt 01 was [BLOCKED](../task-001-20261022-01/verdict.md) when the user committed the ty update. This attempt reviews the complete updated range, including that update.

The candidate remained unchanged throughout attempt 02. Existing local workflow edits are outside the subject and preserved. Review artifacts are the only additions from this attempt; no fixes, commits, pushes or deployments were performed.

## Independent results

| Role | Report | Result |
| --- | --- | --- |
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL: schema references, TS errors, handwritten contracts |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL: same shared causes plus required execution-proof gap |
| Repository standards | [standards-review.md](standards-review.md) | FAIL: mandatory rustdoc |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL: shared TS error mapping |
| System resilience | [system-review.md](system-review.md) | PASS within scope |
| Security/tenancy | [domain-review-security.md](domain-review-security.md) | FAIL: diagnostic payload-key disclosure |
| Persistent data | [domain-review-persistent-data.md](domain-review-persistent-data.md) | PASS within scope |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; six retained findings |

No focused follow-up was required. [Claim comparison](claim-comparison.md) groups shared causes and explains why scoped PASS results do not conflict with the proposed findings. The stdout-budget hypothesis was withdrawn after tracing print capture separately from returned values. Final validation independently checked every discovery ID, including unique claims, and consolidated duplicates. The orchestrator includes only that validator's confirmed/revised ledger.

## Reconciled acceptance

| Obligation | Source and available proof | Result |
| --- | --- | --- |
| REQ-001 / AC-001: generated plain scalar/object, native/portable and inline/referenced; authored form preserved | Existing compiler/Prompt resolver and state; named-format tests and plain SDK fixtures | PASS |
| REQ-001/002 / AC-003: exact custom schema, compatible handwritten contracts, refuse incompatible declarations before persistence | Resolver, bounded schema compatibility, declaration HTTP proof | FAIL: FIND-TASK-001-1, -3 |
| REQ-002: required non-null custom object; plain omission; preparation code unchanged | Closed Verdict, CodeCheck/CodeCall split, both executors | PASS direct forms; FAIL composed handwritten forms: -3 |
| Supported model/schema authoring and generic public surfaces | Existing coercion, TS schema conversion, stubs/generated types, Python authoring proof | PASS source; TS runtime authoring proof limited |
| REQ-007 / AC-004: full exact validation, inconclusive invalid outputs, rejected data absent, diagnostics safe | Shared verdict_from, executor/engine callers, negative tests | FAIL: -1, -4, -5 |
| REQ-007 / AC-005: inclusive size/depth, Unicode/escapes/wide arrays, real execution proof for both check kinds | Correct shared bound calculation, pure and Agent cases | FAIL required code proof/privacy: -4, -5 |
| Citation, aggregation, status/dispatch, actual and drift-detail semantics | Existing separate engine/citation/actual owners and regressions | PASS |
| REQ-011 / AC-002: whole/step, direct/queued/status, HTTP/MCP and published Bifrost readback | Judged -> outcome Variant -> existing Scribe/results/query owners; SDK/server/MCP journeys | PASS for accepted data, subject to schema finding |
| REQ-011 / AC-007: existing permissions/tenancy, retention/durability, no new store/column/log/audit projection | Existing authenticated TenantConn/query ownership; foreign-tenant and scoped-read proof | PASS access/storage; FAIL diagnostic projection: -4 |
| REQ-012 / AC-006: actual types; every typed operation/deadline; absent/failed/mixed data; no partial return or rerun | Shared Rust conversion and ordinary-operation reuse; Python/TS conversion and SDK proofs | PASS ordinary typed cases; FAIL error normalization/privacy: -2, -4 |
| Python positional-only arguments, data_type evidence slot, preserved identities/results | Native signatures, typechecks, typed/raw readback and invocation counts | PASS |
| AC-008 exports, generated contracts, authority/examples/docs | Source-owned wire/error types; authored stubs; recorded generation/docs checks; examples updated | PASS runtime surfaces; batch missing Rust/TS docs coverage as nonblocking correction |
| Required rustdoc | New local Judge type/score field undocumented | FAIL: -6 |
| Non-goals and existing-owner reuse | No new compiler/store/transport/lifecycle, options, dependencies/features, migration or decoder registry | PASS |
| Updated ty/auth consumers | Frozen full Python typecheck and three auth unit tests; source review of auth owner | PASS; prior .get typing limitation closed |
| Historical test-first execution | Scenarios 1–3 did not observe RED before implementation | Disclosed historical limit; cannot reconstruct with later tests |

Detailed obligation-to-source/test evidence and reconciliations are in [findings-validation.md](findings-validation.md#acceptance-reconciliation).

## Validated finding ledger

| Stable ID | Discovery / disposition | Classification and exact owner | Consequence and selected correction / closure |
| --- | --- | --- | --- |
| FIND-TASK-001-1 | BEH-001, INV-001 / CONFIRMED | INCORRECT; `crates/wyrd-spec/src/verification/execution.rs:598–620` | Embedding changes local refs/dialect, accepting invalid or rejecting valid data. Preserve the data schema's resource/root/dialect in the existing resolver; prove colliding refs and later-draft constraints on good/bad responses and both check paths. |
| FIND-TASK-001-2 | BEH-002, INV-002, MNT-001 / CONFIRMED | INCORRECT; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:281–292` | Nullish/malformed parser exceptions escape as TypeError. Normalize rejection once at typedResult using the existing error and safe root location; table-test generic throws, exact code, requirement and unchanged raw result. |
| FIND-TASK-001-3 | BEH-003, INV-003 / CONFIRMED | INCORRECT; `crates/wyrd-spec/src/card/verifier/validate.rs:583–585,789–797` | Shallow check admits optional data through composition and rejects valid referenced object data. Delete it; reuse bounded fits against canonical-derived closed plain/required-object shapes; prove composed/ref acceptance/refusal and no saved invalid Card. |
| FIND-TASK-001-4 | SEC-001 / REVISED | INCORRECT; `crates/vala/vala-verify/src/blocks.rs:201–205`; `crates/wyrd-spec/src/verification/execution.rs:515–522`; Python `src/verify.rs:340–365`; TS `src/index.ts:283–292` | Dynamic map keys leak through server reasons and SDK error paths. Server uses schema-owned/root location; generic decoders use safe data root and remove unsafe extraction. Test markers in nested map keys, serialized reasons/errors, data omission and ordinary reread. |
| FIND-TASK-001-5 | INV-004 / CONFIRMED | MISSING verification; `fixtures/cards/verifier_building_blocks/scored-review.yaml:96–111` and corresponding SDK invalid-output stories | Required hosted-code malformed/exact-boundary proof absent. Extend existing fixture/case tables, assert retained exact values and inconclusive/absent one-over/malformed values; run only affected focused selectors. No production bounds defect is asserted. |
| FIND-TASK-001-6 | REPO-001 / CONFIRMED | VIOLATION; `crates/wyrd-spec/src/verification/execution.rs:840–841` | Missing expressly required rustdoc on local test model/field. Add concise docs; static/format proof, no new test or separate style round. |

The complete source evidence, caller/writer traces, diagnosis and decision-complete recommendations for these IDs are preserved in [the independently validated ledger](findings-validation.md#validated-ledger-and-minimal-recommendations). There are no rejected claims to carry as optional advice and no prior accepted finding claimed closed.

## Authority and verification limits

Custom data inherits the existing outcome's query-read permission and tenant scoping. The narrower Bifrost authority makes sensitive metadata descriptive and adds payload permission only for gateway call payload columns. The spec's inherited-protection wording should be clarified against that owner; a new verification-result permission is neither required nor authorized. No material decision blocks the bounded fixes.

[Verification](verification.md) records the frozen full Python typecheck and cumulative whitespace check. Discovery also reran three frozen auth tests. Three focused Rust declaration/compiler/boundary tests passed in attempt 01 and their source is unchanged. The validator independently reproduced schema and TS failures with read-only/frozen tooling. Recorded implementation journeys/checks were inspected, not all rerun. Broad suites, crash injection, docs builds and aggregate gates were not repeated. Required missing code-execution proof remains FIND-TASK-001-5.

## Remediation handoff

Route [TASK-001-R1-preserve-schema-and-safe-data.md](TASK-001-R1-preserve-schema-and-safe-data.md) directly to `$wyrd-implement`. It batches all six validated findings and bounded documentation corrections under the same approved spec. A later task review must audit the full original-base-to-remediated-candidate range and these six stable IDs.
