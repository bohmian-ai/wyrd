# TASK-001 r6 Security, RBAC, and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation under review: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

The candidate commit and tree matched the supplied immutable subject before
this review.

## Reviewed Boundary

This independent pass covered the cumulative TASK-001 security-sensitive
paths and the r5 remediation:

- canonical Arrow Variant input as an untrusted server boundary, including
  extension identity, size, numeric domain, depth, encoding validity, object
  shape, and refusal before shard dispatch, WAL append, ACK, or durable rows;
- complete-present/null-absent validation for sensitive nested Struct fields;
- public and resolved-object query RBAC, including the additional
  `vala.gateway.calls` payload-column permission;
- authenticated tenant propagation through catalog bindings, Scribe, Oracle,
  peer permission digests, scanned-file footer proofs, and distributed error
  settlement;
- stable local and distributed error identity, redaction of tenant/storage
  details, and failed-stream handling that cannot become partial success.

TASK-002 authoring and TASK-003 shredding/leaf-pushdown remain outside this
task and were not treated as missing work.

## Authority and Source Coverage

| Boundary | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| Identity, authorization, and tenancy | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md` | `AuthorizedQueryContext`, resolved table scopes, gateway payload gate, permission digest, Scribe tenant binding, peer claims, footer checks | PASS |
| Bifrost trust boundaries and failure semantics | `architecture/bifrost-design.md`; `architecture/references/domain/olap-serving.md` | built-in source-contract validation, Scribe prepare/dispatch order, Oracle local/distributed execution, query terminals | PASS except `SEC-R6-001` |
| Variant value contract | Spec REQ-004, REQ-011, REQ-019, INV-002, INV-004, INV-006, INV-007; task scenarios 1-3 | `EncodedVariant`, table-owned validators, raw-IPC journey, Oracle `parse_json`, result rendering | FAIL — `SEC-R6-001` |
| Stable public errors and no partial result | `architecture/references/languages/errors.md`; spec REQ-019 | `QueryCatalogError`, footer refusal mapping, query stream/client collection | PASS |
| Prior remediation evidence | r5 remediation packet, security report, validation ledger, root-cause ledger, and verdict | findings 14, 16, 18, 22, and 24 plus the shared-object addendum | PASS except the duplicate-key case below was not covered |

CodeGraph was used first to locate the current trust-boundary flow and callers.
The complete cumulative diff, the latest remediation range, applicable
authorities, current source, pinned `parquet-variant` 59.3 validation, and
recorded r5 evidence were then inspected. No other r6 report was read.

## Boundary Trace

1. Public query authorization still resolves tenant-qualified catalog objects
   and checks every `TableUid` before metadata, provider, physical-planning,
   admission, peer-dispatch, or source IO. One uncovered table refuses the
   whole query (`oracle/mod.rs:3880-3917`).
2. The gateway payload gate walks the optimized plan and its subqueries. A
   projection, filter, wildcard, or expression rooted in either sensitive
   payload column requires `gateway_payload_read`; denial precedes physical
   scan construction and returns no row (`oracle/mod.rs:3920-3985`). Variant
   and Struct accessors do not create a separate reader or permission path.
3. Tenant identity remains derived from the verified principal. The scoped
   permission digest binds the exact resolved table scopes, peer claims are
   compared with receiver-owned state, and every hot or published Parquet
   object proves its single footer tenant before row-group decoding
   (`oracle/mod.rs:3999-4024`; `oracle/exec.rs:1091-1163,1230-1270,3123-3155`).
   The public tenant-invariant error carries no foreign tenant, path, or
   provider diagnostic.
4. A raw Arrow batch reaches the built-in table validator inside `decode_rows`
   before preprocessing and before `try_send` transfers a prepared append to
   a shard. WAL append and ACK remain downstream of that transfer
   (`scribe/execution_lanes.rs:533-621`; `scribe/ingress.rs:552-624`). Schema
   mismatch is selected before Variant traversal, and valid-shaped batches run
   recursive Variant and whole-Struct checks.
5. The r5 Struct remediation correctly refuses a child whose validity differs
   from its nullable parent. `ResultsTable` applies it to `drift_report` and
   `eval_summary`, `CallsTable` to `resolved_model`, and Metrics to both bucket
   Structs. The gateway payload fields and verification summaries retain their
   sensitive classification.
6. `EncodedVariant::from_bytes` now bounds size first, catches shallow-accessor
   panics, performs an iterative node scan, enforces the revision-13 Decimal16
   domain, and avoids upstream recursion after a depth or malformed violation.
   The node-count ceiling prevents exponentially aliased field values from
   monopolizing the process. One object-validity case remains reachable:
   duplicate names through an unsorted metadata dictionary.
7. Catalogued local and worker failures still use `QueryCatalogError`; unknown
   dependency text collapses to `QueryExecutionFailed`. The terminal and SDK
   collectors refuse failed streams instead of returning preceding batches as
   a complete result.

## Material Proposed Finding

### SEC-R6-001 — Raw Variant objects with duplicate keys pass pre-ACK validation

- **Classification:** INCORRECT / data-integrity trust-boundary violation
- **Violated obligation:** Spec REQ-004 requires Variant object keys to be
  unique; INV-002 requires unsupported or ambiguous written content to be
  refused rather than stored; raw canonical Arrow must pass the same
  table-owned pre-ACK boundary as other built-in input.
- **Exact location:**
  `crates/shared/wyrd-queue/src/variant.rs:896-957`, specifically
  `scan_encoded` at lines 923-934; reachable from
  `EncodedVariant::from_bytes` at lines 230-246 and
  `validate_variant_values` at
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:391-415`.
- **Evidence:** `scan_encoded` rejects equal adjacent names only when
  `VariantMetadata::is_sorted()` is true. For unsorted metadata it tests
  `name < previous`, so equal names are accepted. The pinned
  `parquet-variant` 59.3 validator has the same gap: unsorted metadata validates
  offsets but not dictionary-name uniqueness, and object validation likewise
  rejects only a descending name. Therefore an authenticated raw Arrow writer
  can supply an unsorted dictionary containing the same string twice and an
  object whose two field IDs resolve to that string. The local scan finishes
  without a violation, `Variant::try_new` also succeeds, and the built-in
  validator transfers the batch toward WAL/ACK. Existing JSON conversion is
  unaffected because its `BTreeMap` deliberately resolves repeated JSON text
  keys to the final occurrence before encoding.
- **Observable consequence:** Bifrost can acknowledge and durably store a
  Variant object outside revision 13's value domain. Lookup, predicate, and
  JSON rendering consumers are then free to select different occurrences of
  the same logical key, so a sensitive nested payload can read or filter as a
  different value from the one rendered to another terminal. This does not
  bypass the whole-column payload permission, but it breaks the trusted
  evidence value after an authorized write.
- **Smallest testable correction:** At the existing `scan_encoded` owner,
  require strictly increasing resolved object names for both sorted and
  unsorted metadata; do not add another parser or validator. Preserve JSON
  last-key-wins before encoding. Add one focused `wyrd-queue` case using an
  unsorted duplicate-name dictionary and a real-server raw-IPC refusal case
  asserting `WYRD_VALA_400_VARIANT_INVALID_JSON`, no ACK/no retained row, and
  successful following work.

## Verification and Limits

Per review coordination, this pass started no Cargo, mise, codegen, database,
or multi-process job. It used source inspection and the recorded r5 evidence,
which reports the affected queue tests, raw-IPC server journeys, Oracle tests,
formatting, lints, and `git diff --check` green. The recorded shared-object
tests cover exponential aliasing but do not construct an unsorted metadata
dictionary with duplicate names, so they do not close `SEC-R6-001`.

Read-only checks in this pass confirmed the cumulative diff is whitespace-clean
and the supplied candidate/tree identity was unchanged before and after review.

## Status

**FAIL** — RBAC, tenant binding, footer enforcement, sensitive-column gating,
distributed error preservation, no-partial-result handling, and the new
whole-Struct checks remain sound, but the raw Arrow trust boundary still
admits a reachable duplicate-key Variant object before durable work
(`SEC-R6-001`).
