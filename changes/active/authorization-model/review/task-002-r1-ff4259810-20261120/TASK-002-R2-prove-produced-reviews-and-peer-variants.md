---
id: TASK-002-R2
kind: remediation
status: proposed
spec: SPEC-authorization-model
spec_revision: 2
requirements: [REQ-006]
original_task: TASK-002
findings: [FIND-TASK-002-1, FIND-TASK-002-3]
---

# Prove produced reviews and peer Variant parity

Alice's Access-created grant now reaches payments review membership and retained
inputs through the required consumers. Finish that proof with actual produced
review results and evidence. Qualify the Oracle repair across its existing peer
codec, where serializing a field-typed cast currently loses the metadata used to
identify a Variant.

## Subject and authority

Approved contract: SPEC-authorization-model revision 2, supplied working-tree
`changes/active/authorization-model/spec.md`; original task:
`changes/active/authorization-model/tasks/TASK-002-access-ui.md`.
Prior remediation: `../task-002-29a9d276f-20261120/TASK-002-R1-complete-access-management-and-connected-proof.md`.
Reviewed remediation base: `29a9d276f5e668df56f5f16e898a73f7aefc46a0`.
Reviewed candidate: `ff42598100b0edabad5cad2272a3c7735f2eef14`.
Cumulative original base: `300542877cd2a58cbe5f061d5075616e4cc4397b`.
Validated gaps: [findings-validation.md](findings-validation.md).
Decision and carried-forward coverage: [verdict.md](verdict.md).

REQ-006/AC-001 and the original TASK-002 cycle 1 require connected access to
Repository review results and retained evidence. REQ-006's source-independent
visibility and `architecture/bifrost-design.md` §§ Variant SQL and distributed
execution govern peer Variant parity. Apply root/nested instructions and UI
skill where relevant. This task changes no product scope, lifecycle, migration,
or permission model.

## Complete connected produced-output proof — FIND-TASK-002-1

The existing Alice journey reads actual `execution_inputs`, member identity,
review membership, counts and pages, and independently refuses the referenced
Verifier. Those checks are useful and remain accepted. Its member explicitly
stays pending and no Verifier runs; empty result/evidence stores cannot prove
that Alice reads produced review outputs or that ledger outputs remain hidden.

Extend the existing production journey's deployment and PR/Verifier fixture
setup only enough to execute real payments and ledger review work and await its
materialized result and retained evidence. Reuse Alice's UI-created account,
binding, browser session and redeemed CLI credential. Read actual payments result
and evidence values as Alice, and verify that equivalent ledger outputs
contribute no visible item/count/page or point disclosure. Keep the already
proved browser/CLI/TS/Python/MCP identity seams and reuse their public consumers;
do not build another account/grant fixture or repeat unchanged backend suites.
Exercise the populated result/evidence consumer seam at the narrowest tier that
proves it, rather than multiplying equivalent assertions on every surface.

This remains an evidence correction. Production changes are required only if
the stronger test demonstrates a defect; diagnose and correct that defect at
its existing owner. Preserve sensitive-content checks and independent Card
read checks. Do not substitute raw input data or direct insertion of fabricated
result/evidence rows for actual produced outputs.

## Qualify field-typed masks through peers — FIND-TASK-002-3

`restrict_verification_rows` in `vala-bifrost-redux/src/oracle/mod.rs` now casts
each CASE mask to its column's nullable Field. Local Variant tests support the
fix. The native physical cast codec at pinned DataFusion revision `5f4b171`
serializes the Arrow type without the target Field and reconstructs a type-only
cast; its resolved field removes extension metadata. Projection decoding rebuilds
its output schema from these expressions. The existing scoped peer roundtrip
selects only the string `result_id`, so it cannot establish whole-Variant parity.
This is a compatibility evidence gap, not a finding that every key read fails or
that rows leak.

Extend the existing scoped codec roundtrip fixture/test in
`vala-bifrost-redux/src/oracle/codec.rs` to consume a masked whole Variant and a
key read. Compare local and roundtripped output values and the Variant extension
field metadata, retaining existing hidden-row/null-mask assertions. Use the
actual production codec, not a replacement serializer. If this exposes loss,
correct it at the existing codec owner under the repository's pinned dependency
and fingerprint discipline, then rerun the focused proof. Qualify a real peer
query only as needed to cover behavior the codec fixture cannot establish.
Do not invent Wyrd Variant functions, a parallel row-policy path or a custom
wrapper merely to work around missing dependency metadata.

## Observable closure and verification

| Finding / existing requirement | Required observation | Narrow proof owner |
| --- | --- | --- |
| FIND-TASK-002-1 / REQ-006, AC-001 | UI-created Alice reads actual produced payments review result and evidence; populated ledger result/evidence is excluded from values, counts/pages and point reads; existing input/member/definition checks stay green | Existing Alice production BFF/server/Postgres journey, `ui-pages.integration.test.ts`, with existing public consumers<br>**Evidence (commit `c1991bc4b`).** Selector: the Alice runner below. The journey's deployment now runs Verifiers (`reviewing` fixture); both Repositories bind the existing `ci-gate` Verifier through the new three-line policy fixture `policies/gated.toml`; CI evidence is published for each pull request through the public `verify.Evidence.publish`, and both reviews resume and pass. Admin sees 2 rows in each of `results`, `evidence`, `execution_outputs` (line 2803 ff.). Alice's TS SDK reads the payments result (`passed`), the payments evidence by a Variant key read, and its requirement outcome; counts are 1/1/1; `OFFSET 1` pages and reads by ledger's execution id and evidence id are empty (line 2832 ff.). CLI reads the evidence row, Python the result, MCP the result; member status is `completed` on CLI, MCP and the browser page; the referenced Verifier (`ci-gate`) stays refused. No rows were inserted or fabricated. **RED:** none; the stronger test exposed no defect (first full run passed after the fixture change). **GREEN:** 1 passed. `mise run ui:check` and `git diff --check` clean. **Behavior: PASS. TDD: NOT_APPLICABLE** (evidence correction, no production change). |
| FIND-TASK-002-3 / REQ-006 and Variant SQL authority | Scoped whole-Variant output retains identical values and `arrow.parquet.variant` field metadata through the actual peer codec; key reads remain correct; hidden rows stay hidden | Existing scoped peer codec test in `oracle/codec.rs`, plus production peer seam only if needed<br>**Evidence.** Selector `oracle::codec::tests::a_masked_verification_plan_executes_identically_on_a_peer` (`codec.rs:755`), extended to run each query locally and through the production peer codec: the visible row set, a key read (`detail -> 'head' ->> 'sha'`), a filter on that key, and the whole masked Variant, comparing output field and values. **RED:** key read, filter and row set matched; the whole Variant's values matched but the peer's output field had lost `ARROW:extension:name = arrow.parquet.variant`. **Cause:** the pinned DataFusion fork serialized a physical `CastExpr` as its Arrow type only, dropping the explicit target field. **Fix site:** the codec owner, the fork: `PhysicalCastNode` now carries the target field's metadata and nullability, as the logical `CastNode` already does (fork `wyrd-production` `1a0f1af0b`, with its own round-trip test; fork cast tests 29 passed, `datafusion-proto` 286 passed). Here only the pin moved: workspace `Cargo.toml`, `Cargo.lock` and `ORACLE_DATAFUSION_REVISION`, so a peer on the old build refuses the plan at preflight. No Wyrd wrapper, function or second row-policy path. **GREEN:** selector passes; `vala-bifrost-redux --lib` with the test database 959 passed; `mise run fmt`, `mise run lints`, `git diff --check` clean. Waits added: none. **Limit:** not benchmarked (excluded by the maintainer); no multi-server journey was run, the proof is the production codec in process. **Behavior: PASS. TDD: PASS.** |

Record exact selectors and expected RED before production changes for any exposed
defect, then GREEN and affected regressions. Do not reconstruct historical RED
or churn correct production code to repair the acknowledged TDD history.

The existing Alice runner is:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-pages.integration.test.ts -t 'Alice is given payments in Settings'
```

Use the original task's existing TS/testing/production UI prerequisites. Confirm
and record the exact focused codec test name from source before running:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::codec::tests::a_masked_verification_plan_executes_identically_on_a_peer)'
```

If its module/selector changes, record the actual selector; never accept zero
selected tests. Apply affected owner format/lint checks, `mise run ui:check`
when UI code changes, and `git diff --check`; codegen only when its sources
change. Run Cargo-backed work sequentially. Preserve the explicit maintainer
benchmark exclusion and inventory any added/moved hot-path external waits;
claim no measured-performance qualification.

Preserve the accepted Access suspension behavior, readable suspended accounts,
active-only issuance/authentication, tenant isolation, grants, credential and
cancellation semantics. Non-goals: reopening FIND-TASK-002-2, full integrated
suites, unrelated broad capacity assertions, benchmark qualification, shredded
pushdown redesign without a demonstrated regression, new APIs, persistence or
migrations. Return evidence to the caller; do not implement another task, launch
review, commit, merge, push or deploy.
