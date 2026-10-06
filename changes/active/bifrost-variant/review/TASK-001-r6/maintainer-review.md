# TASK-001 r6 maintainer review

## Subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Authority: approved `bifrost-variant` specification revision 13, original
  TASK-001, the r1-r5 remediation packet and evidence, `AGENTS.md`, and the Wyrd
  and Bifrost design authorities.

## Coverage

This was a fresh cumulative maintainer review of the base-to-candidate diff. I
used CodeGraph first, then read the changed owners, their material callers, and
their tests across these seams:

- the `wyrd-queue` Variant owner: `EncodedVariant`, `VariantViolation`, the
  private `VariantViolations` precedence accumulator, JSON/raw iterative
  walkers, rendering, builders, and their unit tests;
- the built-in table registry and validation seam:
  `BuiltinTableDefinition`, `CanonicalBatchValidator`,
  `validate_declared_variants`, `validate_predeclared`,
  `refuse_partial_structs`, `DomainTable::WHOLE_STRUCTS`, and the gateway,
  metrics, verification-result, OTLP, eval, audit, and agent-trace declarations
  and projections that use them;
- the gateway capture, verification result, eval observation, and signal
  producer paths, including nullable-Struct construction and raw admission
  refusal coverage;
- Oracle's single Variant SQL registration owner, planner rewrite, scalar
  functions (including `parse_json`/`try_parse_json`), production session
  installation paths, typed error transport, and query tests;
- Rust, Python, TypeScript, HTTP/gRPC, and MCP result/error projections,
  including recursive native Variant conversion and generated declaration
  parity;
- catalog/Iceberg-v3 creation and validation, Parquet writer/Bloom sizing,
  Forge lineage-preserving rewrite/GC touchpoints, and their focused tests;
- the expanded real-server verification journeys, generated artifacts, the
  public schema guide, and the active Bifrost architecture document.

The implementation is otherwise cohesive at the reviewed seams: stateful work
has concrete owners, the shared validators remove rather than duplicate policy,
the two iterative Variant walkers share one precedence selector, and the large
journey additions exercise cross-boundary behaviors rather than replacing them
with in-process fixtures.

Per review coordination, I did not start Cargo, mise, or code-generation jobs
in the shared checkout. I inspected source and generated-file parity and used
the final-candidate command results recorded in the r5 remediation evidence;
those commands were not independently rerun in this review.

## Findings

### MNT-R6-001 — The active Bifrost design authority omits two enforced persisted-contract rules

- **Changed locations:** `architecture/bifrost-design.md:144-164` and
  `architecture/bifrost-design.md:166-188`; the same stale error summary remains
  in `crates/vala/vala-bifrost-redux/src/tables/mod.rs:197-216`.
- **Governing rule:** `AGENTS.md` makes `architecture/bifrost-design.md` the
  authority for Bifrost ingest/query/storage behavior, requires materially
  modified Rust items to have accurate rustdoc, and requires documentation to
  remain synchronized with public and persisted contracts. Revision 13 locks
  raw Arrow Decimal16 to scale zero and `i64::MAX + 1..=u64::MAX`, with malformed
  input preceding numeric range and depth. Revision 12 locks physically nullable
  children plus all-or-none admission for optional persisted Structs.
- **Concrete maintenance cost:** the architecture document describes the JSON
  numeric mapping but says Scribe repeats only extension, size, encoding, and
  depth checks. It therefore omits the raw Decimal16 admission rule and its
  `invalid > numeric > depth` selection order that `EncodedVariant::from_bytes`
  now enforces. Its built-in schema summary also names the optional verification
  Structs without recording the physically-nullable-child/all-or-none rule, and
  does not record the same rule for metrics buckets or gateway
  `resolved_model`. The user schema guide has these details, but it is
  subordinate to the active architecture authority. A maintainer implementing
  or refactoring from the authority can legitimately remove the numeric scan,
  restore required Parquet leaves, or omit `WHOLE_STRUCTS`, recreating the r5
  data-corruption/admission defects while believing the design is satisfied.
  The local validator rustdoc reinforces the omission by promising only size,
  encoding, or depth failures even though it also returns numeric-range errors.
- **Smallest testable correction:** amend the existing Variant paragraph to
  state the raw Decimal16 domain and the full error-selection order; amend the
  built-in-schema bullets to state that optional persisted Struct children are
  physically nullable but present values are all-or-none for verification
  results, metrics buckets, and gateway `resolved_model`; and update
  `validate_declared_variants`'s `# Errors` text to include numeric range. This
  is documentation-only and is testable with the existing docs check plus a
  source review against the already-covered admission tests.

## Overall verdict

**FAIL**

The candidate's reviewed implementation structure is maintainable, but the
active subsystem authority and one immediate owner document a weaker contract
than the code and approved revision. Because those omissions cover the exact
persistence defects repaired in r5, they are material rather than editorial.

End-of-review identity check: candidate
`ef92074f057b0a240c54b4407d8f32cac1310921`, tree
`e3c59991994125dac41187f17de12935e2b45424`.
