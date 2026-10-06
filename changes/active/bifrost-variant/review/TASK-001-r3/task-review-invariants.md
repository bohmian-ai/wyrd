# TASK-001 r3 invariant review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation tasks: `review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md` and `review/TASK-001-r2/TASK-001-R2-exact-integers-and-late-errors.md`
- CodeGraph was unavailable because the repository has no `.codegraph/` index. I used the complete cumulative Git diff, repository search, direct source/caller tracing, and the pinned dependency checkout.

## Value and state traces

- JSON text enters `EncodedVariant::from_json_text`, is retained as `RawValue`, and reaches `raw_number_variant` without an integral `f64` conversion. Signed values use the integer widths, values from `i64::MAX + 1` through `u64::MAX` use the installed Variant decimal representation, and either integer outside the combined ranges is refused. `serde_json/arbitrary_precision` remains disabled.
- Built-in Arrow input reaches the shared prepared-row admission boundary, where declared Variant extension identity and recursive value validity are checked before queue reservation, ACK, or WAL mutation. Scribe repeats this at the server trust boundary. The built-in table declarations, signal projections, verification/gateway/audit producers, and Rust/Python/TypeScript/MCP result consumers use the same Variant/Struct layouts.
- Every production Oracle session reaches the shared Variant SQL registration before planning, codec work, provider construction, or execution. Local and peer-forwarded DataFusion failures reach `map_datafusion_error`; late failures then enter `failed_terminal_on_path`, which embeds the derive-backed `WyrdProblem`. HTTP and gRPC decode the same typed terminal, and `wyrd-client`, scheduled-query, and MCP consumers rebuild the catalog error through `from_problem` while rejecting the accumulated rows as a result.
- Iceberg creation and validation are v3-only. The pinned reader supplies hidden lineage by standard field IDs; the compaction path validates presence/type/null per batch and copies the columns unchanged. The Wyrd and pinned-fork proofs compare each logical fixture row with both hidden values across repeated rewrites. I found no duplicate-ID scan or optional-metrics gate in production or required proof.
- The prior r1/r2 implementation gaps are closed in executable source. The remaining failure is durable authority drift introduced when revision 11 changed two public contracts without updating the active Bifrost design authority.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001, REQ-002, INV-003, INV-006, AC-002: v3-only creation, standard hidden-lineage preservation, repeated rewrite, and v3 GC | `tables/mod.rs`, `managed_columns.rs`, Forge managed rewrite, and pinned `iceberg-compaction` use the standard v3 hidden fields and unchanged batch copy; `managed_rewrite.rs:1102-1561` and pinned `compaction/mod.rs:3289-3404` compare logical rows and lineage without duplicate-ID or metrics gates | Candidate record reports V10, V12, and V13 passing at the pinned revisions; source inspection confirms the r2 prohibited checks are absent | PASS |
| REQ-003, REQ-004, INV-002: one Variant representation, exact integer meaning, fixed limits, duplicate-key rule, and no narrowing | `wyrd-queue/src/variant.rs:143-199,581-688` owns validated construction and lexical numeric classification; accepted integers stop at `u64::MAX`; wide positive and negative integers return the stable range violation | `variant::tests::json_text_classifies_integers_from_their_tokens` rerun in this review: PASS; candidate V5-V8 evidence covers exact `u64::MAX` reconstruction through all required terminals | PASS |
| REQ-006-REQ-011, INV-001, INV-004, INV-005, INV-007, AC-001, AC-003: all named built-ins have the locked Variant/Struct schemas, producer closure, promotions, sensitivity, and queryable results | Cumulative table declarations and projections under `vala-bifrost-redux/src/tables`, producer changes in `wyrd-client`/`wyrd-server`, recursive Scribe validation, generated contracts, and SDK/MCP projections agree | Candidate record reports V1-V8, V14-V16, OTLP, observe, drift, and MCP journeys passing | PASS |
| REQ-017, AC-005, AC-008: all Oracle sessions use the one SQL owner; Struct stays `get_field`; Variant uses semantic `variant_get`; authorization precedes provider IO | `oracle/variant_sql.rs`, shared Oracle constructors, peer fingerprint/version binding, and sensitive-expression authorization preserve the required producer-to-consumer path | Candidate record reports V9 and the focused Variant operator/function contract passing | PASS |
| REQ-019 and r2 FIND-TASK-001-12: every late failure carries its complete catalog problem in Interactive and Analytical execution over HTTP/gRPC; SDK result collectors discard partial rows | `wyrd-spec/src/vala/api.rs:809-926`, `oracle/mod.rs:3674-3699`, `oracle/query_stream.rs:450-505`, `wyrd-tonic/query_conversion.rs:271-303,465-493,581-589`, and `wyrd-client/bifrost/query.rs:122-136,779-805` form one typed problem path; malformed terminal problems are protocol failures | Review reran `late_catalog_error_keeps_its_identity`, `late_failure_terminal_is_closed_and_non_success`, `failed_terminal_problem_round_trips`, and `failed_terminal_problem_rebuilds_its_catalog_error`: all PASS. Candidate record reports real Rust Interactive/Analytical and accepted Interactive-only Python/TypeScript journeys passing | PASS |
| REQ-005, AC-009: Bloom capacity comes from native parquet-rs row-group geometry with unchanged FPP/folding | `parquet/writer_properties.rs` has no Wyrd NDV duplicate or setter; both recipes retain native Bloom configuration | Candidate V11 evidence reports the focused resolved-properties test passing | PASS |
| Prior FIND-TASK-001-1 through FIND-TASK-001-12 close at their owning source without prohibited replacement mechanisms | r1 fixes remain present; r2 refuses integers outside i64/u64, removes lineage proof drift, and carries `WyrdProblem` on the existing terminal rather than a prose parser or fixed code list | Focused tests above pass; cumulative diff contains no `arbitrary_precision`, duplicate-ID scan, optional-lineage-metrics gate, second Variant model, compatibility alias, or path patch | PASS |
| Repository authority and public documentation describe the shipped revision-11 numeric and late-error contracts | The schema guide is current, but active authority `architecture/bifrost-design.md:149-152` still permits any integer fitting a Variant decimal and says any other number becomes a double; `:439-465` and `:698-715` do not record that every late catalog error is carried as the full problem and raised unchanged | Direct comparison with spec revision 11 REQ-004/REQ-019 and executable sources above | **FAIL — INV-R3-001** |
| Non-goals and human standing direction: no second reader/model, DataFusion repin, shredding pull-forward, duplicate/metrics lineage mechanism, arbitrary-precision JSON, or nonstandard check/setting/option | Cumulative diff and pinned manifests preserve these exclusions; the late-error wire reuses Wyrd's existing typed RFC 9457 problem rather than adding another error catalog | Source and manifest inspection | PASS |

## Proposed findings

### INV-R3-001 — Active Bifrost authority still specifies the superseded numeric contract and omits the complete late-error contract

- **Classification:** INCORRECT
- **Violated obligation:** `AGENTS.md` makes `architecture/bifrost-design.md` the active authority for Bifrost ingest/query behavior and requires public behavior and documentation to align with it. Revision 11 REQ-004 and REQ-019 bind refusal outside i64/u64 and the full catalog problem on every late failure. Prior `FIND-TASK-001-4` also required the active authority to match the shipped Variant and query-failure behavior.
- **Location:** `architecture/bifrost-design.md:149-152,439-465,698-715`.
- **Evidence:** The authority still says an integer outside signed 64-bit may be accepted whenever it fits a Variant decimal and that “any other number” becomes a double. Executable source instead accepts only i64/u64 integers and refuses `u64::MAX + 1` and `i64::MIN - 1`. The authority's query sections say only that Variant failures retain stable fields and list typed terminal outcomes; they do not state revision 11's public rule that the late terminal carries the full existing catalog problem for every catalog error, across both paths and transports, and that every SDK raises it unchanged while discarding partial results.
- **Producer-to-consumer consequence:** `architecture/bifrost-design.md` wins over the approved spec and implementation drift under repository rules. A maintainer following it can re-accept wide Decimal16 integers that the public terminals cannot represent exactly, or narrow the late terminal back to a code-specific/text protocol, reintroducing the two r2 defects even though current tests are green.
- **Required testable correction:** Update the existing storage/Variant paragraph to the revision-11 i64/u64 classification and explicit range refusal, and update the existing Variant SQL/read-terminal paragraphs to state that every late catalog failure carries the same full derive-backed problem as its pre-stream form across Interactive/Analytical HTTP/gRPC, with uncatalogued failures remaining `WYRD_VALA_500_QUERY_EXECUTION_FAILED` and SDK collectors discarding partial rows. Reuse those existing authority sections; add no file, checker, option, setting, compatibility surface, or new mechanism. Run `mise run docs:check`, `mise run check:docs`, and `git diff --check`.

## Prior-finding closure

| Stable finding | Result |
|---|---|
| FIND-TASK-001-1 | Closed by lexical `RawValue` classification without integral `f64`. |
| FIND-TASK-001-2 | Closed by recursive pre-ACK/WAL built-in Variant validation and exact typed errors. |
| FIND-TASK-001-3 | Closed in production and proof: standard v3 projection/copy remains; duplicate-ID and optional-metrics gates are absent. |
| FIND-TASK-001-4 | Reopened in a narrower revision-11 form as `INV-R3-001`: the schema guide is current, but the active Bifrost authority was not updated for the newly approved numeric and late-error contracts. |
| FIND-TASK-001-5 through FIND-TASK-001-10 | Closed; structured distributed errors, the private validated constructor, TypeScript documentation, native Bloom behavior, Rust documentation, and import style remain present. |
| FIND-TASK-001-11 | Closed by refusing every integer outside the i64/u64 union and proving exact `u64::MAX` output. |
| FIND-TASK-001-12 | Closed in implementation and journeys by the typed full-problem terminal; its missing durable authority statement is part of `INV-R3-001`. |

## Verification notes

- Directly rerun and passed: shared encoder exact-integer classification; Oracle exact integer/refusal; local and peer-forwarded late catalog identity; closed failed-terminal behavior; protobuf full-problem round trip; shared-client catalog reconstruction.
- `git diff --check` over the immutable base-to-candidate range passed.
- The candidate's implementation records report V1-V17 plus format, Rust/Python/TypeScript lints and type checks, codegen, documentation, and boundary lanes passing. I did not rerun the expensive Postgres-backed and multi-language suites; the accepted Python/TypeScript Interactive-only limit is not a finding because Rust V9 proves Analytical execution through the same `wyrd-client` reconstruction.
- Candidate stability was checked before writing this report and remained `555308ba14058ddc56102d2f925298ef43858175`.

## Overall result

**FAIL** — `INV-R3-001` is a bounded authority correction. Executable invariants and all prior implementation gaps otherwise satisfy TASK-001 and the binding human decisions.
