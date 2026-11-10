---
id: TASK-002-R2
kind: remediation
status: ready
spec: SPEC-verifier-review-pipelines
spec_revision: 18
parent_task: TASK-002
remediates: [FIND-TASK-002-5, FIND-TASK-002-7]
---

# Prove replay after an unobserved reply and finish changed-helper imports

Route directly to `$wyrd-implement`. This is a bounded proof/static-rule correction, not a production redesign or another plan.

## Acceptance context and outcome

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Approved authority: `changes/active/verifier-review-pipelines/spec.md`, baseline revision 18 with explicitly approved publication/durability corrections; `output-publication.md` and `execution-state-and-durability.md` in that packet. Draft wait/approval additions are excluded.
Original task: `changes/active/verifier-review-pipelines/tasks/TASK-002-agent-tooling.md` (including completed planning correction).
Original cumulative base: `990a85dc7155ff0d4de1acc17e4aeaa7a42d582f`.
Prior original candidate: `75c9bfa9e07163099eac40d5a556c4447c6a519d`.
R1/new reviewed candidate and R2 baseline: `28f07d99cee44669cd4a8209d1f096ee19c5dfd4`.
R1 task: `changes/active/verifier-review-pipelines/review/task-002-75c9bfa9e/TASK-002-R1-publication-integrity-and-proof.md`.
Prior acceptance: that directory's verdict/validated ledger plus this directory's `verdict.md`, `findings-validation.md`, selected reports and `followup-review.md`. Preserve their findings and identities for the next review.

An accepted publication must return its original receipt even when its caller did not receive the first successful reply. The current replay implementation has credible source/positive proof; the specific lost-response observation remains missing. This task supplies that small server-seam observation and required imports in the same edited proof area. The three production fixes and other five original findings have closed; do not rebuild them.

## Diagnosis and selected minimal corrections


### FIND-TASK-002-5 — Required uncertain-response proof remains absent

**CONFIRMED; MISSING; sources DATA-R1-001 and FOLLOWUP-O5.2; O5.2; original B-002/SSR-002 boundary.** Location: `crates/wyrd/wyrd-server/tests/integration/pg_durable_verifier_work.rs:1725–1750,1797–1817` and delivery consumer `crates/skald/skald-agent/src/loop_runtime.rs:632–695`.

The producer accepts a File and the normal Invocation returns `{name,kind}` through RecordedTool into the Agent tool-result turn. The model's next final drafter reply is consumed and settlement retained before consumer Busy triggers recovery. Then a direct Publications::path replay obtains the same receipt. This proves replay independent of missing scratch/storage work; it does not create the required uncertain first response. The explicit lost-reply observation remains unproved despite a passing test and accurate production ordering. Materiality is the named acceptance deliverable, not conjectured data corruption or a network framework requirement.

**Decision-complete correction:** retain the real Agent/recovered-consumer case. Add or extend a bounded assertion at the existing server invocation/publication seam so the first publication response is actually withheld/dropped after accepted state is observable, before the publication caller receives it. Retry the same original call/producer/name/arguments against retained/restored state; assert original receipt, unchanged accepted revision/entry and replay with scratch unavailable. Reuse RunState/Publication/Invocation and existing deterministic server test controls. A seam-local response-loss proof is sufficient: no public resume API, real network fault harness, Agent restart framework, extra persisted identity/cache/store, new scheduler or broader matrix. Do not move adjacent File lifecycle or authorization responsibility. Keep the current exact-object recovery and no-settled-producer-rerun assertions.

Correction burden is one bounded owner-tier failure observation plus focused test; successful receipt deletion from a local variable is smaller but cannot exercise loss before caller delivery. Run the changed exact server selector using the existing managed Postgres wrapper; do not rerun full SDK suites merely for this missing seam.

The focused follow-up is **RESOLVED** and independently source-validated: normal result collection continues at loop_runtime `:704–718`, and the settled draft demonstrates delivery before recovery. Its minimal lower-seam proposal is accepted: a test-only adapter may intentionally discard the first Publications result before the modeled recipient receives success, then independently read accepted state and retry. It need not cancel the full Agent loop or prove automatic Agent reissuance. This is different from discarding the already-completed engine result. Correct the existing test rustdoc's unsupported lost-reply claim while preserving that test's useful positive replay/recovery assertions.

### FIND-TASK-002-7 — New changed signatures violate the required imports rule

**CONFIRMED; VIOLATION; source SSR-R1-001; changed-item mandatory gate associated with O7.1.** Location: `pg_durable_verifier_work.rs:154` publisher return, `:1438,1441` agent_engine caller/recorded parameters, and `verification/state.rs:1370` tests::publish value parameter.

Read the whole added helpers and their calling tests: each qualified type belongs to a called R1 helper, not dormant old debt. The rule explicitly says bring types into scope with use and write bare names in signatures/fields/bounds. Existing surrounding module imports establish the same local pattern. Runtime behavior is unaffected, but an explicit mandatory repository rule is not waived by low probability or green clippy. The original panic/local-import sites are fixed; this is the same Imports/signatures cause entering new proof helpers, so keep FIND-7 rather than assigning FIND-8.

**Decision-complete correction:** import Caller and InvocationRecord at the existing server test module's imports, Value in the state tests module, and use bare names in these changed signatures. Keep imports through the same owning re-exports and preserve helpers/callers/assertions. No abstraction, fixture reorganization or unrelated qualified-path sweep. Burden is a few import/signature edits; deletion of the tested helpers would remove required proof and is not smaller complete closure. Batch with FIND-5. Static review of these changed signatures plus affected fmt/lints is sufficient; no new behavioral test is warranted.


## Constraints and preserved behavior

Keep the existing real-Agent recovered-consumer case, unavailable-inventory/exact-object proof, no settled-drafter rerun and deterministic dropped-run proof. A new seam proof may complement them; it must not replace them with a weaker mock. Make the loss recipient and loss point explicit, and establish acceptance separately from delivery. A missing scratch tree plus unchanged accepted revision/entry establishes replay without new upload/acceptance. Do not claim a discarded engine outcome retroactively lost a receipt already delivered to its Agent.

No production cache, transport, replay index, public resume API, scheduler, general fixture framework, state format or upload owner change. Retain accepted authority, exact File decision/audit, tenant isolation, snapshots, durable-before-receipt, cancellation, retry accounting and budgets. No new imports of implementation owners through the wrong tier; reuse existing owner exports. No unrelated style sweep, snapshot-memory redesign, scan optimization, live grant refresh or new capture policy.

Historical production-before-RED remains recorded. Do not fabricate missing execution history or rewrite earlier reports. Preserve earlier evidence and append accurate closure for this task. The R1 ledger has 28 distinct O IDs (the earlier prose claimed 30); preserve IDs and assertions rather than inventing rows to match the count. Only O5.2 and the remaining changed-item part of O7.1 reopen here.

## Proof strategy and acceptance

This task changes failure-path proof and static imports, not known defective production behavior. Verification-only proof may pass on its first run; do not manufacture RED or production churn. If a genuine production defect is exposed, record it and route any material decision instead of silently expanding this task.

| Retained finding | Required observation |
|---|---|
| FIND-TASK-002-5 / O5.2 | A fresh accepted File publication's first response does not reach its modeled caller; independently observed accepted state retains it. Original producer/call/name/arguments replay returns the original receipt with unavailable scratch, unchanged accepted entry/revision, and no new upload. Existing exact recovered-consumer assertions still pass. Unsupported lost-reply rustdoc is corrected. |
| FIND-TASK-002-7 / changed-item O7.1 | Caller, InvocationRecord and Value are imported at their existing module scopes and those added signatures use bare names; same helpers, calls and assertions remain. Static changed-item inspection closes this gate without an unrelated signature sweep. |

Use the smallest owner-tier observation and existing state/publication/fault controls. Private helper/fixture organization remains implementation-owned. One named new server case below is a planned selector, not a required private code shape; confirm its actual registration after adding it, before executing, and update the exact command if its name differs.

## Focused verification

Run only the modified/new exact server selectors. The first is planned for the missing seam; the second preserves the adjacent existing recovered-consumer observations after its comment/proof edits. The package, integration target, test-support feature and repository-managed Postgres/migration setup are the confirmed R1 owner setup.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all --test-threads=1 -E "test(=pg_durable_verifier_work::publication_receipt_survives_a_lost_reply)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all --test-threads=1 -E "test(=pg_durable_verifier_work::agent_publications_reach_a_recovered_consumer)"'
```

If the implementer extends the existing case to prove the new seam completely instead of adding the planned case, record that equivalent selector and its new assertions; do not require two copies of the same observation. Record nonzero selected cases and actual assertions. Source/comment/import closure is static; run normal affected formatting/lint checks from the repository tasks (`fmt`, `lints`) and `git diff --check`. Cargo-backed checks run sequentially on the shared target. No codegen inputs should change; no full SDK/language journeys, full server integration suite, gate, broad aggregate, benchmark, synthetic load or original Task-001 fault-matrix rerun. Original five independent focused passes and other supplied R1 results remain carried evidence.

Append concise obligation-to-assertion/command results for this task, preserving historical limitations. The next review checks only these two closures, the R1-candidate-to-R2-candidate diff and affected invariants/callers, with unchanged acceptance carried forward. No commits or pushes without caller instruction.

## Obligations

| ID | Source | Observation | Planned assertion |
|---|---|---|---|
| R2-O5.2a | FIND-5 table row; correction ¶ | A fresh accepted File publication's first response does not reach its modeled caller (explicit recipient and loss point) | caller's reply channel yields an error after the server drops result + sender |
| R2-O5.2b | FIND-5 row; constraints "acceptance separately from delivery" | Accepted state is independently observed to retain the publication | Bifrost-read entry equals the exact File manifest |
| R2-O5.2c | FIND-5 row | The same call/producer/name/arguments retried against restored state returns the original receipt | retry against reclaimed RunState returns `PublicationKind::File` |
| R2-O5.2d | FIND-5 row | Retry with scratch unavailable and no new upload | scratch file removed and evidence store mode 000 during retry |
| R2-O5.2e | FIND-5 row | Accepted revision and entry unchanged | revision equal; read content equal; entries list one row |
| R2-O5.2f | FIND-5 row; constraints | Existing recovered-consumer exact assertions still pass | existing case passes unchanged assertions |
| R2-O5.2g | FIND-5 row | Unsupported lost-reply rustdoc corrected | rustdoc no longer claims a lost reply |
| R2-C1 | Constraints ¶ | Deterministic dropped-run proof kept | `dropped_agent_work_makes_no_later_call` untouched by the diff |
| R2-O7.1 | FIND-7 row | Caller, InvocationRecord, Value imported at module scope; changed signatures use bare names; same helpers/calls/assertions | static inspection of imports and signatures; fmt, lints |

## R2 Evidence

Candidate: `28f07d99c..e6fe5fbdd` (`17375f1bc`, `8bc5d1ff1`, `e6fe5fbdd`; the last rewraps rustdoc only). Verification-only: the behavior already existed, so the new case passed on first run. No production change and no RED; none was manufactured.

| Obligation | Implementation evidence | Assertion | Verification evidence | Result |
|---|---|---|---|---|
| R2-O5.2a | test-only responder owns the caller's oneshot sender, publishes, and drops the sender instead of sending its success; the caller awaits the receiver concurrently (`tokio::join!`) | `pg_durable_verifier_work.rs:1938-1955`: first result `== Ok(File)`, caller receive is `Err` | `publication_receipt_survives_a_lost_reply` PASS | PASS |
| R2-O5.2b | `leased.revision()` Bifrost read after the loss | `:1957-1977` | same | PASS |
| R2-O5.2c | `leased.reclaim()` then a new `Publications` retries `call_1`/`draft`#1/`report`/`/work/report.md` | `:1980`, `:1995-1999` | same | PASS |
| R2-O5.2d | `remove_file` of scratch; evidence store `0o000` around the retry | `:1981-1994` | same | PASS |
| R2-O5.2e | revision, content and entries compared after the retry | `:2000-2014` | same | PASS |
| R2-O5.2f | `agent_publications_reach_a_recovered_consumer` unchanged apart from rustdoc | recovery attempts `:1752-1756`, blind-inventory refusal `:1770-1785`, replay `:1813-1823`, no drafter rerun `:1842-1846`, attempts/exact mount and read digest `:1860-1898` | exact selector PASS | PASS |
| R2-O5.2g | rustdoc now says the drafter already received the receipt | `:1693-1695` | static | PASS |
| R2-C1 | not in the R2 diff (`git diff 28f07d99c 8bc5d1ff1` does not touch it) | `pg_durable_verifier_work.rs:2033` onward, unchanged | carried R1 pass; static diff inspection | PASS |
| R2-O7.1 | `use wyrd_server::components::auth::Caller` (`:27`), `InvocationRecord` added to the existing `wyrd_spec::verification::state` import (`:56`), `use serde_json::Value` in `state.rs` tests (`state.rs:1080`); bare names at `:152`, `:1436`, `:1439`, `state.rs:1370`, `:1448`; the two existing `Caller { .. }` literals use the imported name | grep finds no `components::auth::Caller`, `state::InvocationRecord` or `serde_json::Value` paths left in either file | `mise run fmt`, `mise run lints` exit 0 | PASS |

Commands (exit 0): `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --features test-support --test integration --run-ignored=all --test-threads=1 -E "test(=pg_durable_verifier_work::publication_receipt_survives_a_lost_reply) | test(=pg_durable_verifier_work::agent_publications_reach_a_recovered_consumer)"'` → 2 run, 2 passed; `mise run fmt`; `mise run lints`; `git diff --check`.

Deviation: the two planned selectors were run in one nextest filter, not two invocations. The retry runs against a reclaimed (Bifrost-restored) `RunState`, not the original writer, so it also covers the "restored state" case.

Non-goals kept out: no production, cache, transport, replay index, public resume API, scheduler, fixture framework, state format or upload-owner change. No other qualified paths were swept, and no SDK journeys, full suites or gate were run. Untouched: `.agents/skills/*` (pre-existing edits) and `changes/` (uncommitted). R1 evidence and the historical production-before-RED record are kept as they were.

Process exception: the Obligations ledger was written after the edits, not before.

Checker round 1 (fresh, read-only) found R2-O5.2a was vacuous: the test dropped the sender itself, and the first result was never checked. Fixed in `8bc5d1ff1`: the responder owns the sender, and the first result is asserted `Ok(File)` before the loss. It also noted the missing dropped-run row (added as R2-C1) and partial R2-O5.2f citations (expanded). After `8bc5d1ff1`: both exact selectors 2/2 PASS, and `mise run lints` and `git diff --check` exit 0.
Checker round 2: no gaps. It flagged two citations off by about a line (corrected) and two over-long rustdoc lines (rewrapped; one added comment line moves the new test's anchors down by one, and the anchors above are updated).
