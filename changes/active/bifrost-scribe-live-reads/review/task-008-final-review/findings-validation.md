# Independent structured Ponytail validation — TASK-008 final

Recommendation: **FIX_REQUIRED**, solely for residual **FIND-007-4**. **TASK-008-R2 / FIND-007-7 is CLOSED.** No new runtime regression finding is retained.

## Immutable subject, inputs and limits

Candidate `9c3d7ecb982435919924dfa8e6930352b27a9b7e`; parent `23eafa368bca19208faf8311eb7b5421e3660b38`; original TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Read subject/navigation, cumulative and final change inputs, both task-implementation reports, standards, maintainer, system, tenancy, data and concurrency reports, claim comparison and verification. Read original TASK-007/008, revision 20 REQ-014/015 and AC-016/017, prior verdicts/validated ledgers and R1/R2 remediation packets. Applied AGENTS.md, agent rules, spec-driven-development, maintainer-style and the applicable Bifrost file identity, source ownership and terminal contracts. No CodeGraph index exists.

This validator is fresh and separate from discovery. The proposal union contains only `RSTD-FINAL-1`. Claim comparison correctly requires reconciliation of general prior-closure statements with exact current declarations; reviewer agreement is not proof. No additional focused discovery pass is necessary to resolve these two directly inspectable declaration sites.

Only static source/diff/import/caller and Git checks were performed. No cargo, nextest, mise, build, test, benchmark, commit or implementation edit occurred. Only this report was written. TASK-006 is considered only at TASK-007/008 seams. FIND-007-3, SQL tenant columns, the chosen `WYRD_VALA_500_QUERY_TENANT_INVARIANT` code and the disclosed lost-Scribe deadline issue remain under their explicit standing dispositions. Capacity qualification is caller-owned and no performance result is inferred.

## Proposal validation

### RSTD-FINAL-1 — CONFIRMED; preserve FIND-007-4

**Classification:** VIOLATION. This is a mandatory declaration rule, not a runtime result defect or optional stylistic preference.

**Violated obligation:** `architecture/agent-rules.md` explicitly requires “Bring types in with `use` and use bare names in signatures,” including private function parameters. TASK-008-R1 acceptance criterion 1 requires validated new/materially changed declarations to satisfy that shape. AGENTS §15 explicitly preserves mandatory repository rules despite the simplicity preference. Prior review conclusions are evidence beneath these authorities; they cannot waive an uncorrected declaration.

**Exact sites:**

- `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1801`: newly added `fn provider_error(error: iceberg::Error) -> BifrostCatalogError`.
- `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4332`: newly added `pub(super) fn is_tenant_refusal(error: &datafusion::error::DataFusionError) -> bool`.

The original-base-to-candidate diff adds both declarations. They are not untouched baseline qualified signatures. The final-delta diff does not introduce them; this is incomplete cumulative closure, not a regression caused by the metric change.

**Producer-to-consumer and sibling tracing:**

1. `BifrostCatalog::assignment_schema` constructs `IcebergStaticTableProvider` from a tenant-qualified loaded table and maps its construction error through `provider_error` at line 1445. `BifrostCatalog::provider` and `pinned_provider` map errors through the same helper at lines 1560 and 1600. Read all three complete bodies and the complete helper. The producer is the existing Iceberg provider constructor; the helper preserves its concrete error inside `DataFusionError::External` and `BifrostCatalogError::DataFusion`. Correcting the type spelling belongs in this module's import/declaration boundary, without changing provider construction, reader permits, table identity or error wrapping.
2. The shared scan's `exec::is_tenant_invariant_error` walks the typed error chain produced by footer proof. The new `is_tenant_refusal` combines that existing typed detector with the established remote message classification. Its sibling consumers are `Oracle::audit_tenant_refusal` at line 2498, `map_datafusion_error` at line 4315 and `query_stream::terminal_error_code` at line 1283. Read those full bodies and the full detector/helper. They respectively produce the single authenticated leader refusal audit, public pre-stream error and late terminal error. They are reachable production paths, and the signature correction preserves all three. `oracle/mod.rs:16` already imports the precise `DataFusionError` type; nearby `map_query_planning_error` uses it bare.

**Concrete consequence:** the dependency manifest and new declaration shape still violate the expressly required module-import contract on actual catalog and security-refusal paths. No incorrect rows, weakened tenant boundary, missing audit or changed error code is alleged. Clippy success does not establish compliance with a prose-only mandatory import rule.

**Ponytail correction ladder:** deleting either helper would alter or duplicate existing conversion/classification responsibilities merely to avoid a type spelling correction. Repository behavior already supplies the solution: use the existing Oracle `DataFusionError` import and add the existing Iceberg error type to the catalog module's top import block, with an alias if useful to distinguish its owner. Use that imported name in `provider_error`. No new helper, wrapper, type, trait, dependency, feature, checker, fixture or consumer guard is needed. Qualified value/constructor expressions remain permitted; do not sweep unchanged qualified declarations.

**Decision-complete minimum correction:** change only the import/declaration spelling at these two existing owners. Preserve exact concrete types, helper bodies, visibility, call graph, feature gates, catalog error conversion, footer-refusal classification, one-query audit, terminal selection, reader permissions and every adjacent lifecycle/durability behavior. The defect originates at the declaration sites, so downstream guards cannot close it. No new product, public API, architecture, concurrency, persistent-data or security decision is required.

**Focused closure proof:** inspect the two final signatures and top module imports against the cumulative diff, verify type identity and unchanged bodies/callers, and run committed-range whitespace checking. Formatting and applicable warnings-denied lint proof may be supplied when execution is permitted. Do not manufacture a runtime RED test or introduce an import-rule checker. Existing runtime behavior tests need no new assertion for this spelling-only correction.

## Reconciliation of prior closure claims

| Prior finding | Independently checked evidence | Disposition |
|---|---|---|
| FIND-007-1 | Current session-sized memory groups and live partition routing are documented by both task reports; source/test paths retain sparse/empty/mixed source proof. | Remains closed; no new proposal. |
| FIND-007-2 | NativeOutputTally counts delivered rows/Arrow bytes; native completion carries totals/fingerprint; decoder reconciles before closure. | Remains closed; final metric change preserves validation. |
| FIND-007-3 | Explicit standing maintainer decision. | Unchanged and excluded. |
| FIND-007-4 | Previously identified tenant/metadata/writer declaration sites are corrected. Both additional newly added helpers above remain qualified, established directly from the cumulative diff and current full source. | **Residual OPEN**; retain the prior ID rather than assign a new finding. |
| FIND-007-5 | Original TASK-008 exact-proof inventory provides package, target/features, exact selectors and environment wrappers; retained log records matching single-test outcomes and attribution. | Remains closed; runtime outcomes are furnished/retained evidence. |
| FIND-007-6 | Changed envelope test retains its assertions and substantive panic contract. New metric test likewise documents its fixture/assertion panics. | Remains closed. |
| FIND-007-7 / TASK-008-R2 | Final diff removes precisely ten progress-line trailing spaces and one extra report EOF blank line; substantive evidence remains intact. Current committed-range checks pass. | **CLOSED**. |

Behavior's general FIND-007-4 closure statement and maintainer's prior-site closure do not specifically disprove the two new declaration sites. Their behavioral/maintenance assessments remain useful, but complete import-rule closure must be revised. The earlier R1 verdict also missed these sites. This validation corrects that overbroad closure claim based on source; it does not assert a new executable defect.

## Final-delta regression assessment

Independently inspected the metric producer, native/wire completion consumers, collector and focused proof bodies in addition to discovery reports:

- `FollowerScanEvidence::finalize` consumes the follower collector after drain. Scribe's peer executor calls it when producing native completion; the gRPC adapter projects that completion through its existing wire encoder. Physical Parquet bytes remain distinct from delivered native or IPC bytes.
- `LiveFrameDecoder::accept` rejects frames after closure before any second fold. Native rows/bytes/fingerprint and remote schema/count/digest/terminal checks precede `record_footer`. The fold introduces only updates to the existing atomic scan accumulator; no IO, lease retention, retry or timeout is added.
- `LiveScribeExec` owns the shared accumulator and cloned execution leaves retain the same Arc. `OracleQueryScanStats::visit` captures it and `finalize` remains idempotent. The focused test proves remote 4096 plus native 1024 reaches query totals as 5120 bytes/two files and leaves memtable-only bytes absent. The staged pruning test separately asserts positive real Parquet bytes, retained matching rows and pruned groups.
- Harness retirement is explicitly adjacent TASK-006 work. Relevant PeerCluster/WyrdTestServer journeys retain the production peer/server paths. The documented weaker in-process connection-loss evidence is not escalated against the user's standing exclusion. No production reader, footer proof, native completion or lease mechanism is removed by deleting the process harness.
- Shared HTTP default becomes 8080 consistently in its constant, contract/golden defaults and documentation, matching the server HTTP listener. Endpoint overrides and gRPC default remain distinct existing client behavior.

The validated **new-runtime-regression ledger is empty**. No MISSING, INCORRECT, DRIFT or REGRESSION proposal survives against REQ-014/015 or AC-016/017's reviewed behavioral scope. Overall findings are not empty because the cumulative mandatory declaration violation above remains.

## Static closure and verification limits

Independently executed, all exit 0:

```sh
git diff --check a7582db58 HEAD
git diff --check HEAD~1 HEAD
git diff --check 6e7add054 HEAD
git diff --ignore-space-at-eol --ignore-blank-lines HEAD~1 HEAD -- changes/active/bifrost-scribe-live-reads/review/task-008-review/final-named.log changes/active/bifrost-scribe-live-reads/review/task-008-review/task-review-invariants.md
```

The final command produces no content difference; the ordinary artifact diff identifies the exact whitespace removals. This proves R2 independently without rerunning any workload.

Runtime results remain user-furnished exact-tree evidence: Oracle journeys 40/40, server 26/26, first-class Python/TypeScript passes, final live/follower 19/19, testing lib/bins 61/61, client 314/314, lints/fmt clean and codegen without drift. The recorded aggregate has 8/9 passing lanes with the sole failure owned by the deleted process harness. These support the inspected behavior; they are not this validator's executions or a fresh aggregate/capacity qualification. No runtime blocker is invented from the static-only restriction.

## Final deduplicated ledger

| Stable ID | Discovery source | Validation / classification | Location | Minimum correction and closure proof |
|---|---|---|---|---|
| FIND-007-4 | RSTD-FINAL-1 | CONFIRMED / VIOLATION | catalog/bifrost_catalog.rs:1801; oracle/mod.rs:4332, under redux src | Import existing Iceberg error in catalog; reuse existing Oracle DataFusionError import; use bare names in both signatures. Preserve all concrete identities/bodies/callers. Static import/signature/body diff proves correction; supplied format/lint and committed-range whitespace proof confirm integration when permitted. |

No rejected proposal is retained as optional advice and no new finding ID is assigned. **FIX_REQUIRED** is recommended solely for this bounded residual of FIND-007-4. **R2 is closed and no new runtime regression was established.**
