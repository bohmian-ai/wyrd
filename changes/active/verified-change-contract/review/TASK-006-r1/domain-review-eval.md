# TASK-006 Eval Domain Review

**Immutable subject:** base `f8811ac5035c3aa165d34c38992f9889b3c9081f`, candidate `55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`

**Reviewed boundary:** Continuous Eval from acknowledged `vala.eval.observations` rows through frozen-record lookup, sampling, trace wait/read, `ScenarioScoring`, deterministic and LLM-judge task execution, named media resolution, context capture, pass-gate/common-verdict mapping, canonical result-item/summary projection, publication, and generic settlement/dispatch. The review also inspected the focused engine tests, SQL seam tests, and the real-server terminal-matrix journey.

## Authority and Source Coverage

| Authority or source | Coverage |
|---|---|
| `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md` | Repository review, test-tier, error-path, bounded-work, and immutable-subject rules |
| `changes/active/verified-change-contract/spec.md` revision 35 | `REQ-077`, `REQ-083`–`085`, `REQ-111`, `REQ-130`, `REQ-131`, `REQ-152`, `INV-004`, `INV-010`, `INV-012`, `INV-015`, `AC-014`, `AC-016`, `AC-020`, `AC-027`, `AC-033` |
| `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md` | All six ordered implementation scenarios, acceptance criteria, prohibited changes, and declared evidence |
| `changes/active/verified-change-contract/architecture/verifier/eval.md`; `architecture/references/domain/evaluation.md`; `changes/active/verified-change-contract/architecture/logic/table_schema.md`; `architecture/bifrost-design.md` | Eval terminal matrix, evidence fidelity/reproducibility, canonical result schemas, and daily partition contract |
| `crates/vala/vala-eval` | Executor fanout/error propagation, assertion/trace/judge tasks, media bindings, sampling, workflow reports, gate and capture semantics, production Skald invoker |
| `crates/wyrd/wyrd-server/src/verification/{eval,engines,results,runner,observations}.rs` | Continuous adapter, Oracle reads, media resolver, common verdict, canonical projection, publication and settlement |
| `crates/vala/vala-bifrost-redux`; `crates/wyrd/wyrd-sql`; `crates/wyrd/wyrd-storage` | Post-ACK hook, observation key extraction, durable queue/trace wait, object reads, and table schemas |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs` and focused unit/integration tests | Terminal-matrix, provider capture, media refusals, trace wait, persistence, retry, and restart proof |

## Review Findings

### Critical

None.

### Important

- **EVAL-DOM-001 — REQ-131 / INV-012: one record's complete media set is incorrectly applied to every judge Prompt.** `ScenarioScoring::score_record` creates one global `MediaBindings` from every record descriptor and attaches it to the shared snapshot (`crates/vala/vala-eval/src/orchestrator/scoring.rs:178-199`). Every `LlmJudgeTask` forwards that entire set (`crates/vala/vala-eval/src/tasks/judge.rs:60-70`), and `SkaldJudgeInvoker::bind_media` attempts to bind every descriptor into the current task's resolved Prompt, returning a terminal error when any ID is absent (`crates/vala/vala-eval/src/orchestrator/judge.rs:84-111`). `EvalSpec.tasks` permits multiple judge tasks, and REQ-131 requires each named binding to be validated against the resolved judge Prompt, not every Prompt in the run. A valid Eval with judge A declaring `${media:image}` and judge B declaring no media (or a different binding) reaches judge B and errors instead of producing its assertion, so the run retries and ultimately becomes `errored`. **Correction:** at the existing Skald invocation boundary, bind only descriptors declared by that resolved Prompt while still letting Prompt rendering reject any declared-but-missing binding; retain validation that each supplied descriptor is consumed by at least one executable judge Prompt. Add a focused two-judge engine test with disjoint media needs and a production-invoker test for an unrelated-media judge, plus a continuous-path missing-binding case.

- **EVAL-DOM-002 — REQ-083 / REQ-111 / INV-012: the continuous trace adapter discards trace evidence that the existing executor exposes.** The server query selects neither `events` nor `links` (`crates/wyrd/wyrd-server/src/verification/eval.rs:394-407`), and `span` hard-codes both vectors and their dropped counts to empty/zero (`crates/wyrd/wyrd-server/src/verification/eval.rs:466-496`). The existing trace executor deliberately exposes `span.events` and `span.links` to authored `span_selector` JSONPaths (`crates/vala/vala-eval/src/tasks/trace.rs:124-146`), while Bifrost's canonical span table persists both nested columns (`crates/vala/vala-bifrost-redux/src/tables/traces/projection.rs:418-470`). Consequently, a sampled-in continuous Eval assertion over an event name, event attribute, linked trace, or link attribute evaluates fabricated empty evidence and can produce a false assertion/verdict instead of preserving existing trace semantics. **Correction:** extend the existing `BifrostReader::spans` projection/decoder to reconstruct the persisted `SpanEvent` and `SpanLink` values and dropped counts into `SpanRecord`; do not add another trace model. Add a real-server trace assertion over at least one event and one link and assert the expected canonical item/verdict.

- **EVAL-DOM-003 — Evaluation reproducibility / REQ-083: multi-span trace assertions have nondeterministic input order.** `BifrostReader::spans` issues a multi-row query without `ORDER BY` and pushes batches/rows in returned physical order (`crates/wyrd/wyrd-server/src/verification/eval.rs:394-423`). The retained executor contract supports positional selectors such as `$.spans[0]` (`crates/vala/vala-eval/src/tasks/trace.rs:124-146`), and the shipped journey itself authors positional selectors but exercises only one span (`crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:144`). Oracle/Parquet order is not a stable logical ordering across hot/fused reads, compaction, retry, or restart. The same committed trace can therefore produce different assertion results and common verdicts. **Correction:** impose one stable logical order in the existing trace query, using persisted trace facts (for example start timestamp then span ID as a total-order tiebreaker), before constructing `SpanRecord`s. Prove it with a multi-span real-server case whose physical insertion order differs from the asserted logical order and remains stable after a reread/restart.

- **EVAL-DOM-004 — REQ-131: the 20 MiB media ceiling does not bound the bytes actually read.** `TenantMedia::resolve` stats the object and checks that metadata, then performs a separate unbounded `get_object` and immediately base64-encodes the returned body without validating its length (`crates/wyrd/wyrd-server/src/verification/eval.rs:702-753`). `StorageHandle::get_object` materializes the entire backend response (`crates/wyrd/wyrd-storage/src/handle.rs:272-303`), and the same handle permits object replacement. An object replaced between stat and read can exceed the declared ceiling, consuming unbounded memory and reaching the provider despite REQ-131's bounded-byte requirement. **Correction:** make the storage-owner read itself bounded (read at most `MEDIA_LIMIT_BYTES + 1`, then reject the sentinel extra byte) and use that single returned body for provider binding; the metadata check may remain only as a fast rejection. Add a storage test that changes or reports stale metadata while the body exceeds the limit and proves the resolver neither materializes nor forwards more than the cap.

- **EVAL-DOM-005 — AC-014: the required cross-day partition-pruning journey is absent.** AC-014 requires the real continuous journey to prove frozen `wyrd_event_time` selects the correct UTC-day partition even when client `created_at` falls on another day. The journey helper offers no `created_at` control and always calls `EvalObservationOptions::from_parts` with only media/trace options (`crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:215-226`); the later assertion proves only that the run and row have the same event time (`crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:762-780`). Thus the test can pass if record lookup accidentally prunes by `created_at` whenever both timestamps happen to share a day, leaving an explicit acceptance obligation unproved. **Correction:** add a real-SDK observation whose authored `created_at` is on a different UTC day from its committed managed event time, complete its run, and assert the exact record/result was read through the frozen managed-day selector.

### Suggestions

None; optional improvements are outside this acceptance audit.

## Open Questions

None. The findings are resolvable within the approved behavior and existing owners.

## Verification Notes

- Reviewed the complete supplied base-to-candidate diff and the current bodies/callers of the Eval engine, adapter, queue, storage, result mapper, and tests. CodeGraph was unavailable because this repository has no `.codegraph/` index.
- No source was modified and no verification command was rerun. The task artifact records successful focused `vala-eval` tests plus `mise run test:vala`, `test:sql`, `test:wyrd`, `test:bifrost`, `test:wyrdstate:journey`, `fmt`, `lints`, and `git diff --check`; this review treats those as reported evidence, not independently reproduced evidence.
- Existing proof covers the one-span trace path and one media-bearing judge. It does not cover preserved event/link evidence, deterministic multi-span ordering, multiple judges with different media requirements, a raced/stale media-size read, or AC-014's different-day `created_at` case.
- During review, repository `HEAD` advanced from the supplied candidate to `01146cf87d22147b87d0c9224aa2bdf67decad92`, but the only committed delta was repository skill documentation; the explicit candidate commit and every reviewed production/test source remained unchanged. The report remains anchored to candidate `55c5bff84fbfcc8f43967a8ad3aeac6057f10c3f`.

## Overall Result

**FAIL** — five bounded, task-required correctness or acceptance-proof gaps remain in continuous Eval trace/media semantics and the required journey coverage.
