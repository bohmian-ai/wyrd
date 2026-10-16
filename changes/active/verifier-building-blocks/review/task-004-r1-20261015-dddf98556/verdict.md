# TASK-004 R1 re-review — FIX_REQUIRED

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-004`.
- Branch: `change/verifier-building-blocks-task-004`.
- Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
- Candidate: `dddf985563029a004fa768110e36d0c5f12bcded` (HEAD, unchanged throughout review).
- Prior candidate: `07adb7907951f343544e550f6cabc5b7aa3aadea`.
- Approved authority: `changes/active/verifier-building-blocks/spec.md` r8; original `tasks/TASK-004-sdks-examples-and-docs.md`; prior review `task-004-20261015-07adb7907/verdict.md` and `TASK-004-R1-sdk-contract-and-journey-closure.md`, including implementation evidence. Snapshots and complete cumulative/remediation diffs are preserved here.

This fresh review covers the complete base-to-candidate range, with remediation emphasis. All owner scope decisions are accepted: extra edits (including the fixture validation test and PDF binary rule) are approved; no spec revision is needed; FIND-004-7 is superseded by r8; external examples and provisional code.run arguments belong to TASK-005. Journeys require authorship, compilation/typechecking and discovery here; execution remains the merged-change obligation after TASK-005.

## Independent review results and reconciliation

| Role | Report | Discovery result | Reconciliation |
|---|---|---|---|
| Caller behavior | task-review-behavior.md | FAIL | Deadline proposal confirmed; full FIND-004-8 closure claim rejected by source validation. |
| Values/invariants | task-review-invariants.md | FAIL | Fresh independent replacement pass; deadline confirmed, Check closure claim rejected. |
| Repository standards | standards-review.md | FAIL | Explicit panic-documentation proposal confirmed; Check closure claim rejected. |
| Maintainer | maintainer-review.md | FAIL | Same deadline cause consolidated under FIND-004-1. |
| System resilience | system-review.md | FAIL | Same request-local deadline cause; no shared-server availability change needed. |
| Security/tenant/evidence | domain-review-security.md | PASS | No material proposal; server enforcement proof remains deferred. |
| Concurrency/recovery | domain-review-concurrency.md | FAIL | Same initial connection/gate cause consolidated. |
| Focused follow-up | followup-review.md | RESOLVED | Closed Check schema is not arbitrary metadata; retained FIND-004-8 proposed and independently confirmed. |
| Structured Ponytail validation | findings-validation.md | Three confirmed findings | Final source-validated ledger governs this verdict. |

The focused follow-up was required because reports disagreed materially about the remaining Check field, not to obtain agreement. `claims-comparison.md` records routing and independence handling. An inadvertent peer-closure message to the first invariant reviewer caused its report to be retained only as `supplemental-invariants.md`; a separate fresh reviewer received no peer conclusions and produced the required independent invariant report. All supplemental proposals were validated too. All required reports are complete; no role was simulated by the orchestrator.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Shared typed named/positional input projection; slot/type refusal | Shared Target/Inputs; TS nativeInputs and native Inputs::One/Named; Python thin projection | Owner tests authored; scoped Clippy, typechecks and journey discovery | PASS |
| Lazy evidence, exact input identity, shared upload reuse | File/ArtifactRef, RunEvidence::pin, WyrdStorageClient upload dispatcher | Source/caller audit, fixture test, nine tracked assets | PASS |
| Run verify/queued submit vs Window Recorded; stable replay identity and honest status/results | Observe/Target and shared Execution owner; retained client; no fabricated completion result | Existing exact tests compile/select; execution blocked by sandbox socket permission | PASS for source/authored proof, runtime limitation recorded |
| Caller timeout bounds the entire public wait operation | Shared timeout_at is correct; Python lazy handle acquisition still precedes it | Source trace and independently reproduced API-key wait(0) transport error | FAIL — FIND-004-1 |
| Python local ID construction, eager connect, retained queued handle and failed-first-use retry | PyExecution + existing LazyConnection::new/ready/get | Existing Python tests, typecheck; deadline defect counted once above | PASS |
| Typed fixed Python status/result/requirement/citation/evidence/provenance views | Most TypedDict wire views now precise; RequirementResult.check remains dict[str, JsonValue] | Existing type fixture passes but accepts an impossible Agent shape; closed Check source establishes missing case | FAIL — FIND-004-8 |
| All required action/check negative rows, hosted tools/exact scope/delegation, scheduled ready SPC in each SDK | Three counterpart journey files; deterministic fixtures/provider scripts; real fitter/scheduler retained | Rust integration check/list; Python typecheck/19-case collection; TS typecheck/list | PASS at approved authored-proof stage |
| Scheduled/reconnect/attribution execution identity | All three scheduled readers and all three attribution filters use execution_id; result_id joins retained | Source and language compile/typecheck | PASS |
| Matched canonical examples, extraction/runtime/tool references, architecture/generation sources | Shared policy story and three language entrypoints, cumulative documentation and generator changes | Independent source audit; earlier owner evidence retained; no new end-to-end claim | PASS within TASK-004 ownership |
| New touched Rust documentation | Original ownership fields fixed; new fixture helper/test omit panic sections | Exact AGENTS §16 and full-body source audit | FAIL — FIND-004-10 |
| Non-goals and boundaries | No SDK durable engine, queue, authorization policy, second uploader/transport or compatibility layer required by R2 | All specialist owner/caller audits | PASS |

Detailed obligation rows and exact source evidence remain in the two task-review matrices and final validation ledger. Passing compile or discovery does not stand in for the deferred real-server proof.

## Validated findings

| ID | Status / classification | Exact location | Observable consequence / required correction |
|---|---|---|---|
| FIND-004-1 | CONFIRMED / INCORRECT; partial prior closure | sdks/wyrd-sdk-python/src/verify.rs:201; client.rs:140-163 | First lazy wait acquires/initializes its connection before shared wait starts the caller deadline. Bound initialization contention, credential IO and shared polling by one caller budget using existing owners. |
| FIND-004-8 | CONFIRMED / MISSING; partial prior closure | sdks/wyrd-sdk-python/python/wyrd/verify/__init__.pyi:176; tests/typecheck/verify_contracts.py:63 | Closed Check schema is erased into arbitrary JSON, allowing impossible kinds/configurations. Finish the authored discriminated wire views with actual fixed fields and real JSON leaves; preserve runtime dictionaries. |
| FIND-004-10 | CONFIRMED / VIOLATION; new | crates/wyrd-spec/tests/verifier_fixtures.rs:171 and :187 | Two new items miss mandatory panic contracts under AGENTS §16. Add the two documentation sections in the same substantive remediation; no behavior change or separate style round. |

No rejected proposal or optional improvement enters the ledger. See `findings-validation.md` for complete producer/sibling traces, proof and selected minimal correction boundaries.

## Prior-finding closure

| Prior finding | Disposition |
|---|---|
| FIND-004-1 | OPEN: shared status/poll budget fixed; new Python implicit first connection and initialization gate remain outside it. |
| FIND-004-2 | CLOSED: existing LazyConnection owns offline/eager/ready/retained/retry lifecycle. |
| FIND-004-3 | CLOSED: both TS methods support single-slot positional values, named maps and explicit object disambiguation through shared Inputs. |
| FIND-004-4 | CLOSED: all nine samples tracked and consumed from samples/ by all three SDKs. |
| FIND-004-5 | CLOSED at approved authored-proof stage: concrete three-language negative/tool/trace/SPC cases compiled/typechecked/discovered; execution explicitly deferred. |
| FIND-004-6 | CLOSED: six execution correlation consumers use execution_id; genuine result joins remain result_id. |
| FIND-004-7 | SUPERSEDED by r8/human authority; external repositories now TASK-005. |
| FIND-004-8 | OPEN: most fixed records corrected; exact Check remains erased. |
| FIND-004-9 | CLOSED: original wrapper ownership fields now documented; new fixture documentation uses FIND-004-10. |

## Verification and limits

Independent narrow checks passed: exact fixture validator (1 selected/1 passed); Rust integration compilation and nextest discovery; all-feature library Clippy for wyrd-client/wyrd-ai/wyrd-sdk-python/wyrd-sdk-ts; cargo fmt check; Python lint/stub validation, format and typecheck; Python integration collection; existing installed TS compiler, Vitest discovery and Biome lint/format; tracked nine-format candidate-tree proof; source-only cumulative whitespace check.

Three exact shared wait/replay tests compiled and were selected but could not bind wiremock ports in this sandbox. The new deadline test was rerun with WYRD_LOG tracing and failed at the same OS permission boundary before assertions. No runtime pass is claimed. Independent public Python zero-budget probes nevertheless reproduce the wrong transport error without a listener. Temporary UV cache and existing installed TS/venv executables avoided read-only global cache failures; logs preserve original attempts and successful alternatives. Historical committed review *.diff files contain whitespace; their provenance was not rewritten. N-API/codegen/docs/full journey aggregates were not rerun in this focused remediation review; existing evidence and source parity were audited, with merged proof still outstanding.

No full journeys or gate ran. No environment variables were printed. No production source was modified, committed or pushed. Candidate identity and tracked source remained unchanged.

## Verdict

**FIX_REQUIRED** — bounded implementation/declaration/documentation corrections remain under the approved spec.

Remediation: [TASK-004-R2-python-wait-and-typed-check-closure.md](TASK-004-R2-python-wait-and-typed-check-closure.md), routed to `$wyrd-implement`. This review does not implement that task or grant merge/completion approval.
