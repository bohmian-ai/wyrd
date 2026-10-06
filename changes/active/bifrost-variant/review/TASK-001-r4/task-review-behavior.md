# TASK-001 r4 behavior review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The review used the explicit base and candidate objects, not the later branch
tip. The candidate commit and tree resolve to the identities above. CodeGraph
was used first for the Variant, Oracle stream, shared-client, and server call
paths, then the full cumulative diff, owning source, callers, tests, pinned
fork identities, and recorded command evidence were inspected independently.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 — every Bifrost table is Iceberg v3 and non-v3 state is refused | `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1051,1095`; v3 GC is enabled in `src/forge/gc.rs:223-341` | Recorded V10/V13; repeated-rewrite journey source inspected | PASS |
| REQ-002 — both hidden lineage values survive repeated Forge rewrites without entering the logical schema or changing the five-field handoff | Standard metadata-field projection is pinned in `iceberg-compaction` revision `2b65fa189f2d05002acc6e59515a071a63777970`; Wyrd observes the two reserved fields in `tests/integration/forge/managed_rewrite.rs:1163-1243` and compares them after repeated rewrites | Recorded V10 and V13, including failure-before-commit and GC | PASS |
| REQ-003 — canonical Variant type, extension, and `0x0d` fingerprint | `wyrd-spec/src/vala/api.rs:279-295`; `wyrd-queue/src/variant.rs:400-432`; `vala-bifrost-redux/src/tables/mod.rs:800-878` | Fresh focused table-contract test passed; recorded V1/V12/codegen | PASS |
| REQ-004 — JSON meaning is retained, duplicate keys use the final occurrence, exact `i64`/`u64` values round-trip, and out-of-range integers are refused | One raw-token walker and upstream Variant model in `wyrd-queue/src/variant.rs:139-203,599-690`; one upstream JSON rendering path at `:229-333`; SQL rendering at `oracle/variant_sql.rs:384-516` | Fresh exact-number and JSON-rendering tests passed; recorded Rust/Python/TypeScript/MCP journeys | PASS |
| REQ-005 — Scribe and Forge Bloom filters derive capacity from row-group geometry while preserving FPP/folding | `vala-bifrost-redux/src/parquet/writer_properties.rs:127-147` leaves NDV to parquet-rs and sets the existing FPP only | Fresh `bloom_capacity_uses_row_group_limit_for_scribe_and_forge` passed; recorded V11 | PASS |
| REQ-006 — span attributes, nested event/link attributes, entity references, and promoted span fields use the locked typed schema | `tables/traces/spans.rs:28-97`; producer projections retain source values in `tables/traces/projection.rs` | Recorded trace OTLP journey and V1/V2/V4/V14/V15 | PASS |
| REQ-007 — log body/attributes and promoted log fields use the locked typed schema | `tables/logs/records.rs:29-68`; canonical projection remains table-owned | Recorded log OTLP journey and V1/V2/V4/V14/V15 | PASS |
| REQ-008 — metric attributes, exemplar attributes, typed metric structs/lists, and promoted service name use the locked schema | `tables/metrics/points.rs:25-134`; kind-specific validation remains in the table validator | Recorded metric OTLP journey and V1/V2/V4/V14/V15 | PASS |
| REQ-009 — `drift_report` and `eval_summary` have the exact persisted Struct shapes | `tables/verification/results.rs:33-102`; producers build the same children in `wyrd-server/src/verification/results.rs` | Fresh table-contract test passed; recorded V2 and SDK/MCP journeys | PASS |
| REQ-010 — eval, gateway, agent-trace, and audit open payloads are Variant with no legacy JSON-text column | `tables/eval/{observations,result_items}.rs`, `tables/gateway/calls.rs:23-27`, `tables/dev/agent_traces.rs:18-33`, and `tables/audit/audit_log.rs:39-42`; all producers use `VariantColumnBuilder` | Recorded V2, V5-V8, and consumer journeys | PASS |
| REQ-011 — OTLP and canonical Arrow producers remain equivalent and promoted values are copied from their canonical source | Signal projections use the same table ledgers and Variant owner; promotion source logic remains in `tables/{traces,logs,metrics}/projection.rs` | Recorded three OTLP journeys plus V14/V15; direct canonical-Arrow equivalence remains Rust-owned as the approved TASK-002 deferral states | PASS |
| REQ-017 — all production Oracle sessions expose one semantic Variant SQL surface; Struct remains `get_field` | `oracle/variant_sql.rs:59-146`; production installs at `oracle/mod.rs:3101`, `oracle/analytical.rs:540,7684`; peer/codec fingerprints include the version at `oracle/{codec,peer}.rs` | Fresh malformed-cell and catalog-error Oracle tests passed; recorded V9 operator/session matrix | PASS |
| REQ-019 — stable write/query failures retain exact catalog identity, including late distributed failures | Trust-boundary validation is table-owned in `tables/mod.rs:145-334`; the general distributed envelope is `oracle/mod.rs:4231-4308`; terminals carry `WyrdProblem` through `wyrd-spec`, tonic, and `wyrd-client/src/error.rs:139-229` | Fresh local/remote catalog and late-terminal tests passed; recorded V5-V9 and codegen | PASS |
| INV-001 — no built-in structured payload remains protobuf bytes or JSON text | Built-in schema ledgers listed above declare Struct/Variant; cumulative producer and consumer diff removes the replaced binary/text paths | Fresh V1; recorded V2-V8 and V14/V15 | PASS |
| INV-002 — no field/value is silently dropped, narrowed, or retyped | Exact integer classification and recursive Variant validation are in `wyrd-queue/src/variant.rs` and `tables/mod.rs:217-334`; undeclared and unsupported inputs are typed refusals | Fresh queue and Scribe-validator tests; recorded rejection journeys | PASS |
| INV-003 — compaction and v3 maintenance preserve logical values and hidden lineage | Wyrd leaves physical lineage to the pinned Iceberg owners and compares logical identity plus hidden values across two rewrites in `managed_rewrite.rs:1109-1559` | Recorded V10/V12/V13 | PASS |
| INV-004 — sensitive Variant/Struct leaves require the source column's permission before IO | Sensitive columns remain declared in each table ledger; Oracle authorizes resolved tables before provider/IO at `oracle/planner.rs:212-256` and retains logical-column gating in `oracle/mod.rs:3306-3308` | Recorded V9 sensitivity matrix and security journeys | PASS |
| INV-005 — every promoted value equals its canonical source or is null | Promotion code remains inside the canonical signal projections rather than a second post-processing path | Recorded trace/log/metric OTLP assertions | PASS |
| INV-006 — tenant derivation and query/physical tripwires remain unchanged | v3/Variant changes route through existing catalog, Scribe, Oracle, peer, and Forge owners; peer permission and tenant digests remain bound in `oracle/peer.rs` | Recorded multi-pod V9, v3 V10, and tenant/security evidence | PASS |
| INV-007 — Variant processing remains bounded by fixed depth/size and existing batch/request bounds | `VARIANT_MAX_DEPTH`/`VARIANT_MAX_ENCODED_BYTES` are fixed in `wyrd-spec/src/vala/api.rs:288-295`; encoding/validation enforce them in `wyrd-queue/src/variant.rs:139-203,575-690`; request-envelope admission still precedes decoded work | Fresh limit/validation tests; recorded raw-IPC pre-ACK journey | PASS |
| AC-001 — OTel Variant fields are queryable with typed values, absent-key nulls, and matching promotions across first-class SDKs | Canonical schemas/projections and shared `variant_get` implementation are wired end to end | Recorded V4, V5-V7, V14, and V15 | PASS |
| AC-002 — built-in and user tables are v3 and row lineage survives compaction/GC | Catalog v3 creation/refusal and repeated-rewrite journey cited under REQ-001/REQ-002 | Recorded V10/V13 | PASS |
| AC-003 — built-in Struct/Variant payloads are queryable in Rust, Python, TypeScript, and MCP | Shared native JSON rendering is `VariantJsonEncoderFactory`; client terminal deserialization is `wyrd-client/src/bifrost/facade.rs:891-930`; MCP uses the same encoder | Recorded V2 and V5-V8 | PASS |
| AC-005 — undeclared/unsupported/invalid writes refuse before queue/WAL and JSON SQL failures keep stable behavior | `validate_canonical_user_batch` checks undeclared/type/nullability before recursive Variant values; strict/lenient parsers share `EncodedVariant` at `oracle/variant_sql.rs:535-626` | Fresh Scribe-validator and parser/Variant tests; recorded raw-IPC and SDK refusal journeys | PASS |
| AC-008 — whole-column and derived sensitive reads are refused before provider IO | Authorization stays on logical roots before SQL execution; Variant lowering does not bypass it | Recorded V9 permission-before-follower-lease assertion | PASS |
| AC-009 — both writers use row-group Bloom geometry without a local duplicate capacity mechanism | `writer_properties.rs:127-147,316-330` | Fresh focused Bloom test passed | PASS |
| Constraint — `wyrd-spec` remains IO-free/PyO3-free; durable behavior stays in Rust owners; heavy analytical dependencies stay out of client tiers | Variant contracts remain in `wyrd-spec`, encoding in `wyrd-queue`, execution in Vala, and projections in SDKs; manifests add only the approved narrow Variant dependencies | Recorded client-tier/PyO3/codegen checks | PASS |
| Constraint — use the existing Arrow 59.3 Variant crates and narrow Iceberg forks; do not repin DataFusion in TASK-001 | Root manifest pins `parquet-variant* = 59.3`, Iceberg `e999331f...`, compaction `2b65fa1...`; DataFusion remains `55.0.0` | Manifest/lock inspection and recorded V12/V13 | PASS |
| Constraint — preserve audit hash inputs, fixed trace identifiers, public logical schemas, sensitivity, tenant tripwires, and the five-field Forge handoff | Audit verification recomputes through the canonical staging hash owner; changed schemas and producers are covered by the built-in ledger; Forge handoff shape is unchanged | Recorded V2/V10 and affected consumer journeys | PASS |
| Constraint — generated schemas/contracts are regenerated, not hand-edited | Contract source and generated schemas/proto/declarations move together | Recorded `mise run codegen:check` | PASS |
| Constraint — TASK-001 changes only the approved capability and contains no unrelated implementation/process change | Candidate also changes the repository-wide task-review process in both skill copies at `.agents/skills/wyrd-task-review/SKILL.md:47-74,208-333` and `.claude/skills/wyrd-task-review/SKILL.md:47-74,208-333`; those changes add a new mandatory reuse reviewer and repeat-review root-cause protocol unrelated to Variant storage/query | Cumulative diff and commits `a6f1326f6` / `a6429060f` | **FAIL — BVR-R4-BEH-001** |
| Non-goal — no migration or compatibility alias | No migration, legacy storage reader, compatibility route, or alternate wire model was added | Cumulative diff inspection | PASS |
| Non-goal — no shredding policy, physical-field declarations, nested-pruning facade, or DataFusion fork work | No DataFusion patch entries or TASK-003 physical planning implementation entered the candidate | Manifest and cumulative diff inspection | PASS |
| Non-goal — no user-model inference or TASK-002 authoring behavior | User-defined Variant authoring remains deferred; TASK-001 only adds the shared type/conversion substrate needed by its built-in surfaces | Cumulative diff and SDK journey inspection | PASS |
| Non-goal — no second Variant model, reader, planner, or ingest path | Encoding, validation, rendering, schema conversion, and SQL kernels reuse the installed Arrow Variant crates and existing owners | Fresh focused tests and cumulative source inspection | PASS |
| Non-goal — no signing, new configuration knob, or unrelated dependency | No Variant limit/config option, signing path, or unrelated runtime dependency was added | Manifest and configuration diff inspection | PASS |

## Proposed findings

### BVR-R4-BEH-001

- **Classification:** DRIFT
- **Violated obligation:** TASK-001 must contain no unrelated change; the
  approved scope and expected write set are limited to Variant storage/query,
  v3 lineage, built-in producers/consumers, SDK projections, generated
  contracts, documentation, and their tests. The task-review completion rule
  also requires that no unrelated change enter the cumulative diff.
- **Exact location:** `.agents/skills/wyrd-task-review/SKILL.md:47-74,208-333`
  and `.claude/skills/wyrd-task-review/SKILL.md:47-74,208-333` in commits
  `a6f1326f6` and `a6429060f`.
- **Evidence:** The candidate adds a mandatory `reuse-rev`, changes repeat
  reviews to require a root-cause follow-up, expands Ponytail validation, and
  changes verdict/remediation artifact requirements. These repository-wide
  review-process rules neither implement nor verify any TASK-001 acceptance
  criterion, and no Variant production or journey path consumes them.
- **Observable consequence:** Accepting TASK-001 would also ship a permanent
  repository review-policy change whose behavior and cost were never approved
  by SPEC-bifrost-variant. That makes the immutable task candidate broader than
  the approved change even though the Variant product behavior itself passes.
- **Required testable correction:** Remove the two mirrored task-review-skill
  changes from the TASK-001 candidate (restore both files to the base version)
  and carry any desired workflow-policy change in its own reviewed change.
  Prove closure with
  `git diff --exit-code 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..HEAD -- .agents/skills/wyrd-task-review/SKILL.md .claude/skills/wyrd-task-review/SKILL.md`
  plus the existing TASK-001 verification set. No production refactor or new
  test harness is needed.
- **Relationship to prior findings:** Independent. It shares no producer,
  value, or invariant with prior `FIND-TASK-001-1` through
  `FIND-TASK-001-12`; it was introduced while remediating/reviewing those
  findings rather than by their product-code correction sites.

## Verification performed and limits

- Candidate object check: `a6429060f^{tree}` is
  `64177d67141993ec18e63b43dc227dbc31d70950`.
- `git diff --check 80b33286e..a6429060f` passed.
- Fresh `wyrd-queue` focused run: exact-number conversion, contract limits,
  malformed stored Variant handling, native JSON rendering, and unsupported
  Arrow type refusal — 5 passed.
- Fresh `vala-bifrost-redux` focused run: built-in contract, local/remote
  catalog identity, late catalog identity, malformed Variant SQL handling,
  Bloom geometry, and canonical validator error preservation — 6 passed.
- The available final remediation evidence records V1-V17, the Rust/Python/
  TypeScript/MCP journeys, v3 repeated rewrite/GC, both fork tests, codegen,
  docs, formatting, lints, and touched-crate lanes as passing. Those expensive
  journeys were not rerun in this discovery pass.
- The current branch advanced after the immutable candidate to a commit that
  changes only `wyrd-implement` skill files. Source assessment and matrix
  references use the explicit candidate object; the reviewed candidate commit
  and tree did not change.

## Overall result

**FAIL**

The requested Variant/v3/query behavior is supported by source and credible
verification, but the immutable candidate contains the unrelated review-policy
changes in `BVR-R4-BEH-001`.
