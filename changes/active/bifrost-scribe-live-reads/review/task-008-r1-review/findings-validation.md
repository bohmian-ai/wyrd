# Independent structured Ponytail validation — TASK-008-R1

Result: **one CONFIRMED finding, FIND-007-7**. FIND-007-4/5/6 are closed. No runtime regression proposal survives or is introduced by this validation.

## Immutable subject, inputs and limits

Candidate `23eafa368bca19208faf8311eb7b5421e3660b38`; correction parent `6e7add054e33701ca5ecb52a5c859948b15161a3`; original TASK-007 base `a7582db587c6170a290760f1741673125612b797`; TASK-008 immediate base `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`.

Read subject/navigation, supplied cumulative and correction diffs, both implementation reports, standards, maintainer, system and all three domain reports (tenancy, data, concurrency), claim comparison and independent verification. The discovery union contains exactly RSTD-R1-1. There is no conflicting runtime claim or unresolved discovery path requiring a focused follow-up.

Applied approved spec revision 20 REQ-014/015 and AC-016/017, original TASK-007/008, TASK-008-R1 and prior validated findings, AGENTS completion standards, agent rules, spec-driven-development, maintainer-style and implementation-execution verification authority. Current maintainer decisions stand: FIND-007-3 unchanged, Postgres tenant columns excluded, `WYRD_VALA_500_QUERY_TENANT_INVARIANT` retained. Unrelated TASK-006 work remains excluded. No CodeGraph index exists.

Independent validation used source/import/body/caller inspection and static Git checks. No source edit, commit, Postgres wrapper, full mise lane or runtime proof rerun occurred. Only this report was written. Candidate HEAD remained unchanged; tracked working-tree diff is empty. Root's current 13/13 exact redux tests, scoped format and warnings-denied Clippy are executed evidence distinct from the supplied prior server/journey results. No performance or full-lane conclusion is inferred.

## Prior-finding closure independently checked

| Prior ID | Current source and evidence | Disposition |
|---|---|---|
| FIND-007-4 | R1 introduces module imports of `DataTenantId` in `forge/managed/policy.rs`, `parquet/writer_properties.rs`, `scribe/claim_assembly.rs` and `oracle/exec.rs`; reader metadata and test writer properties use their existing imported types. Corrected tenant field/parameters, reader return and fixture return use bare names. `wyrd-spec/src/lib.rs:55` re-exports exactly `ids::DataTenantId`, so the removed claim-test direct import does not change identity. | **CLOSED**; agent-rules declaration/import obligation now passes without new types or wrappers. |
| FIND-007-5 | TASK-008 exact-proof section at lines 76–124 supplies concrete names, package, lib/target, feature/profile, exact expression and environment recipe for 13 redux units, 3 server units and 9 journeys. Retained `task-008-review/final-named.log:31–55` has matching nonzero single-test PASS summaries. Migration/setup is recorded at lines 23–30. Current lead execution is distinguished from earlier evidence and deferred full lanes/capacity. | **CLOSED** for exact recipes and truthful attribution. Supplied environment-backed execution is not claimed as this validator's execution. The user-provided retained log resolves the original scratchpad execution location. |
| FIND-007-6 | `schema/managed_columns.rs:48–85` adds a substantive `# Panics` covering canonical field order, tenant absence and principal/request nullability. Full test body still performs the exact names assertion and both non-null assertions; R1 changes no assertion. | **CLOSED**; no invented runtime proof or extra test is needed. |

The source changes in R1 are equivalent import/declaration spellings and panic rustdoc only. Source inspection also traces the corrected reader helpers from `PublishedFooterLoader::load` (`oracle/exec.rs:1035`) and hot/staged metadata conversion (`:2956`), where proof still precedes reader construction, pruning and decoding. Hot keys take the authenticated binding from `oracle/mod.rs:3043`. Claim assembly still receives `context.binding.tenant` from `scribe/staging_runtime.rs:714–722` and forwards it into `ArtifactPlan` (`claim_assembly.rs:252–266`). Both Forge callers still supply `self.binding.tenant` (`forge/managed/executor.rs:198–203,285–290`) to the existing writer properties. R1 changes none of those producer, consumer or sibling paths. No changed executable expression, feature, dependency, ownership boundary or protocol is introduced by the correction.

## Proposal validation

### RSTD-R1-1 — CONFIRMED, retained as FIND-007-7

**Classification:** VIOLATION. This is a required proof/gate failure, not a row-result defect or discretionary formatting preference.

**Obligation:** TASK-007 verification and TASK-008 verification expressly require `git diff --check`; TASK-008-R1:68 retains that proof. `architecture/references/languages/implementation-execution.md:244–253` requires whitespace checking and final diff inspection. AGENTS §12 (`AGENTS.md:546–562`) requires touched-surface checks to pass and states that a red gate blocks completion. The review skill requires assessing the immutable cumulative candidate, so an empty post-commit working-tree check cannot stand in for checking that change.

**Locations:** `changes/active/bifrost-scribe-live-reads/review/task-008-review/final-named.log:1–10` has ten trailing spaces; `changes/active/bifrost-scribe-live-reads/review/task-008-review/task-review-invariants.md:55` adds a blank line at EOF.

**Producer-to-consumer evidence:** captured Docker progress was stored verbatim with trailing spaces in the newly committed log; the newly committed prior report includes an extra final empty line. These bytes enter the immutable R1 addition and are consumed by Git's existing whitespace check. Direct file inspection and `git diff --numstat HEAD~1 HEAD` establish both artifacts are additions in this correction. They are not excluded TASK-006 changes or a dormant runtime path. The task explicitly requires these retained evidence/report artifacts to support this review.

Independently reproduced commands:

```sh
git diff --check HEAD~1 HEAD
git diff --check a7582db587c6170a290760f1741673125612b797 HEAD
git diff --check
git diff --exit-code
git rev-parse HEAD
```

The first two checks produce exactly the ten trailing-space diagnostics and one EOF diagnostic. Root's separately captured `diff-check.log` records exit 2 for the committed range. The working-tree checks pass and HEAD is the declared candidate. Their different outcomes are expected: the latter checks see no tracked uncommitted change. Root's scoped source/docs/task check also passes; it isolates the failure to the committed review artifacts and does not waive their required whitespace obligation.

**Observable consequence:** the immutable correction and cumulative reviewed change fail the required whitespace proof. No production regression, changed judgment, corrupt result or false historical test result is alleged. The artifact-only failure is bounded but still blocks acceptance under the explicit completion rule.

**Ponytail ladder and smallest correction:** retain the durable proof/report content. Deleting whole artifacts would discard requested evidence; deleting only the ten trailing spaces and the extra EOF blank line preserves it. The existing Git check already detects the issue. No dependency, new checker, configuration exception, wrapper, runtime guard or test harness is needed. Correct the artifact bytes at their existing evidence owners; do not broaden cleanup or alter test names, commands, summaries, results or prior conclusions. Do not modify source, weaken the check, or restrict its paths to conceal the failure.

This is distinct from prior FIND-007-5: the exact test recipes/results are now complete; the new defect is whitespace in artifacts introduced into the immutable correction. FIND-007-4/5/6 remain closed. One correction boundary covers both artifact instances; separate findings or repeated downstream checks would add no value.

## Final deduplicated ledger

| Stable ID | Discovery source | Status / class | Required outcome | Focused closure proof |
|---|---|---|---|---|
| FIND-007-7 | RSTD-R1-1 | CONFIRMED / VIOLATION | Normalize only the ten log line endings and prior report EOF, preserving all evidence and runtime behavior. | On the next immutable candidate, run `git diff --check a7582db587c6170a290760f1741673125612b797 <candidate>` and `git diff --check 23eafa368bca19208faf8311eb7b5421e3660b38 <candidate>`; both exit 0. Inspect the actual correction diff to prove only the identified whitespace changed. Working-tree checks alone do not close the committed-range failure. |

No other finding is retained. The correction requires no product, public API, architecture, security, concurrency or persistent-data decision. No Cargo test, Postgres setup, full lane, new formatter or new test is necessary for this artifact-only remediation; the existing static Git proof directly exercises the gap. The independently validated ledger therefore supports **FIX_REQUIRED**, solely for FIND-007-7.
