---
id: TASK-PACKET-R1
kind: remediation
status: ready
spec: SPEC-forge-concurrent-planning
spec_revision: 11
parent_task: packet-wide
remediates: [FIND-PACKET-1, FIND-PACKET-2, FIND-PACKET-3, FIND-PACKET-4]
---

# Close packet-wide repository standards and final verification

## Contract and candidates

- Approved spec: `changes/active/forge-concurrent-planning/spec.md`
- Task index: `changes/active/forge-concurrent-planning/tasks/README.md`
- Review verdict: `review/packet-wide-7ac45dec9/verdict.md`
- Reviewed candidate/base: `7ac45dec99535c881b7a936c66623044f15d8823` / `c1508b375`

This remediation runs after the task-specific source corrections so its audit
and final gate cover the complete corrected candidate.

## Diagnosis

The range contains new/materially changed Rust items without required rustdoc,
`# Errors`, `# Panics`, or async cancellation/partial-progress documentation.
Changed modules also contain function-local imports and fully qualified types
where module-scope imports and bare names are mandatory. Two packet Markdown
files contain extra EOF blank lines. Because the range crosses SQL, Vala,
server, contracts, three SDKs, generated artifacts, docs, test infrastructure,
and `mise.toml`, AGENTS.md requires `mise run gate`; no result is recorded.

## Intended correction outcome

Every changed Rust symbol complies with documentation and import/signature
rules, the cumulative diff is clean, and one final aggregate gate proves the
corrected broad candidate.

## Decision-complete recommendation

Audit the full changed-symbol set, document existing behavior in place, move
misattached prose to its owner, and include failure, panic, cancellation, and
partial-progress contracts where applicable. Move local imports to module or
test-module import blocks and use bare names in changed signatures, fields, and
impl heads, retaining only the explicit narrow trait-as-underscore exception.
Remove only the two extra EOF blank lines. After every task remediation lands,
run one `mise run gate`; do not repeat its component lanes as final commands,
and separately retain only specialized proof genuinely outside the aggregate.

## Preserved behavior and non-goals

- Do not change runtime behavior while repairing documentation/import shape.
- Do not add lint suppressions, wrapper types, aliases, documentation helpers,
  or weaken checks.
- Do not use this task to absorb task-specific functional findings.

## Acceptance criteria

| Finding | Acceptance criterion |
|---|---|
| `FIND-PACKET-1` | Complete changed-symbol audit finds no missing substantive rustdoc or required errors/panics/cancellation/partial-progress section. |
| `FIND-PACKET-2` | Complete changed-range scan finds no prohibited local import or qualified signature/field/impl type. |
| `FIND-PACKET-3` | Cumulative `git diff --check` exits zero. |
| `FIND-PACKET-4` | One final `mise run gate` passes on the corrected immutable candidate and its result is recorded. |

## Focused proof and broader verification

Run the repository's changed-symbol documentation and import audits (or the
existing checks that cover them), format, lints, and cumulative diff check.
Then run the single final `mise run gate`. Record any specialized lane outside
that aggregate explicitly; do not duplicate aggregate components.

## Implementation evidence

The audit covers `c1508b375..` the corrected candidate. It includes the TASK-001-R1 through TASK-006-R1 remediations and the follow-on table-authority bound (`dc7a51eb2`). The changes touch only documentation, imports, and type spelling. Runtime behavior is unchanged, and no lint suppression, wrapper, or documentation helper was added.

Changed-symbol audit method:

1. `git diff -U0 c1508b375` maps every added Rust line to its file.
2. **Documentation audit.** An item counts as changed when an added line falls anywhere in its extent: its doc block, signature, or body.
   - **Lint pass.** Workspace Clippy runs with `--all-features --all-targets`, `check-private-items = true`, and the lints `missing_docs`, `missing_docs_in_private_items`, `missing_errors_doc`, and `missing_panics_doc`. Its results are filtered to changed items.
   - **Source pass.** A second scan covers what Clippy misses, such as trait-impl methods and associated items. For each changed `fn`, `type`, `const`, and `static` it checks:
     - that rustdoc exists;
     - that `# Errors` exists when the item returns `Result`;
     - that `# Panics` exists when the item contains `unwrap`, `expect`, `assert*`, `panic!`, or `unreachable!`.
3. **Shape scan.** It flags, within changed lines:
   - function- or impl-scoped `use`, except `Trait as _`;
   - qualified paths in changed signatures, struct and enum fields, and impl heads.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-PACKET-1` | 94 Rust files. All reviewer-named sites are covered: `ensure_builtin`, `reject_reserved_field_names`, `run_snapshot_expiry_for_table_inner` (now with cancellation and partial-progress prose), `load_maintenance_protection_inner` (the misattached impl-block prose was moved onto it), `build_frames`, the three `forge_peer` service methods, `DrivenClaim::Target`, the `push_*` digest helpers, `leader::tests::key`, `DEFAULT_MAINTENANCE_INTERVAL`, `ADMITTED_AT`, `MEMBERS`, and the renamed cleanup test. The rest of the changed-symbol set gained rustdoc, `# Errors`, and `# Panics` where its body requires them. | Clippy doc-lint residue on changed items: **0**. Source-scan residue: 3 false positives. `oracle::run_sql_query` and `olap_catalog::registered_physical_layout` are documented with `# Errors`; the scanner stops at a multi-line attribute or a `// tenant-isolation` comment. `Debug::fmt` in `planner.rs` returns `fmt::Result`. | PASS |
| `FIND-PACKET-2` | Function-local imports moved to module or `mod tests` import blocks. These include `settings.rs`, `exec.rs` (`BloomProbe::for_column` and test functions), `leader.rs`, `leadership.rs` (the `Wire` alias was removed), `fingerprint.rs`, `policy.rs`, `promoted_object.rs`, `tables/mod.rs`, `persistence.rs`, `staging_runtime.rs`, `analytical_supervisor.rs`, `sdks/wyrd-sdk-rust/src/lib.rs`, and 25 imports in `wyrd-testing/tests/bifrost/oracle/distributed.rs`. Qualified signature, field, and impl types are now bare names with top-level `use` across catalog, forge, oracle, scribe, vala-sql, wyrd-spec, wyrd-tonic, wyrd-server, wyrd-testing, and the integration tests. Test-only imports carry the same `cfg` as the items that use them. | Shape-scan residue: 13 sites, all in the permitted form. Ten import the parent module because a bare name would collide: `spec::Schema` (arrow vs iceberg) ×4, `types::Type` (parquet vs iceberg) ×2, `v1::ForgeCompactionOutcome` (domain vs wire), `sync::Mutex<time::Instant>` and `time::Instant` (std vs tokio), and `watch::Sender`. The other three are `T::Err`, an associated-type projection, and `fmt::Debug`/`fmt::Formatter`/`fmt::Result`. No function-local `use` remains apart from `Trait as _`. | PASS |
| `FIND-PACKET-3` | The extra EOF blank lines were removed from TASK-002-R1, TASK-004-R1, TASK-006-R1, TASK-005-R1-implementation-reference, and TASK-003. The remediation merges had added three more beyond the two the reviewer found. | `git diff --check c1508b375` → exit 0 | PASS |
| `FIND-PACKET-4` | Final aggregate on the corrected immutable candidate | `mise run gate`, run by the lead after the benchmark | PENDING |

Commands, all exit 0:

- `mise run fmt` and `cargo fmt --all --check`
- `mise run lints`: workspace `--all-features --all-targets`, plus the release-feature `wyrd-server` binary, which caught and confirmed the `test-support`-gated imports
- `cargo clippy -p vala-bifrost-redux --lib --tests -- -D warnings`, built without `test-support`
- `mise run codegen:check`, `mise run docs:check`, `mise run check:unwrap-audit`
- `git diff --check c1508b375`
- `mise run test:bifrost:integration:redux`: 903/903 passed, showing that the import and doc moves left behavior unchanged

Non-goals held: no runtime behavior change, no `#[allow]`, no alias added to evade the rule. The only renames resolve genuine name collisions and reuse names the crate already uses for the same types: `IcebergSchema` in `execution_lanes.rs` (as in `catalog/layout.rs`) and `DataFusionResult` in the `analytical_supervisor.rs` tests (as in `analytical.rs`). Every other collision uses a parent-module import, and no task-specific functional finding absorbed.
