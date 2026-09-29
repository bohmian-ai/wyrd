# TASK-006 R2 Eval Domain Review

**Immutable subject:** base `f8811ac5035c3aa165d34c38992f9889b3c9081f`, candidate `3593bbc31273673f87159315fbf66a73562d3c99`

**Reviewed boundary:** acknowledged `vala.eval.observations` rows through best-effort activation, immutable sampling, exact-record and trace reads, the existing `ScenarioScoring` / `EvalExecutor` path, deterministic and LLM-judge tasks, named media resolution, context capture, common-verdict mapping, canonical item/summary publication, and terminal settlement. The review included the original task, the R1 validated findings and remediation task, cumulative source, and the recorded focused and broad verification evidence.

## Authority and Source Coverage

| Authority or source | Coverage |
|---|---|
| `AGENTS.md`; `architecture/agent-rules.md`; `architecture/references/languages/spec-driven-development.md` | User-journey priority, negative-flow proof, bounded work, error semantics, and immutable review rules |
| `changes/active/verified-change-contract/spec.md` revision 36 | `REQ-077`, `REQ-083`–`086`, `REQ-111`, `REQ-130`, `REQ-131`, `REQ-152`, `INV-004`, `INV-010`, `INV-012`, `INV-015`, `AC-014`, `AC-016`, `AC-020`, `AC-027`, `AC-033` |
| `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md` | Six execution scenarios, prohibited changes, acceptance criteria, and declared evidence |
| `changes/active/verified-change-contract/review/TASK-006-r1/{verdict.md,findings-validation.md,TASK-006-R1-continuous-eval-closure.md}` | Every prior finding, approved correction boundary, and claimed closure proof |
| `changes/active/verified-change-contract/architecture/verifier/eval.md`; `architecture/references/domain/evaluation.md` | Terminal matrix, existing-engine reuse, evidence fidelity, trace lifecycle, capture, and media semantics |
| `crates/vala/vala-eval` | Sampling, executor propagation, DAG/task outcomes, judge and media flow, trace projection consumed by tasks, gate, and capture behavior |
| `crates/wyrd/wyrd-server/src/verification/{eval,engines,observations,results,runner}.rs` | Continuous adapter, System-authorized input reads, bounded/ordered trace decoding, media resolution, public errors, projection, publication, and settlement |
| `crates/wyrd/wyrd-sql`; `crates/vala/vala-bifrost-redux`; `crates/wyrd/wyrd-storage` | Immutable observation ordinal, first-commit activation, persisted observation/trace shape, and bounded object reads |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`; focused `vala-eval`, server, SQL, and storage tests | Terminal matrix, replay, cross-day lookup, trace evidence/ceiling, System read authority, stable errors, media/provider flow, retry, and restart proof |

## Prior-Finding Closure

| Prior finding | Evaluation-domain result |
|---|---|
| `FIND-TASK-006-1` | PASS — Scribe's first-commit disposition gates activation; the later-day sealed replay journey proves one run with the committed row's event time. |
| `FIND-TASK-006-2` | PASS — enqueue stores a serialized per-binding observation ordinal and claims reuse it across retry/restart. |
| `FIND-TASK-006-3` | PASS — `StorageHandle::get_object_bounded` retains at most limit-plus-one and `TenantMedia` refuses the effective oversized body before encoding/provider work. |
| `FIND-TASK-006-4` | PASS — the trace query and decoder reconstruct persisted events, links, attributes, and dropped counts into the existing `SpanRecord`. |
| `FIND-TASK-006-5` | PASS — trace evidence has a total `start_time_unix_nano, span_id` order, exercised across repeated execution/restart. |
| `FIND-TASK-006-6` | PASS — the SDK journey separates authored `created_at` from the managed event day and mutation-proves lookup by frozen `wyrd_event_time`. |
| `FIND-TASK-006-7` | PASS — continuous reads use the stable persisted tenant System principal with table-scoped authority through Oracle; refusal and audit identity are exercised. |
| `FIND-TASK-006-8` | PASS — trace reads close the event-time interval, request a fixed ceiling plus one sentinel, and refuse overflow before task/provider execution. |
| `FIND-TASK-006-9` | PASS — public run errors retain stable codes/fixed text while dependency causes stay in protected diagnostics and storage locators are redacted. |
| `FIND-TASK-006-10` | PASS — `fan_out_bucket` now documents first-error propagation, sibling cancellation, and partial-progress consequences without changing the execution shape. |

## Review Findings

### Critical

None.

### Important

- **EVAL-R2-001 — MISSING: AC-027's continuous media-refusal matrix is not proved through the required real server/provider seam.** `AC-027` and TASK-006 Scenario 3 require missing binding, inaccessible/cross-tenant URI, unsupported kind/MIME, and oversized media to remain visible execution/input errors with no result, dispatch, or provider call. The real continuous journey's negative records at `crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs:727-744` cover missing assertion context and one cross-tenant URI only. Unsupported MIME, absent object, and oversized body are exercised only against `TenantMedia` in `crates/wyrd/wyrd-server/src/verification/eval.rs:1135-1192`; the nominal missing-binding test at `crates/vala/vala-eval/src/tasks/media.rs:208-223` injects `required_media`, but production `ScenarioScoring::score_record` always supplies an empty required list at `crates/vala/vala-eval/src/orchestrator/scoring.rs:184-193`. Those checks validate helpers but cannot detect a broken continuous adapter, retry/terminal mapping, publication suppression, dispatch suppression, or provider suppression for the required refusal modes. **Observable consequence:** the acceptance gate can stay green if the production continuous path turns one of these media failures into a result/verdict, dispatches, or invokes the provider. **Required correction/proof:** extend the existing `continuous_eval_runs_the_terminal_matrix` fixture and server/provider capture with records for an unbound `${media:shot}`, an inaccessible object (the existing cross-tenant case may remain), unsupported kind/MIME, and an effective oversized body; prove each run exhausts/terminates `errored` with no result rows or Operator dispatch and that the local provider receives no request for the refused records. Reuse the current graph, object store, provider capture, and assertions; add no new harness or production abstraction.

### Suggestions

None.

## Verification Limits

- This was a static acceptance audit of the immutable cumulative range plus the remediation evidence. No broad `mise` lane was rerun in this review wave.
- The candidate remained `3593bbc31273673f87159315fbf66a73562d3c99` throughout inspection. The shared review directory contains other reviewers' concurrent artifacts, which were not used as conclusions or modified here.
- Recorded evidence reports passing focused engine regressions, `test:vala`, `test:sql`, `test:wyrd`, every Bifrost unit/integration/journey lane, `test:wyrdstate:journey`, format, lints, codegen, boundary checks, and `git diff --check`. Those green lanes do not close `EVAL-R2-001` because none drives the missing continuous negative cases.

## Overall Result

**FAIL** — the implementation and all ten prior evaluation-relevant findings are closed, but the explicit real-server/provider negative-media acceptance obligation remains unproved.
