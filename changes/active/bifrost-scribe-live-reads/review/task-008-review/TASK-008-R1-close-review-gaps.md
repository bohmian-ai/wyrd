---
id: TASK-008-R1
title: Close cumulative declaration, documentation and proof gaps
kind: remediation
status: ready
spec: SPEC-bifrost-scribe-live-reads
spec_revision: 20
requirements: [REQ-014, REQ-015, AC-016, AC-017]
parent_task: TASK-008
remediates: [FIND-007-4, FIND-007-5, FIND-007-6]
depends_on: [TASK-007, TASK-008]
---

# TASK-008-R1: close independently validated review gaps

Route directly to `$wyrd-implement`. This is a bounded static/evidence correction under existing approved behavior.

## Subject and authority

- Approved spec: `changes/active/bifrost-scribe-live-reads/spec.md`, revision 20.
- Original tasks: `changes/active/bifrost-scribe-live-reads/tasks/TASK-008-tenant-proven-per-file.md` and `changes/active/bifrost-scribe-live-reads/tasks/TASK-007-one-parquet-scan-for-live-reads.md`.
- Prior remediation: `changes/active/bifrost-scribe-live-reads/review/task-007-review/TASK-007-R1-close-live-scan-gaps.md`.
- Candidate: `6e7add054e33701ca5ecb52a5c859948b15161a3`; immediate parent: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; original TASK-007 base: `a7582db587c6170a290760f1741673125612b797`.
- Diagnosis authority: this directory's `verdict.md` and `findings-validation.md`; exact independently executed proof: `verification.md`.

## Diagnoses and selected correction boundaries

### FIND-007-4: new declarations recreate the mandatory import violation

The original R1 declaration sites were corrected, but TASK-008 introduces qualified tenant parameters/fields and reader return types. Validated locations include `oracle/exec.rs:1111,1137–1138,1593`, `scribe/claim_assembly.rs:87`, `forge/managed/policy.rs:260`, `parquet/writer_properties.rs:121`, and the new test writer helper return at `oracle/exec.rs:3990`. Agent rules require all these declaration types to appear in the owning module's top import block and use bare names; fields and private/test declarations are included.

These interfaces are reachable: published loading and hot/staged readers call the footer helpers; bound hot sources call the metadata-key constructor; staging runtime supplies the assembly tenant; both Forge rewrite paths pass the table binding tenant into writer properties. The type identity is correct but the required dependency declaration shape is not. A reader inspecting the module dependency manifest cannot find the new domain/reader types there.

Use the existing module/test-module import blocks for the existing `DataTenantId`, `ArrowReaderMetadata` and `WriterProperties` types and bare names in the owned changed declarations. Preserve the concrete identities, feature gates and all runtime behavior. This is the source of the compliance gap; no wrapper, helper, new dependency or downstream guard is needed. Qualified value/constructor expressions remain permitted. Do not expand into unrelated old declaration cleanup.

### FIND-007-5: named acceptance proof still requires guessing

TASK-007 now provides improved old recipes and honest historical/deferred attribution. TASK-008's evidence at lines 65–72 still names writer, distributed, Forge promotion/rewrite and write/read journey checks while supplying only complete footer-unit commands, a module filter fragment, and a claim that exact journey expressions were used. New R1 partition/native proof exists in source but is not completely recorded in its implementation evidence. The packet cannot reproduce every named proof without selecting package, target, features and environment itself. This does not establish that historical runs failed or never happened.

The original TASK-007 server evidence also individually names three peer-service tests while supplying only a module selector; include those retained names in the exact-proof inventory. `findings-validation.md` confirms the TASK-008 journey identities span the `oracle`, `forge` and `scribe` targets, so one generic package/profile fragment cannot reproduce them.

Keep the existing tests and evidence owners. Add complete exact recipes for every specifically named test in the TASK-008 and R1 evidence, confirming names/targets from current source (and exact nextest listing if needed). Record authentic historical commands/outcomes when recoverable; label reconstructed current recipes and unavailable historical scenario records honestly. Record available RED/GREEN/verification attribution without inventing a RED run. Link or incorporate this review's genuinely executed partition/native checks as current independent evidence; do not claim they were implementation-time runs. Postgres journeys and full lanes stay explicitly deferred while the current restriction applies. No harness or production change closes this gap more directly than correcting the evidence itself.

### FIND-007-6: the changed schema proof omits its panic contract

The original native frame and follower-shape panic docs were fixed. The newly renamed/materially changed `schema/managed_columns.rs:48–49` test `with_managed_columns_appends_the_envelope_without_a_tenant_column` still has only a one-line outcome doc despite its schema-order/nullability/identity assertions. AGENTS §16 and agent rules explicitly require panic documentation on changed tests.

Add a substantive `# Panics` section describing that test's actual asserted envelope ordering, absence and nullability/identity conditions. Keep its assertions and runtime behavior intact. This belongs on the existing proof item, not in an unrelated test-doc sweep. No fabricated RED test or new runtime test is appropriate.

## Preserved behavior, constraints and non-goals

Preserve approved footer tenant binding/proof, no-row-column schema, stable field IDs, strict no-compatibility refusal, query-local terminal failure and leader audit, native Arrow transfer/counts/fingerprint, remote wire protocol, source leases, signed projection/filter, session partitions, admission, write ACK/WAL/publication/Forge behavior and shared storage governance. FIND-007-3 remains accepted unchanged. SQL `data_tenant_id` is excluded. Use the approved `WYRD_VALA_500_QUERY_TENANT_INVARIANT` code.

No new abstraction, source/decoder, proof cache, synchronization, dependency, feature, public API, migration or benchmark work. No commits. Do not run Postgres wrappers, full mise lanes or capacity benchmark under the current restriction. Set `CARGO_TARGET_DIR=$PWD/target-review` and `MISE_STATE_DIR=$PWD/.mise-review-state` for permitted verification. Do not weaken assertions or checks to obtain a pass.

## Acceptance and focused proof

1. **FIND-007-4:** validated new/materially changed declarations use module imports and bare types. Diff/import inspection proves identity and feature-gate preservation; scoped format/compile/Clippy proof passes without suppressions.
2. **FIND-007-5:** every named acceptance check has a complete exact recipe and truthful execution attribution; no placeholders or unsupported historical claims. Recovering or reconstructing recipes does not authorize prohibited execution. Current independent proof may be linked to `verification.md` with correct attribution.
3. **FIND-007-6:** the existing changed schema test documents its actual panic conditions and retains all assertions. Static doc/body inspection and scoped formatting suffice.

All corrections are static-only; do not manufacture a RED run or add tests to mirror imports/rustdoc. If a recipe needs independent permitted execution, use exact mise-pinned selectors. The already executed focused partition/native recipes and their results are in `verification.md`. For an affected schema proof rerun, use:

```sh
CARGO_TARGET_DIR=$PWD/target-review MISE_STATE_DIR=$PWD/.mise-review-state mise exec -- cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E 'test(=schema::managed_columns::tests::with_managed_columns_appends_the_envelope_without_a_tenant_column)'
```

Run scoped formatting and warnings-denied Clippy for touched redux source and `git diff --check`; do not broaden to full lanes. Full repository and environment-owning proof remains caller-owned when permitted. A later independent review reassesses the immutable cumulative candidate against both original tasks and this remediation.
