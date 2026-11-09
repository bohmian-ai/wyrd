# TASK-001 remediation verdict: FIX_REQUIRED

Only the remaining mandatory documentation portion of **FIND-TASK-001-3** is open. **FIND-TASK-001-1** (obsolete captures) and **FIND-TASK-001-2** (primitive/nullability proof) are closed. No new runtime defect, proof gap, specification decision or specialist gap was established.

## Immutable subject and scope

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
Prior candidate: `790da3369c1b0107765db234d72c1ae0c4680446`.
Current candidate: `c0a06eacdc70249e9c32e44b7670314cf1aefb3a`.

First remediation review: full prior-to-current diff, three follow-on commits, affected capture owners/callers/consumers, new proof and cumulative documentation closure. Original approved revision 3, TASK-001, prior verdict/ledger, all eight prior specialist reports, TASK-001-R1 and its evidence remain preserved. [subject.md](subject.md) records paths, navigation, selection, packet hashes and exact ranges. No ancestry rewrite occurred. HEAD, tracked worktree and packet hashes stayed unchanged; only new review reports/tasks were written. Packet/reports remain untracked; no commit, merge, push or implementation change occurred.

## Independent coverage and follow-up

One fresh `ponytail-rev` independently reviewed the actual fix and prior evidence under the remediation topology, and wrote [findings-validation.md](findings-validation.md). It validated selection and carry-forward for all applicable obligations. A repeat eight-reviewer team was unnecessary: production Graph/loader/registry/upload/hydration, permission/tenant/subject and planner/executor owners are unchanged, and their source-backed prior reports cover those contracts. Current local replacement capture and attachment paths, proof additions and rustdoc remain freshly inspected.

The added Vala surface is one native input test, not a production validator change. Related Bifrost/observe/trace doc-link repairs close the prescribed cargo doc gate and introduce no runtime change. No selected report is missing; no material conflict, new blocker or untraced failing path requires follow-up or separate specialist discovery. The residual issue is incomplete closure of the original mandatory finding, not a new finding validated by its discoverer.

## Closure and retained ledger

| Finding | Independently reconciled result |
|---|---|
| FIND-TASK-001-1 | CLOSED. Binding owns its current Trigger capture; AgentCall owns one current capture per workspace destination. All attachment and Action/Check conversions transfer current contributions. Regression proves replacement equivalence, obsolete descriptor removal, shared Artifact preservation and genuine active conflict refusal. Existing Graph and publication owners remain unchanged. |
| FIND-TASK-001-2 | CLOSED. Public SDK construction and canonical parity cover the missing named kinds, 56 operators, Source/reference forms, bindings and slot matrix. Native schema/null admission and existing omission proof connect authored slots to execution. Canonical public serde construction is usable; no unsupported missing-export claim or new SourceText helper requirement is retained. |
| FIND-TASK-001-3 | REVISED / VIOLATION; source IDs REPO-001 and M-002. Most documentation locations are corrected, but five SDK test wrappers still omit applicable panic contracts required by root AGENTS and the Rust guide. |

The sole retained finding is **FIND-TASK-001-3**, precisely:

- `sdks/wyrd-sdk-rust/tests/integration/authoring.rs:322` (`release_access`) and `:328` (`summarized_access`) forward versions to `access_verifier`'s expected construction, which panics on invalid version/refusal.
- `:451` (`baseline`) reaches expected fixture-code reading via `finance_release` and `fact_step`, which panics on missing/unreadable source.
- `:530` (`outcome`) forwards a requirement ID to the existing helper that panics if that ID is absent.
- `:794` (`policy_published`) constructs its CardRef before checking the server result and can panic for an invalid version.

These new wrappers have no corresponding panic section. The mandatory private/test documentation gate cannot be established by successful public cargo doc or Clippy. No runtime defect is alleged. Minimum correction is five short accurate documentation sections, using existing callee contracts; preserve all bodies, assertions and behavior. No new helper, guard, test, checker, dependency or optional style work is warranted. Full source traces, materiality and correction burden are in the independent ledger.

## Reconciled acceptance

| Obligation | Result / evidence |
|---|---|
| REQ-001 / AC-007 complete shared/public primitives | PASS: new public native-family proof plus retained canonical proof. |
| REQ-002/008 / AC-002 edits, capture, snapshots and conflicts | PASS: producer correction, all affected conversions and focused replacement regression. |
| Required versus nullable slot behavior | PASS: public authored slot matrix, native supplied-null/schema test and existing omission test. |
| REQ-004 references, ordering, concurrency and dependent outputs | PASS: unchanged runtime owner and carried prior concurrency/dependent-journey evidence; planning prose corrected. |
| REQ-005/006 / AC-003 publication, sources, reuse, retained state and exact attribution | PASS: current descriptor closure plus prior system/persistence source and real Artifact journey evidence. |
| REQ-006/008 / AC-001 receipt/state/path and file parity | PASS: local parity rerun, unchanged declaration/hydration and carried real parity/state journeys. |
| REQ-007 / AC-004 two-component case matrix | PASS: prior behavior/security and journey evidence remain valid; no consumer change reopens it. |
| INV-001–005 and non-goals | PASS: durable/transport/auth/schema/scheduler owners preserved; no new dependency/feature or foreign-runtime projection. |
| Mandatory cumulative changed-item rustdoc | FAIL: five inherited panic contracts remain FIND-TASK-001-3. |

## Verification, limits and next step

[verification.md](verification.md) records fresh passing runs of four exact local authoring tests, public SDK primitive proof and two native input tests: seven tests total. Format, scoped all-target Clippy, warning-free client/SDK cargo doc and fix-range whitespace check also pass. Cached SDK linking reports the previously observed compact-unwind-size warning; its selected test passes.

Prior real-server journeys, state regression and bounded-concurrency proof carry forward. No repeated full journeys/broad aggregates, cross-platform execution, new fault injection, live-model assessment or deliberate source mutation was performed. No new runtime risk was found in this bounded fix; only the explicit documentation gate remains.

Ready remediation: [TASK-001-R2-finish-helper-panic-contracts.md](TASK-001-R2-finish-helper-panic-contracts.md), routed directly to `$wyrd-implement`. This finishes the existing mandatory correction, without a new style audit or expanded expectations. The next review checks those five contracts and the actual new diff while preserving findings 1 and 2 as closed and carrying prior unaffected evidence forward. Runtime tests and comprehensive specialist discovery need not repeat for prose-only closure.

This is the first unsuccessful remediation review; the two-round convergence assessment threshold is not met. The remaining cause is incomplete documentation closure, not a fix-induced regression or expanded behavior requirement.

**Verdict: FIX_REQUIRED — remaining finding FIND-TASK-001-3 only.**
