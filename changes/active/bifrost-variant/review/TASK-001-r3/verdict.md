# TASK-001 r3 review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Candidate tree: `7508aea43a6e48877a594ce467eb03979166a5fa`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior remediation: `review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`
- Prior remediation: `review/TASK-001-r2/TASK-001-R2-exact-integers-and-late-errors.md`
- Review directory: `changes/active/bifrost-variant/review/TASK-001-r3/`

The complete cumulative base-to-candidate range was reviewed. `HEAD` remained
the candidate through discovery, follow-up, and independent validation. Review
artifacts are outside the reviewed commit and do not alter its source.
CodeGraph was unavailable because this repository has no `.codegraph/`
directory.

## Binding decisions applied

- Iceberg lineage uses only standard Iceberg v3 behavior. No duplicate-ID scan
  or optional-metrics gate is allowed in production or proof code.
- `serde_json/arbitrary_precision` stays disabled. Integral JSON tokens outside
  the `i64`/`u64` union are refused.
- Every catalogued late failure carries its complete catalog problem; only an
  uncatalogued failure becomes `WYRD_VALA_500_QUERY_EXECUTION_FAILED`.
- Python and TypeScript Interactive-only late-failure journeys are accepted;
  the Rust multi-pod journey owns distributed proof through the shared client.
- A mechanism, check, file, setting, or option absent both established
  standards and comparable widely used projects is drift and is not required
  as remediation.

## Independent review results

| Review | Result | Material proposal |
|---|---|---|
| Behavior | PASS | None |
| Invariants | FAIL | `INV-R3-001` |
| Repository standards | FAIL | `REPO-R3-1`, `REPO-R3-2` |
| Maintainer | FAIL | `MAINT-001`, `MAINT-002` |
| System resilience | FAIL | `SYS-001` |
| Variant/Arrow | PASS | None |
| Iceberg durability | PASS | None |
| Oracle/DataFusion | FAIL | `ODF-001` |
| Security/tenancy | PASS | None |
| SDK/transport parity | PASS | None |

Every required discovery report is present. No required reviewer or report was
unavailable.

## Follow-up decision

A focused follow-up was required because the behavior and SDK reports found no
late-error gap while the system and Oracle reports found a Variant-only remote
carrier; the reports also disagreed on architecture parity and the closure of
the prior import-rule finding.

`followup-review.md` resolved all three uncertainties. It confirmed the
Variant-only/prose carrier while narrowing out an unproven worker-timeout
example, confirmed that the active Bifrost authority contradicts revision 11,
and enumerated the remaining changed declarations that violate the import
rules. No disagreement remains unresolved.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and verification evidence | Result |
|---|---|---|
| Iceberg v3 creation, standard hidden-lineage preservation, repeated rewrites, and v3 maintenance | Production and proof paths use field-ID projection, bounded batch validation, unchanged lineage copy, and repeated-rewrite comparison. No duplicate-ID scan, optional-metrics gate, or replacement mechanism remains. | PASS |
| Exact Variant representation, admission, built-in persisted shapes, promotions, and integer range | Shared raw-token classification accepts exact `i64`/`u64` only; raw Arrow Variant values are validated before ACK/WAL; built-in producers and consumers agree; `arbitrary_precision` is absent. | PASS |
| Active architecture and public contract documentation | The schema guide matches revision 11, but the winning Bifrost authority still permits wider Variant decimals and omits the universal full-problem late-terminal rule. | **FAIL — `FIND-TASK-001-4`** |
| Oracle Variant SQL, Struct semantics, session registration, and local terminal behavior | Shared registration, `variant_get`, Struct `get_field`, local catalog mapping, public terminal, protobuf conversion, and shared-client reconstruction pass. | PASS |
| Every distributed late catalog failure preserves its complete problem without prose parsing or a family-specific branch | The worker transport reduces external errors to text. Oracle structurally reconstructs only four Variant errors and recognizes sibling catalog failures through message fragments; its own test requires another serialized catalog error to become generic. | **FAIL — `FIND-TASK-001-12`** |
| Rust/Python/TypeScript/MCP/HTTP/gRPC projections and accepted journey split | Downstream surfaces carry the selected full problem and discard partial rows. Interactive Python/TypeScript and Interactive/Analytical Rust evidence use the shared client as approved. | PASS, subject to the upstream distributed producer finding |
| Repository Rust import and declaration rules | Required rustdoc and most style remediation are present, but changed test functions still contain ordinary local imports and changed declarations still use qualified types in fields, signatures, returns, and bounds. | **FAIL — `FIND-TASK-001-10`** |
| Security, tenancy, audit, sensitivity, resource, and recovery boundaries | Permission-before-IO, tenant tripwires, pre-WAL admission, audit ownership, cancellation, and Forge recovery remain intact. | PASS |
| Non-goals and drift boundary | No second Variant model/reader, shredding work, migration, compatibility alias, duplicate lineage mechanism, new setting, checker, or unrelated public option entered the candidate. | PASS |

## Validated finding ledger

`findings-validation.md` is the independently validated source of truth.

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-4` | CONFIRMED | INCORRECT | Correct the existing Bifrost authority to the revision-11 `i64`/`u64` integer boundary and universal full-problem late-terminal contract. |
| `FIND-TASK-001-10` | REVISED | VIOLATION | Move the listed ordinary imports to their module blocks and use imported bare declaration types throughout the cumulative changed surface. |
| `FIND-TASK-001-12` | REVISED | VIOLATION / DRIFT | Replace the Variant-only serialized-text carrier and catalog message heuristics with one general structured catalog-error envelope over the existing distributed external-error boundary. |

No retained correction requires a new product, public API, architecture,
security, compatibility, cross-service, concurrency, resource-ownership, or
persistent-data decision. Revision 11 and existing repository mechanisms fully
decide the corrections.

## Prior-finding closure

| Stable finding | Result |
|---|---|
| `FIND-TASK-001-1`, `FIND-TASK-001-11` | Closed: exact `i64`/`u64` classification and adjacent refusal are implemented with `arbitrary_precision` off. |
| `FIND-TASK-001-2` | Closed: recursive built-in Variant validation occurs before ACK/WAL. |
| `FIND-TASK-001-3` | Closed: standard Iceberg v3 lineage behavior and proof contain no duplicate or metrics gate. |
| `FIND-TASK-001-4` | **Not closed:** the active authority contradicts or omits revision-11 behavior. |
| `FIND-TASK-001-5` | Closed for its revision-10 Variant-specific obligation; revision 11's universal distributed-error requirement remains under finding 12. |
| `FIND-TASK-001-6`–`FIND-TASK-001-9` | Closed. |
| `FIND-TASK-001-10` | **Not closed:** prohibited local imports and qualified declaration types remain. |
| `FIND-TASK-001-12` | **Not closed:** the terminal can carry a full problem, but the distributed producer reconstructs only Variant errors structurally. |

## Verification evidence and limits

Discovery reviewers independently reran focused exact-integer, Variant
admission, Oracle SQL/error, tonic conversion, shared-client reconstruction,
security/tenancy, and pinned Iceberg lineage tests; all reported passing.
`git diff --check` passed, and the candidate remained immutable. The original
task and R2 evidence record the full V1–V17, format, lint, language, codegen,
and journey lanes as passing on the candidate.

Those green checks do not close the retained source-confirmed gaps. The current
distributed journey exercises the special Variant carrier and an uncatalogued
generic failure, not a non-Variant worker catalog error. Existing lint and
format lanes do not enforce the explicit import-placement rule, and executable
tests do not make contradictory active authority correct. The accepted
Interactive-only Python/TypeScript limit is not a verification deficiency.

## Verdict

**FIX_REQUIRED**

Three bounded implementation findings remain. Remediation is specified in
`TASK-001-R3-close-authority-style-and-distributed-errors.md` and routes
directly to `$wyrd-implement`.
