# TASK-001 r7 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Reviewed remediation: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

The candidate and tree remained unchanged throughout review. Review artifacts
were written only under `TASK-001-r7`.

## Independent review results

| Review | Result | Material result |
|---|---|---|
| Behavior | FAIL | Deep JSON performs quadratic synchronous work; OTLP non-finite refusal lacks a real journey. |
| Invariants | FAIL | The shared JSON owner does not satisfy the bounded-work invariant. |
| Repository standards | FAIL | Authority wording, exact command evidence, and cumulative whitespace gate fail. |
| Maintainer | PASS | No material maintainability finding. |
| System resilience | FAIL | Synchronous deep-JSON work can hold Oracle capacity beyond the query deadline. |
| Reuse | FAIL | The byte renderer repeats upstream validation already performed by the shared gate. |
| Variant/Arrow domain | FAIL | Deep traversal and rejected-number size accounting remain incorrect. |
| Oracle/DataFusion domain | FAIL | The shared deep-JSON defect is reachable through `parse_json`. |
| Scribe ingest domain | PASS | Admission, ACK ordering, durability, and recovery pass. |
| Persistent schema domain | FAIL | Physical reconciliation loses Variant identity and the required lineage failure proof is absent. |
| Security/tenancy domain | PASS | No material security, RBAC, tenancy, or hostile-input finding. |
| SDK parity domain | FAIL, narrowed | The proposed specification conflict was rejected; only the OTLP journey gap remains. |

The mandatory repeat-review follow-up completed in root-cause mode and returned
`RESOLVED`. The fresh structured Ponytail validation retained seven stable
findings across six correction roots, reused the five reopened prior IDs,
rejected the raw-scanner rewrite and OTLP specification-conflict claims, and
confirmed that no specification revision is required.

## Reconciled acceptance

| Obligation | Result | Evidence |
|---|---|---|
| Canonical Variant type, exact accepted values, extension checks, raw canonicality, and renderer safety | FAIL | Runtime canonicality is largely present, but reopened findings 18 and 25 plus new finding 27 leave bounded traversal, actual-built-byte precedence, and duplicate validation incomplete. |
| Typed built-in schemas and whole-present/null-absent Structs | PASS | Built-in declarations, validators, producers, gateway peer refusal, and hot/published null reads agree. |
| Oracle Variant SQL and terminal error transport | FAIL | SQL registration and exact terminal identity pass, but finding 27 is reachable synchronously through `parse_json`. |
| Iceberg v3, lineage preservation, GC, and Bloom sizing | FAIL | Success behavior passes; new finding 28 permits physical Variant identity drift and reopened finding 3 leaves the mandated failed-lineage/no-commit transition unproven. |
| Rust, Python, TypeScript, MCP, and CLI Variant projection | PASS | Finite JSON projection, exact `u64::MAX`/`3.0`, terminal error preservation, and partial-row discard pass. |
| User-journey proof | FAIL | Reopened finding 24 leaves the newly visible OTLP non-finite partial-success behavior below the required journey tier. |
| Active authority and implementation evidence | FAIL | Reopened finding 4 leaves false complexity wording; exact-command recording and the cumulative whitespace gate remain mandatory cleanup before completion. |
| Scope and non-goals | PASS | No new public API, configuration, migration, compatibility path, per-language validator, shredding, or finding-13 revival entered the candidate. |

## Validated findings

| Finding | Classification | Consolidated root |
|---|---|---|
| `FIND-TASK-001-18` | INCORRECT / RESOURCE SAFETY | One shared JSON traversal repeatedly reparses nested suffixes. |
| `FIND-TASK-001-25` | INCORRECT | Refused numeric members are represented as encoded placeholders before size selection. |
| `FIND-TASK-001-4` | MISSING / AUTHORITY DRIFT | Authority and evidence overstate the raw scanner's complexity. |
| `FIND-TASK-001-27` | VIOLATION / DUPLICATE MECHANISM | The byte renderer repeats full dependency validation. |
| `FIND-TASK-001-24` | MISSING EVIDENCE | OTLP non-finite record-local refusal lacks collector-to-query proof. |
| `FIND-TASK-001-28` | INCORRECT / DATA INTEGRITY | Existing-table reconciliation ignores Variant extension identity. |
| `FIND-TASK-001-3` | MISSING EVIDENCE | The required missing-lineage/no-commit transition is absent. |

The validated ledger is `findings-validation.md`; the cross-round trace is
`root-cause.md`. Findings 3, 4, 18, 24, and 25 retain their original IDs because
their earlier remediation or proof was incomplete. Findings 27 and 28 are the
only genuinely new defects. Exact-command recording and cumulative whitespace
remain required completion cleanup without separate finding IDs.

## Prior-finding closure

R6 closes findings 14, 16, 21, and 26 at their behavioral roots. Findings 3,
4, 18, 24, and 25 remain open only for the exact gaps recorded here. Finding 13
remains rejected by the user and is not revived.

## Verification limits

The implementation record reports 68 `wyrd-queue` tests, 847 Postgres-backed
Vala tests, focused Oracle/log/gateway tests, three journeys, and green format,
lint, codegen, skills-sync, and docs checks. Reviewers did not rerun the broad
runtime lanes. Static review confirmed that the full cumulative
`git diff --check` is red at `TASK-001-r6/standards-review.md:88`, and that the
R6 record lacks the required exact selectors for named changed Rust tests.

## Verdict

**FIX_REQUIRED**

The seven retained stable findings are consolidated into six root corrections in
`TASK-001-R7-close-json-identity-and-proof-gaps.md`. The packet is
decision-complete and routes directly to `$wyrd-implement`.
