# Independent structured Ponytail validation

Result: **FIX_REQUIRED**. The deduplicated ledger contains three bounded repository/evidence violations. No independently established executable-behavior, tenancy, durability, concurrency, or system-availability defect remains in the scoped candidate.

## Immutable subject and review inputs

- Candidate: `6e7add054e33701ca5ecb52a5c859948b15161a3`.
- Immediate TASK-008/R1 base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.
- Original cumulative TASK-007 base: `a7582db587c6170a290760f1741673125612b797`.
- Authority: approved spec revision 20 REQ-014/015 and AC-016/017; original TASK-007, TASK-008, TASK-007-R1 and prior verdict; AGENTS §§5/11/16, agent rules, spec-driven-development and maintainer-style references, and applicable Bifrost/analytical ownership authority.

Read both task discovery reports, standards, maintainer, system, all three domain reports, focused follow-up, claim comparison, and current verification. Validation used current source, immediate changes, cumulative live-source/native-terminal context, full bodies of the proposed correction sites, and caller searches. No CodeGraph index exists. Only this report was written; no tests, Postgres wrappers, full lanes, source edits or commits were performed. Current runtime results are attributed to `verification.md`, not to this validator.

Current user authority excludes SQL `data_tenant_id`, unrelated TASK-006 work, and re-raising FIND-007-3. The approved error is `WYRD_VALA_500_QUERY_TENANT_INVARIANT`. The review does not demand removal of the required private native completion evidence as tenant-task complexity.

## Claim dispositions

| Discovery claim | Disposition | Independent source resolution |
|---|---|---|
| INV-008-1 / RSTD-008-1 / MAINT-008-1: newly qualified tenant/reader interfaces violate required module dependency declarations | **CONFIRMED**, consolidated as FIND-007-4 | Each cited tenant field/parameter and Arrow reader return is present in current source and introduced in the immediate diff. MAINT-008-1's unique new `fixture_writer_properties` return is also qualified. Agent rules explicitly cover fields, arguments, returns and test module scope. Original corrected TASK-007 sites do not exempt new declarations. |
| BEH-008-1: named proof evidence lacks reproducible exact commands/current attribution | **REVISED**, retained as FIND-007-5 | TASK-008 has exact commands for its two refusal checks, but only module selections/command fragments for separately named writer checks and journeys. The new R1 partition/native-completion checks are absent from its task evidence; current review verification supplies genuine commands and passes for both. The defect is incomplete task evidence, not proof that any historical execution failed or never happened. |
| RSTD-008-2: materially changed schema assertion lacks panic rustdoc | **CONFIRMED**, retained as FIND-007-6 recurrence | `schema/managed_columns.rs:48–79` renames and materially rewrites the test; its single-line rustdoc has no `# Panics` despite three assertion branches. AGENTS §16 expressly includes private tests and makes missing documentation blocking. The original two cited TASK-007 panic-doc sites are corrected; this is a new instance of that same changed-test invariant. |
| Follow-up uncertainty: N partition pieces imply an impermissible N checks of a file | **REJECTED as a finding** | `HotParquetExec::partition_pieces`/`execute` starts distinct partition readers; `hot_stream` proves metadata once before each reader builder and never inside the output loop. Published loader does the same at its reader-opening boundary. REQ-015 says once when opening; it does not demand coordinated proof state across all opens. REQ-013 separately says telemetry counts a logical file once. No new synchronization/cache should be added. |
| Empty material ledgers from tenancy, data, concurrency and system reviewers | Independently corroborated within inspected boundaries | Authenticated binding reaches producer footer and cache identity; hot/staged and published readers compare before decode, published loader is mandatory and key-metadata bypass is refused. Native producer and consumer reconcile the same rows/Arrow-memory bytes/fingerprint; sparse/empty memory source construction retains N groups. No material contrary source path was established. Agreement itself was not used as proof. |

No discovery proposal is retained merely as optional advice. The three remaining findings require no new product, public API, architecture, concurrency, resource-ownership or persistent-data decision.

## Final deduplicated ledger

### FIND-007-4 — CONFIRMED / VIOLATION: required declaration shape recurs in newly added tenant proof interfaces

**Discovery sources:** INV-008-1, RSTD-008-1, MAINT-008-1. Preserve the prior ID because the cumulative obligation is the same module-import rule; the prior individual sites were improved, while the new tenant interfaces recreate it.

**Violated obligation:** architecture/agent-rules.md: “Bring types in with `use` and use bare names in signatures,” expressly including fields, arguments and returns. TASK-007-R1 criterion 4 requires owned new/materially changed declarations to follow it. This rule also independently governs TASK-008.

**Exact changed locations:**

- `crates/vala/vala-bifrost-redux/src/oracle/exec.rs:1111`: new `verify_scanned_footer_tenant` tenant argument.
- Same file `:1137–1138`: new `tenant_proven_reader_metadata` tenant argument and qualified `ArrowReaderMetadata` return.
- Same file `:1593`: added `hot_metadata_key` tenant argument.
- Same file `:3990`: new test helper `fixture_writer_properties` qualified `WriterProperties` return.
- `crates/vala/vala-bifrost-redux/src/scribe/claim_assembly.rs:87`: added `AssembleClaimRequest.tenant` field.
- `crates/vala/vala-bifrost-redux/src/forge/managed/policy.rs:260`: added `ForgeTablePolicy::to_core_config` tenant argument.
- `crates/vala/vala-bifrost-redux/src/parquet/writer_properties.rs:121`: added rewrite-properties tenant argument.

**Producer/caller/consumer proof:** The authenticated query/cut or signed assignment tenant enters `ObjectMetadataKey`; `PublishedFooterLoader::load` and `hot_stream` pass it through the new shared comparison before either published or hot/staged reader sees rows. `hot_metadata_key` is called by Oracle cut materialization with `cut.binding.tenant`. Assembly's production caller is `ScribeStagingRuntime::assemble`, which passes `context.binding.tenant` into `ClaimAssembler::encode` and the existing `ArtifactPlan`. Forge's `plan` and `rewrite_plan` both pass `self.binding.tenant` to `to_core_config`, which forwards it to the existing rewrite writer recipe. Policy/executor/assembly test callers preserve the same type. The new fixture writer helper is called by grouped, hot, resource/pruning and published fixtures in its owning test module. None of these is a dormant declaration proposed for speculative cleanup.

The defect is the spelling/dependency placement at these declarations. Footer values, production caller authority and refusal placement are otherwise correct. No tenant bypass or runtime corruption is alleged.

**Observable consequence:** The reviewed code fails an explicit repository acceptance rule and omits new interface dependencies from the prescribed owning-module inventory. The prior import correction cannot be considered cumulatively complete just because its original locations were fixed.

**Smallest correction:** Import the already-existing `DataTenantId` and `ArrowReaderMetadata` into the relevant production modules, and `WriterProperties` into the owning test-module imports; use bare names at the listed declarations. Preserve exact type identity, feature gates, binding propagation, footer proof, error chains and caller graph. Qualified constructors/value expressions remain allowed. Do not extend this into unrelated old qualified declarations or create a wrapper/helper/dependency.

**Ladder:** Deleting a tenant argument would remove required proof authority; deleting the new reader conversion would not itself satisfy the declaration rule. Existing types plus ordinary Rust `use` cover the complete correction. No abstraction or behavioral change is needed.

**Focused closure proof:** Inspect the changed declarations against module imports and the same callers. Formatting and the smallest allowed scoped compile/Clippy check establish unchanged type identity. No new runtime test or manufactured RED is warranted. Full gates remain caller-owned under the current command restrictions.

### FIND-007-5 — REVISED / VIOLATION: current named proof packet remains incomplete

**Discovery source:** BEH-008-1. Preserve the prior ID for the same evidence-owner defect. Original TASK-007 now has substantially improved historical/current/deferred recipes; that improvement is accepted rather than discarded.

**Violated obligation:** AGENTS §11 and spec-driven-development's test-command precision require every specifically named proof in a task artifact to carry its complete exact focused command. TASK-007-R1 criterion 5 requires truthful historical/new/deferred attribution, and its ordered proof requires exact selected checks/results for new scenarios.

**Exact locations:** `tasks/TASK-008-tenant-proven-per-file.md:63–72` (evidence rows and command paragraph); `review/task-007-review/TASK-007-R1-close-live-scan-gaps.md:79–101` (ordered proof); original TASK-007's “Exact recipes and attribution” evidence section.

**Independent packet/source proof:**

- TASK-008 names `scribe::parquet_writer::tests::parquet_footer_preserves_complete_claim_evidence` and `generation_encoding_preserves_sort_tenant_and_artifact_identity`. Both exist in `scribe/parquet_writer.rs` (`:1462`, `:1807`), but the packet provides only a module expression for these checks, not their exact focused recipes.
- TASK-008 names the selective distributed journey, Forge rewrite/recovery journey, Forge unchanged-promotion journey, and Scribe write/flush/read journey. Their source is respectively `wyrd-testing/tests/bifrost/oracle/distributed.rs:72`, `forge/live_rewrite.rs:1141`, `forge/scribe_promotion.rs:203`, and `scribe/write_read.rs:58`. `wyrd-testing/Cargo.toml:26–36` assigns these to three different targets (`oracle`, `forge`, `scribe`). The evidence's generic Postgres/package/profile fragments and statement that exact expressions were used do not identify those complete target/selector recipes. This does not challenge the recorded six journey executions merely because four distinct named journey identities appear in those rows.
- The two new R1 checks are real source tests: `oracle::follower::tests::scribe_live_sources_keep_the_session_partition_count` and `oracle::live::tests::native_completion_reconciles_the_delivered_output`. Existing original-task recipes list the older configuration/frame tests instead. Current `task-008-review/verification.md` now supplies exact selected commands and genuine passes for the new checks; that evidence can be linked into the packet without rerunning them.
- Original TASK-007's server result additionally names three individual peer-service tests but supplies a module selection. The same exact-named-test rule applies if those names remain acceptance evidence. Completing the named-proof inventory in the existing evidence sections covers this without a new mechanism.

**Producer/consumer and reachability:** The existing evidence sections produce the proof claims used by fresh implementation/review agents. Tests, package/target declarations and environment wrappers are already present, so omitted selectors/setup can be recorded at that evidence owner. The failure is reached by trying to reproduce the named accepted check solely from its packet; choosing between three targets or constructing the selector requires outside rediscovery. Source correctness or an aggregate green does not cure the missing exact recipe.

**Observable consequence:** The packet cannot reproduce all of its specifically named proof or clearly distinguish new R1 acceptance runs from older module/historical results. This is not an assertion that historical execution failed, was fabricated, or did not occur. Current five focused passes do not retrospectively establish unavailable historical RED results or execute Postgres journeys.

**Smallest correction:** Complete the existing evidence sections with source-confirmed package/target/features/setup and exact selectors for every named check. Cross-reference the genuine current review commands/outcomes for the two new R1 checks. Recover authentic historical scenario evidence when available; otherwise explicitly state unavailable historical RED/execution and distinguish reconstructed current recipe from claimed historical result. Keep currently prohibited environment-dependent executions explicitly deferred. Do not invent RED/GREEN evidence, add tests/harnesses, rerun old commits, or run Postgres/full lanes to fix an evidence omission under this review's restrictions.

**Ladder:** Removing a redundant named claim is acceptable only if the remaining evidence still proves the full task; deleting required proof is not. Existing evidence/test ownership already solves reproduction; exact text and truthful attribution suffice. No production correction is needed.

**Focused closure proof:** Match every retained named-test claim to its real source, target and exact selector, and to authentic executed/historical/deferred attribution. The current two R1 focused passes may be reused. Future permitted execution uses mise with the mandated worktree-local target/state directories; Postgres-dependent proof is recorded as deferred until authorized. Do not require new tests for evidence prose.

### FIND-007-6 — CONFIRMED / VIOLATION: changed-test panic documentation recurs in the new envelope proof

**Discovery source:** RSTD-008-2. Preserve the prior ID for the same mandatory changed-test panic invariant. Original `live_frames_release_batches_incrementally_and_validate_the_footer` and `scribe_follower_execution_shape_contract` now have substantive `# Panics` sections and are closed at their old locations. The retained occurrence is the separately changed TASK-008 schema proof, not an allegation that those R1 edits were omitted.

**Violated obligation:** AGENTS §16 requires substantive rustdoc for every materially changed Rust test and `# Panics` whenever panic remains possible. Agent rules explicitly include private tests and identify incomplete documentation as blocking.

**Exact location:** `crates/vala/vala-bifrost-redux/src/schema/managed_columns.rs:48–79`, `with_managed_columns_appends_the_envelope_without_a_tenant_column`.

**Independent producer/consumer proof:** The immediate diff renames the old tenant-envelope test, removes the old tenant assertion blocks and replaces the documented outcome with the tenant-free envelope. The current body calls production `with_managed_columns`, then asserts exact field order and non-null principal/request identities. Its one-line rustdoc states an outcome but has no panic section. This actual fast-lane test is the direct consumer of the managed-envelope producer; the failure can occur when any asserted schema invariant changes. The full producer and test bodies were inspected. Ordinary Rust test discovery invokes this zero-argument test, so no bespoke call path or new harness is needed.

**Observable consequence:** The materially changed proof item lacks an explicitly required panic contract, despite asserting precisely the envelope order/nullability boundary that a maintainer changes here. No failed runtime result or missing tenant deletion is alleged.

**Smallest correction:** Add substantive documentation to this existing test identifying the envelope order/absence and principal/request non-null invariants, with a `# Panics` section for their assertion failures. Retain the body and assertions. Do not replace assertions with fallible success, suppress documentation rules, or clean unrelated old tests.

**Ladder and focused proof:** Existing rustdoc is the correction boundary; neither a helper nor another test is needed. Inspect docs against all three assertions, and run formatting/whitespace. If an existing runtime proof is selected, its exact identity is `schema::managed_columns::tests::with_managed_columns_appends_the_envelope_without_a_tenant_column` in the redux lib; the documentation-only correction itself does not require a manufactured behavioral RED or new suite.

## Prior closure, complexity and verification reconciliation

FIND-007-1 is closed by exactly N memory-source groups including empty/sparse cuts; staged sources use the same session target. FIND-007-2 is closed by production `NativeOutputTally`, completion totals/fingerprint, and independent decoder reconciliation. Current focused passes substantiate both. FIND-007-3 is accepted unchanged under the maintainer's explicit disposition. Original specific FIND-007-4 and FIND-007-6 sites were corrected, but the newly changed interfaces/schema proof repeat those same rules as detailed above. FIND-007-5's old task recipes improved; its current named-proof packet gap remains.

The retained consumer guard belongs at file opening because the scan owns the external file trust boundary. Producers stamp from authenticated bindings; the reader must still refuse missing/foreign metadata. Removing that guard because the writer stamps correctly would weaken REQ-015. Existing metadata/cache/reader owners cover it with one footer field and shared comparison; narrow published/hot adapters and required terminal error propagation do not justify a new tenant engine. Native tally is explicitly required R1 work. No speculative complexity finding is retained.

`verification.md` records five selected focused passes and whitespace/immutable-HEAD checks on this candidate. These cover footer refusal, hot refusal, sparse/empty/mixed partitions, incremental frames and native terminal reconciliation. Historical module/journey/lint/codegen/docs claims remain historical and were not rerun by this validator. Postgres/full lanes and benchmark remain outside this review's execution scope. The ledger calls for bounded declaration, rustdoc and evidence corrections only; it neither implements them nor changes approved behavior.
