# TASK-004 verdict — FIX_REQUIRED

Nine independently validated, bounded implementation/authorship findings remain. No specification revision is required. Route the accompanying [TASK-004-R1 remediation](TASK-004-R1-sdk-contract-and-journey-closure.md) directly to `$wyrd-implement`. This review does not implement, commit, merge, push or deploy.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004`.
- Branch: `change/verifier-building-blocks-task-004`.
- Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0` — parent of first named implementation commit `52fc831f8`, so that initial implementation is included.
- Candidate: `07adb7907951f343544e550f6cabc5b7aa3aadea`.
- Packet/evidence input and observed HEAD: `aa32c821ef74886526b7367bfd77d51c87e0d521`; its changes after candidate only add packet/evidence files. Tracked implementation source remained unchanged throughout review.
- Approved authority: SPEC-verifier-building-blocks r7 at `changes/active/verifier-building-blocks/spec.md`; original task at `changes/active/verifier-building-blocks/tasks/TASK-004-sdks-examples-and-docs.md`; coordination README and approved TASK-005 handoff. Original task's r6 frontmatter is preserved history, with the explicit r7 handoff applied.
- External demo: base `000ed1f2f587a6629a471c51aadff5e05f0c820b` plus frozen uncommitted files, preserved in `external-source.tar.gz`, `external.diff`, and `external-snapshot.sha256`. No external implementation commit is invented. Its precise source is reviewable; its uncommitted provenance and unexecuted installed-SDK/server workflow remain limits.
- Full cumulative diff: [cumulative.diff](cumulative.diff); exact inputs/map: [subject.md](subject.md); original spec/task/coordination snapshots are retained alongside this verdict.

## Independent review results and follow-up

| Independent role | Report | Discovery/validation result |
| --- | --- | --- |
| Caller behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL, BEH-001–004 |
| Values/invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL, I-001–005 |
| Repository standards | [standards-review.md](standards-review.md) | FAIL, STD-001–003 |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL, MNT-001–002 |
| System resilience | [system-review.md](system-review.md) | FAIL, SYS-001 |
| Security/tenancy | [domain-review-security.md](domain-review-security.md) | FAIL, SEC-001 |
| Client concurrency/recovery | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL, CONC-01 |
| Fresh focused follow-up | [followup-review.md](followup-review.md) | RESOLVED, FUP-001–002 |
| Fresh independent Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED, nine final findings |

All required reports are present and cover their assigned surfaces. Discovery received the same navigation map and immutable inputs without other roles' conclusions. The orchestrator grouped claims in [claims-comparison.md](claims-comparison.md), then requested follow-up because reports disagreed about scheduled Custom authorship and matched external policy semantics. Follow-up located Custom in all languages and distinguished immediate guard from background result. The fresh validator independently traced every proposal, including unique claims, rejected the false Custom absence, consolidated duplicate causes, and included the TypeScript attribution-reader sibling of the SQL identity finding. No further unresolved conflict requires another follow-up. The orchestrator adopted only its CONFIRMED/REVISED ledger; no human-directed addition is assigned a finding ID.

## Reconciled acceptance matrix

PASS here means the assigned Task004 implementation or authorship obligation is supported by source and available focused proof. It does not mean deferred server journeys passed. FAIL rows name the independently validated gap; unsupported discovery allegations do not override validation.

| Requirement / criterion / constraint | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001/002: generic hydrated named slots, missing evidence preserved | Shared `verify::Target::project`, `state::bound_verifier`; wrapper conversion | Client projection tests; authored named-input journeys | PASS |
| REQ-002 / INV-002: single-slot positional support in all SDKs | Rust/Python preserve One; TS native converter always produces Named | TS tests cover named maps only | FAIL, FIND-004-3 |
| REQ-010: local lazy File, stream hash, missing-only upload and exact Artifact declarations | File/RunEvidence, shared `sha256_file`, storage upload dispatch | Focused file/projection tests and existing transfer owner source | PASS |
| REQ-011: Run verify, 422 Window refusal, queued Run submit, Recorded Window submit | Observe verify/submit and existing drift producer | Shared owner tests and authored real-time/scheduled journeys | PASS |
| REQ-012: retained authenticated client, explicit/ambient reconnect, ID/status/wait projections | Shared Execution, native wrappers, Rust exports | Isolated reconnect/refusal tests; three-language reconnect calls status and wait | PASS except lazy/deadline rows |
| REQ-012: Python lazy construction | New PyExecution only holds a connected handle; existing LazyConnection alternative exists | No lazy constructor/proof | FAIL, FIND-004-2 |
| REQ-012/013: bounded caller wait, honest visible result/terminal error | Wait checks deadline after status IO and result return | Existing 28-test run cannot expose slow in-flight status | FAIL, FIND-004-1; real-result/no-fabrication behavior otherwise retained |
| REQ-013: one replay key/body across transport retry, no SDK durable queue or ambiguous direct replay | Target enqueue/execute and existing shared transport | Retry/visibility and direct-timeout owner tests | PASS |
| REQ-014 / Scenario5: genuine Service PSI registration/fitting, submit/drain/schedule, failed-only delivery | Existing three-language scheduled Operator journeys extended | Authored actual fitter/result/receiver/pass/empty checks, execution deferred | PASS for assigned authorship |
| Required drift methods | PSI lifecycle and Custom observe-a-run schedules exist; SPC fixture deliberately unready | Ready/fitted numeric SPC journey absent | FAIL, FIND-004-5; Custom absence claim rejected |
| Required extract row in all languages | Format fixture and expected outputs exist, but sample bytes only ignored locally | Candidate tree lacks all nine files; setup does not generate them | FAIL, FIND-004-4 |
| Required action/check positive and negative rows | Existing graph covers many positive/refusal cases | Agent/code failures, trace ordering/attributes and invalid Verdict/citation remainder listed exactly in validated ledger | FAIL, FIND-004-5 |
| REQ-008 / Scenario3 hosted Card/Bifrost/code tool permission/delegation proof | Hosted story only invokes fs.read; direct Bifrost delegation is a different path | Required downstream protected-tool assertions absent | FAIL, FIND-004-5; no SDK bypass asserted |
| REQ-015: typed identity/breakdown/provenance parity | Rust/TS closed records and native Python result properties | Python fixed dictionary records erased to Any | FAIL, FIND-004-8 |
| REQ-012/015: analytical execution correlation in authored consumers | Three scheduled helpers and three attribution readers use result_id for execution | Separately minted scheduled result identity; source trace across all languages | FAIL, FIND-004-6 |
| REQ-016: canonical matched Rust/Python/TS support-policy story | One shared Card/PDF, immediate/background/reconnect entrypoints and failing answers | Authored support-desk assertions; example checks reported | PASS for assigned authorship |
| REQ-016: external same-policy result meaning | Separate immediate guard has assertion; background policy-check omits it | Frozen external test expects only follows-policy | FAIL, FIND-004-7 |
| REQ-017: extraction/runtime/tool guides and existing guide replacement | New references and updated guides/design/doctrine/Bifrost sources | docs:check reported; startup parity explicitly pending server integration | PASS task-local |
| r7 live MCP/dispatch/binding/template/SQL producer/lineage cutover | Explicit TASK-005 ownership/handoff | No live runtime completion claimed from doc catalog or codegen | PASS for Task004 handoff; final obligation remains |
| INV-001/002: one shared client and complete existing transport/storage reuse | Observe/Target/Execution, storage executor and foreign-runtime projections | Independent owner comparisons, reported dependency/Clippy checks | PASS |
| INV-003/004: no SDK authorization or durable queue/result/audit owner; authority retained | Authenticated transport, declarative request identities, no trusted tenant fields | Authored tenant/verifier denials; server enforcement remains assigned | PASS; hosted proof gap above remains |
| INV-006: failed verdict as data and no invented result from completion | Typed result projection, missing/no-result/error branches | Supporting owner tests and authored failure cases | PASS except identified proof/deadline gaps |
| INV-008: assigned SDK/docs/example replacement without aliases | Removed old emit/judge surfaces and active old guides/fixtures | Reported exports/codegen/dependency/docs checks | PASS within staged Task004 scope |
| Mandatory changed Rust field documentation | Two new PyO3 ownership fields lack field rustdoc | Explicit AGENTS §16 gate, compiler/lints alone insufficient | FAIL, FIND-004-9 |
| AC-005–010: isolated proof now, authored journeys now, integrated execution later | Existing lanes include new journey files; focused checks reported | Review-run client tests pass; missing authorship/assets remain above | FAIL for incomplete authorship; deferral itself valid |
| Non-goals/regression constraints | No second transport/store, SDK provider engine, app framework, new Card kind or UI redesign | Full cumulative source/diff and independent reuse audits | PASS; unrelated scenario evaluation not charged as removed building-block functionality |

## Validated finding ledger

The authoritative complete ledger, including every discovery-ID disposition, exact source locations, producer-to-consumer/caller traces, reachable consequences, selected correction and focused closure proof, is [findings-validation.md](findings-validation.md). Its nine entries are incorporated here and copied with their diagnosis/recommendation into the self-contained remediation task.

| Stable ID | Status / classification | Source IDs | Correction boundary and consequence |
| --- | --- | --- | --- |
| FIND-004-1 | CONFIRMED / INCORRECT | BEH-002, I-001, SYS-001, CONC-01 | Shared Execution::wait must enforce its deadline around status/auth/retry/body IO; late successful results currently bypass caller timeout. |
| FIND-004-2 | CONFIRMED / MISSING | BEH-004, I-003 | Python Execution must reuse LazyConnection for lazy ID/client construction while preserving eager/returned connected handles. |
| FIND-004-3 | REVISED / MISSING | I-002 | TS verify/submit must preserve positional versus named inputs through shared Inputs; preserve maps and disambiguate object JSON explicitly. |
| FIND-004-4 | CONFIRMED / MISSING | BEH-001, STD-003 | Normally track all nine deterministic sample assets and update shared consumers; a clean candidate cannot run extraction journeys. |
| FIND-004-5 | REVISED / MISSING | BEH-003, I-004, SEC-001, FUP-001 | Complete retained authored failure/tool/trace/ready-SPC rows in existing three-language journey families; preserve existing Custom/PSI coverage. |
| FIND-004-6 | REVISED / INCORRECT | STD-001 | Use published execution_id in all shared scheduled and attribution readers; result_id currently selects the wrong identity. Preserve legitimate result joins. |
| FIND-004-7 | CONFIRMED / INCORRECT | I-005, FUP-002 | External background policy-check must include canonical deterministic refund assertion; immediate guard cannot supply its resumed result semantics. |
| FIND-004-8 | CONFIRMED / MISSING | MNT-001 | Precisely type Python fixed dictionary wire records using existing TypedDict/JSON practice; retain runtime representation and open JSON leaves. |
| FIND-004-9 | CONFIRMED / VIOLATION | STD-002, MNT-002 | Document the two new native wrapper ownership fields under the explicit rustdoc gate, batched with implementation corrections. |

No optional preference, new security policy, unapproved drift aggregation, duplicated runtime mechanism or speculative hardening entered the remediation. Rejected/nonblocking discovery notes are not requirements. There is no earlier TASK-004 verdict or stable finding to close; these IDs are the first review ledger. Historical TASK-001 findings are upstream context, not relabelled current findings.

## Verification and limits

Independently executed during this review:

```bash
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(/^observe::|^verify::/)'
```

Result: **28 passed, 233 skipped**, recorded in [client-check.log](client-check.log). `git diff --check` passed; tracked source diff remained empty and HEAD unchanged at final artifact assembly. These checks do not prove the new delayed-status deadline case, ignored sample delivery, precise Python typing or missing journey assertions.

The implementation's other Clippy, Python 628/TS 70 unit, format/lint/type, codegen/N-API/docs/examples/dependency results are reported evidence in the original task snapshot, not independently reproduced logs. Full SDK/server journeys, served contracts, real worker/tool/ready-SPC integration, startup configuration parity and installed-SDK external demo execution were deliberately not run under the approved task scheduling. Final all-task proof still runs after TASK-005. The frozen external changes remain uncommitted in their own repository; closure must identify the corrected external source independently.

The implementation review subject is immutable. Only review reports, snapshots and the remediation packet were written. The next review must reassess the original base through the entire fixed candidate, this verdict and all nine finding IDs, rather than reviewing only the fix diff. **FIX_REQUIRED** remains the completion result until that independent review passes.
