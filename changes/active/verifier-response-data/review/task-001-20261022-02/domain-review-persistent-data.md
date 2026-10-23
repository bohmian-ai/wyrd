# Persistent-data domain review

Result: **PASS** within the persistent-data/durability boundary. Proposed findings: **none**.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Base: `4d0f7005888a899910008399d1b5e15e2a3207f9`.
Candidate: `9e56fc340cb5fd769d3b34f47433e2719339fdfb`.
Reviewed cumulative range: `git diff 4d0f7005888a899910008399d1b5e15e2a3207f9...9e56fc340cb5fd769d3b34f47433e2719339fdfb`.
HEAD remained the updated candidate during this review. Existing local skill/workflow edits and all other reviewers' reports were excluded. No other discovery reports were read. Attempt 01 was blocked when the candidate changed; its domain analysis was rechecked here against the full original-base-to-updated-candidate diff. Committed source is the acceptance subject; later working-tree edits are excluded.

This review follows accepted response data from the code/Agent producers through whole and step requirement results, direct and queued persistence, delayed readback, and the published outcome Variant. It also checks invalid-data omission, standard outcome/actual behavior, and local typed-read immutability. Permission interpretation is owned by the independent security review; this PASS does not resolve the spec's elevated-payload wording.

## Authority and coverage

| Authority | Obligation and inspected boundary |
| --- | --- |
| Root `AGENTS.md`, `changes/spec-driven-development.md`, `agent_docs/{index,rust,errors,maintainer-style,agent-surfaces}.md`, `TESTING.md` | Existing owners, durable server behavior, immutable acceptance review, scoped proof. No nested AGENTS applies to the changed production modules in this domain. |
| `changes/active/verifier-response-data/spec.md` REQ-002, REQ-007, REQ-011, REQ-012; AC-002/004/005/007 | Step and whole requirement data, invalid output, retained/published results, no durable mutation by client decoding. |
| Original TASK-001 scenarios 2–4 and implementation evidence | Validation, readback, typed read expectations and existing verification claims. |
| `architecture/wyrd-design.md:1320–1364`, `:653–669` | Accepted data belongs to RequirementResult, aggregation/citations stay separate, result writes retain the existing Scribe retry owner. |
| `architecture/bifrost-design.md:159–270` | One outcome Variant; exact nesting/value preservation, existing Variant numeric/size/depth limits, no new column/store. |
| `architecture/README.md` | Owning-authority priority. |

CodeGraph exploration preceded source searches. Changed owners inspected include `Verdict`, `RequirementResult`, generic result/outcome projections and `decode_data` in `wyrd-spec/src/verification/execution.rs`; `verdict_from`; Agent and sandbox code executors; `Judged` and the building-block engine; and the server result builder. Existing adjacent owners traced include direct `Service::stage_direct`, queued `VerifierRunner::result`, completion staging, `ScribeWrite::Result`, `ResultsTable`, and `Service::read_result`/`get_execution`. The unchanged existing store/lifecycle was considered explicitly rather than inferred from a green test.

## Updated-candidate cumulative recheck

The complete original-base-to-`9e56fc340cb5fd769d3b34f47433e2719339fdfb` range was reviewed again for this domain, including the code/Agent validation producers, Judged/report transfer, generic serialized outcome contract, result staging/publication, and readback. Exact Git comparison confirms the previously traced production owners, architecture storage authority, and Rust/MCP persistence proofs have identical blobs in both candidates. The cumulative diff still adds no table, schema migration, column, durable queue, retention setting, publication worker, or alternative result store. Committed candidate bodies of `ResultPayloadBuilder::build/summary`, `Service::read_result`, and completion commit/stage were read explicitly again: full-outcome Variant encoding, selected-result readback, and stage-before-commit/take-once behavior remain as evidenced below.

The additional commit was also inspected in full. Its lockfile updates only the dev type checker `ty` (0.0.39 → 0.0.91); the committed `pyproject.toml` places ty in the development dependency group. Generator/AsyncGenerator annotations do not alter fixture cleanup or server lifetimes. The extra typed `.get("data")` assertion is static proof and cannot affect stored values. The `HttpAuth.auth_flow = sync_auth_flow` change sends authentication through the existing per-request token/header method; its full committed class body has no result conversion, storage, retry, or lifecycle ownership. None of these changes introduces a persistent-data/durability consequence. The task no longer records the old `.get` typing limitation. Whether these ancillary changes meet task scope and broader SDK standards remains with the corresponding independent reviewers; this domain result is confined to durable result correctness.

No dependency installation, synchronization, environment mutation, tests, or broad checks were run for this recheck. Only the assigned report was written.

## End-to-end evidence

| Required property | Source and verification evidence | Result |
| --- | --- | --- |
| Valid data is validated once at the check boundary and reaches the requirement unchanged | `blocks.rs:197–211` validates the entire declared response, deserializes Verdict, and applies limits. Agent `blocks/agent.rs:472–508` and code `hosted/sandbox.rs:364–391` both call it before emitting CheckOutput. `Judged::checked` at `engine.rs:974–988` transfers the accepted map. | PASS |
| Whole and step requirements share the same data channel | `engine.rs:615–630` dispatches both positions through CheckOutput/Judged; `Judged::report` at `:910–925` sets optional step identity and `data`. `Engine::run` at `:324–343` collects all Done::Requirement entries into the one outcome. Rust journey `assert_scored` at `verifier_building_blocks.rs:1327–1357` checks Agent, whole code and step code data plus the step identity. | PASS |
| Rejected data is absent, and other valid requirements survive | Either executor returns BlockError on schema or bounds refusal. `engine.rs:630` takes `Judged::inconclusive`; `Judged::bare` at `:940–950` sets `data: None`. Requirement serialization omits None (`execution.rs:258–266`). Rust journey at `:1425–1461` asserts BAD/HUGE Agent and code data absent, valid sibling step data present, and unchanged aggregate handling. Agent integration at `blocks_agent.rs:585–677` includes missing/null/wrong/unknown fields, size/depth and plain-format refusal. | PASS |
| Data neither replaces actual capture nor drives aggregate verdict | `Judged::report` still applies `capture_actual` exclusively to actual at `engine.rs:919`; data is transferred separately at `:921`. Aggregation at `:331–334` reads only requirement outcomes; executor actual remains None. Citation filtering/downgrade still modifies citations/outcome/reason rather than synthesizing a data judgment. | PASS |
| Direct and queued writers retain the same complete outcome, with no new persistence mechanism | Direct service `service.rs:606–625` stages and returns the same outcome; `stage_direct` at `:741–792` uses the existing ResultPayloadBuilder/ScribeWrite pair. Queued `runner.rs:843–898` uses the same builder and records its selected result id. `results.rs:254–268,282–323` serializes the full outcome and encodes it as OUTCOME Variant. Result table/schema/layout are unchanged. | PASS |
| Queued completion and delayed visibility retain their previous semantics | `completions.rs:198–227` stages an accepted completion's existing write on its reserved slot before commit and takes it once; stale settlement stages nothing. No candidate changes alter this boundary or rerun policy. `service.rs:189–235,246–295` reads the selected result id from tenant status and decodes the whole stored Variant into the extended outcome; temporarily invisible results remain absent. | PASS |
| Published values preserve their existing retention/storage boundary | `ResultsTable` keeps the outcome in its existing sensitive Variant and retains its existing physical layout. No data-specific column, table, WAL, queue, migration, or cleanup path was added. `results.rs:316–321` uses the existing EncodedVariant owner, so Variant bounds still apply in addition to per-data bounds. Rust journey `custom_verdict_data_survives_readback` at `:1369–1414` compares the entire flushed Bifrost outcome with the queued result, including plain omissions and step data; MCP journey at `verification.rs:876–967` checks direct/queued data and a flushed public SQL data projection. | PASS |
| Typed decoding cannot mutate the retained result or trigger another execution | `VerificationResult::decode_data` in `execution.rs:477–516` consumes the local result and collects conversions before constructing the typed envelope; it has no runtime/store dependency. Execution-state decoding only maps an already-present result. Rust typed mismatch journey at `verifier_building_blocks.rs:1544–1582` rereads the ordinary stored result after typed failures. SDK siblings reuse ordinary execution/status/wait workflows. | PASS |
| No data is copied into result metadata/Operator failure context | `ResultPayloadBuilder::summary` serializes the map only into OUTCOME; other named columns remain identities/verdict/timing. `engines.rs:44–59` derives the Operator summary only from verdict and counts; drift counts use existing detail. The data propagation additions do not add logs, audit fields, or metadata projections. | PASS |

## Existing-owner comparison

The existing ResultPayloadBuilder → outcome Variant → Scribe result write → ResultsTable → ordinary Bifrost query/read_result path already supports an added optional nested member. The candidate extends that contract and the Judged transfer; no replacement lifecycle or persistence owner was introduced. A new store, typed-data table, column, publication worker, or per-data retention mechanism would duplicate that existing workflow and is unnecessary. The ordinary Variant domain/bounds and existing asynchronous visibility remain intact; the specification explicitly retains those limits.

## Verification limits

This was an independent static domain audit. I inspected actual test source and the task's recorded passing focused commands; I did not rerun Cargo, SDK/server journeys, crash tests, cloud storage, or broad gates. Existing restart/retry/retention machinery is unchanged; the review does not claim newly executed crash-recovery proof. No source was changed. The separately assigned security review must reconcile elevated payload-read language with effective query permissions.

Final domain finding ledger: **empty**.
