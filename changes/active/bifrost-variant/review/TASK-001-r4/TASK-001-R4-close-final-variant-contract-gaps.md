---
id: TASK-001-R4
kind: remediation
status: ready
spec: SPEC-bifrost-variant
spec_revision: 11
requirements: [REQ-003, REQ-004, REQ-009, REQ-018, REQ-019, INV-002, INV-007, AC-003, AC-005]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-14, FIND-TASK-001-15, FIND-TASK-001-16, FIND-TASK-001-17, FIND-TASK-001-18, FIND-TASK-001-19, FIND-TASK-001-20, FIND-TASK-001-21]
---

# Close final TASK-001 Variant contract and evidence gaps

## Authority and immutable review subject

- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Reviewed base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Reviewed candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Reviewed tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Validated ledger: `changes/active/bifrost-variant/review/TASK-001-r4/findings-validation.md`

Implement this task with `$wyrd-implement`. Reassess the complete original
base-to-remediated-candidate range afterward; do not review only this
remediation diff.

## Outcome

TASK-001 retains its consolidated Variant, Oracle, built-in, SDK, and Iceberg
owners while closing the remaining trust-boundary and proof gaps. Raw Variant
input is bounded before dependency recursion, only the exact canonical Arrow
extension is admitted, locked failure precedence is deterministic, nullable
verification Structs cannot fabricate children, the CLI has its real journey,
and the tracked task packet describes the actual candidate. The dead rendering
wrapper is absent; the explicitly allowed review-policy edits are preserved.

## Issue diagnoses and required corrections

### FIND-TASK-001-14 — Null verification Structs fabricate child values

The verification-result producer builds absent `drift_report` and
`eval_summary` parents with null parent validity but valid placeholder children:
empty strings, encoded JSON null, and numeric zero. DataFusion 55 `get_field`
returns the child array without applying parent validity, and the Variant
placeholder guard cannot identify valid JSON-null bytes or primitive zeros.
Queries can therefore observe child values for a report that is absent.

Correct the existing verification-result producer. Under an absent parent,
emit null child slots using Arrow's native masked-null Struct semantics; use
the existing `VariantColumnBuilder` null path for `features` and nullable
primitive/string arrays for the other children. Preserve the declared public
non-null child schema, present-row values, DataFusion `get_field`, and the
shared Variant placeholder guard used by other storage inputs. Do not add a
read-normalization layer or custom Struct operator.

### FIND-TASK-001-15 — Predeclared built-ins violate locked error precedence

The predeclared table validator traverses declared Variant values before the
later fingerprint establishes the complete undeclared, missing, ordered, and
non-Variant wire schema. A request combining an earlier schema defect with a
later malformed, over-depth, or oversized Variant therefore returns the wrong
stable error. Canonical signal validation already demonstrates the required
phase ordering.

Extend the existing table-owned predeclared validator to establish the
complete declared user schema in logical-field order and return the existing
undeclared or unsupported errors before calling the shared recursive Variant
value validator. Keep the later fingerprint as the physical identity fence.
Do not add a second validator type, table-name switch, or downstream guard.

### FIND-TASK-001-16 — Raw depth is checked after recursive full validation

`EncodedVariant::from_bytes` checks size, calls upstream `Variant::try_new`,
and only then checks the Wyrd depth limit. The pinned dependency recursively
fully validates list/object children, while a very deep one-child structure
remains far below the byte ceiling. A raw built-in Arrow request can therefore
exhaust the process stack before Wyrd returns the required depth error.

At the existing `EncodedVariant::from_bytes` owner, keep the byte limit first,
then iteratively preflight depth through the installed Variant
representation's shallow accessors and stop at depth 65. Contain documented
malformed shallow-access panics with the repository's standard-library unwind
boundary and map them to the existing invalid-JSON violation. Only after the
bounded preflight passes should upstream `Variant::try_new` perform full
validity checking. Upstream remains the encoding authority; do not add a
byte-format parser, second Variant model, dependency, or configurable limit.

### FIND-TASK-001-17 — Foreign extension metadata is accepted and erased

The shared `is_variant` predicate checks only `ARROW:extension:name`.
Arrow-to-wire conversion then normalizes a correct-name field with foreign
extension metadata into canonical Variant and erases the mismatch. The
dependency intentionally ignores supplied metadata, so it does not enforce the
Bifrost wire contract.

Make the existing shared predicate require both `VariantType::NAME` and exact
empty extension metadata. Continue using this one owner from Arrow-to-wire
conversion and recursive admission at every nesting level. Add no sibling
validator.

### FIND-TASK-001-18 — Numeric/depth compound failures depend on key order

The sole raw-token walker returns immediately when it reaches either a depth
or numeric violation, and objects are traversed in sorted-key order. A JSON
object containing an out-of-range integer and a depth-65 branch can therefore
return either public error based only on key names, although numeric range is
locked before depth.

Keep `EncodedVariant::from_json_text` and its raw-token walker as the only
conversion owner. Retain a depth violation while continuing the bounded
traversal necessary to find a higher-priority numeric violation, then select
numeric before depth independently of object order. Do not add another parser,
Variant type, or limit. The rejected oversize-plus-depth claim is not part of
this remediation.

### FIND-TASK-001-19 — CLI Variant rendering lacks a journey

The compiled CLI now installs `VariantJsonEncoderFactory`, but its existing
server journey selects only primitive columns. SDK and MCP journeys cannot
prove CLI metadata propagation, binary wiring, or JSONL spelling, so a CLI-only
regression could emit the physical storage Struct or collapse `3.0` to `3`.

Extend the existing compiled-binary `query_server_journey` using its current
server and credential helpers. Query a Variant object containing `3.0` and
`u64::MAX`; assert native object output, exact integer digits, and raw `3.0`
spelling. Add no harness, fixture framework, or CLI-specific encoder.

### FIND-TASK-001-20 — Dead public rendering wrapper

`EncodedVariant::to_json` delegates directly to the existing
`variant_bytes_to_json` owner and has only two same-module unit-test callers.
Production consumers already use the byte renderer because they hold metadata
and value buffers separately. The public method adds a second owner and API
surface without a capability.

Delete `EncodedVariant::to_json` and update its two unit-test assertions to
call `variant_bytes_to_json(encoded.metadata(), encoded.value())`. Add no
replacement method, trait, or new test.

### FIND-TASK-001-21 — Final task evidence is factually stale

The tracked task front matter binds revision 11 while its authority link says
revision 10; its final evidence records compaction revision `94db7b94...` while
the candidate pins `2b65fa189f2d05002acc6e59515a071a63777970`; and its
placeholder diagnosis names nonexistent `wyrd_queue::variant::is_placeholder`
instead of the actual logic in `mask_placeholders`. Independent r4 proof makes
this an evidence-integrity defect rather than an unresolved lineage defect.

Update the existing task record once: name revision 11, the actual
`mask_placeholders` owner and behavior, and the tested/pinned final compaction
SHA. Attribute the final-pin fork proof accurately and do not imply the broader
Postgres V10 journey was rerun at the final pin if it was not. Do not create a
second evidence artifact.

## Constraints and preserved behavior

- Preserve exact i64/u64 integer meaning, doubles including `3.0`, duplicate-key
  last-wins behavior, null-versus-missing semantics, limits, catalog codes, and
  the logical fingerprint tag `0x0d`.
- Preserve one Variant model based on the installed Arrow 59.3 Variant crates;
  upstream remains the full encoding/validity authority.
- Preserve table-owned built-in validation before ACK/WAL and the existing
  catalog error identities; only phase ordering and exact extension identity
  change.
- Preserve the public verification Struct schemas, present values, sensitive
  classification, audit hash inputs, promoted values, and canonical producers.
- Preserve Struct `get_field`, semantic `variant_get`, all Oracle session
  registration, late interactive/distributed error identity, and SDK
  no-partial-result behavior.
- Preserve Iceberg v3 creation, row lineage, final fork pins, v3 GC, the
  five-field Forge handoff, and native Parquet Bloom behavior.
- Keep `wyrd-spec` IO-free and PyO3-free; keep DataFusion, Parquet, and Iceberg
  out of client-tier crates.
- Make no security/planning change for the rejected `SEC-R4-001` proposal.

## Non-goals

- No shredding, leaf projection/pruning, TASK-002 authoring behavior,
  DataFusion repin, migration, compatibility alias, signing, new public wire
  field, error code, configuration, dependency, feature, or validation
  framework.
- No second Variant parser/model, read-normalization layer, Struct SQL
  operator, CLI encoder, test harness, evidence artifact, or repository check.
- No unrelated Oracle, Scribe, Forge, SDK, documentation, or workflow-policy
  refactor.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-001-14` | Every child of absent `drift_report` and `eval_summary` reads as SQL null on hot and published rows; present summaries remain unchanged. |
| `FIND-TASK-001-15` | A predeclared built-in request with competing schema and Variant defects returns the locked earlier schema code, receives no ACK, and writes no durable row; isolated Variant defects retain their codes. |
| `FIND-TASK-001-16` | A compact hostile raw Variant deeper than 64 returns the exact depth error without process loss, after which the same server accepts a valid batch; malformed bytes remain typed errors. |
| `FIND-TASK-001-17` | Correct-name Variant fields with non-empty/foreign metadata are refused as `UnsupportedType` before ACK at top-level and nested built-in positions. |
| `FIND-TASK-001-18` | Numeric-range wins over depth with identical code/details in both object-key orders through direct conversion, Oracle `parse_json`, and one bounded pre-ACK write. |
| `FIND-TASK-001-19` | The existing compiled CLI journey proves native Variant JSON, exact `u64::MAX`, and raw `3.0` spelling. |
| `FIND-TASK-001-20` | `EncodedVariant::to_json` is absent; existing Variant rendering tests pass through the shared byte owner. |
| `FIND-TASK-001-21` | The task evidence names revision 11, an existing placeholder owner, and the exact compaction SHA present in both manifest and lock; the final-pin V13 proof passes. |

## Focused proof and broader verification

Run every new or changed exact test through `mise exec --` with its explicit
package, target, features, and exact expression. At minimum prove:

1. the existing verification real-server journey extended for absent/present
   Struct child projection;
2. the existing raw-IPC server journey extended for competing-error order,
   hostile depth with post-refusal server availability, and foreign extension
   metadata;
3. focused `wyrd-queue` direct compound-precedence and rendering tests;
4. focused Oracle `parse_json` compound-precedence tests;
5. the existing compiled CLI server journey through its repository-managed
   environment;
6. the exact pinned compaction V13 command at
   `2b65fa189f2d05002acc6e59515a071a63777970`;
7. `mise run fmt`, `mise run lints`, `mise run codegen:check`,
   `mise run check:skills-sync`, and `git diff --check`;
8. the original TASK-001 V1–V17 proofs whose owned source or behavior changed,
   plus the affected Rust/Python/TypeScript/MCP consumer journeys.

Do not replace exact focused selectors with a positional filter that can select
zero tests. Do not weaken, ignore, allow, or delete an existing gate to clear
this remediation.
