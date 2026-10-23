# Persistent-data domain review

**FAIL** — accepted-data persistence and readback pass source inspection; one explicitly required Agent boundary proof remains incomplete (DATA-1). This is a proof gap, not a demonstrated production storage defect.

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `fb1fd9234acf9ff20b1feff89bf9f3da9cba4f14`.
- Candidate: `d2ffc59873b5af688adf1d1e5b2a3a388160bb83`.
- Independently read the cumulative diff for the data producer, engine, result contracts, result projection, outbox, shared client, Python and TypeScript conversions, and their relevant tests. Used the remediation range and attempt-02 verdict/validated ledger/R1 task as closure hypotheses, not acceptance authority.
- HEAD remained the candidate at the final inspection. Only this assigned report was written. Other working-tree workflow edits and concurrent reports were excluded.

Domain boundary: a code/Agent response becomes accepted optional requirement data, or is refused; accepted data enters immediate results and the existing sensitive outcome Variant, survives queued wait/status and published Bifrost reads, and remains unchanged by local typed-read mismatch. Existing result selection, tenant attribution, authorization, retention, and bounds are part of the trace.

## Authority and source coverage

| Boundary | Governing authority | Source inspected and conclusion |
|---|---|---|
| Accepted/rejected response | Approved spec REQ-002/007; task scenario 2; `architecture/wyrd-design.md` Verifier outcomes and Verdict data | `vala-verify/src/blocks.rs::verdict_from`, `blocks/agent.rs` check executor, server `verification/hosted/sandbox.rs` code executor, `wyrd-spec/src/verification/execution.rs::Verdict`. Both executors call the same exact-schema validator, decode only valid envelopes, then check compact-byte/container bounds. Refusal returns no Verdict/custom map. |
| Requirement and step results | Spec REQ-011; Verifier requirement/report authority | `vala-verify/src/blocks/engine.rs` check dispatch and `Judged::{bare,checked,cited,report}`. Accepted data transfers unchanged into each whole/step requirement; isolated failures use `bare` with `data: None`; `actual`, detail, citation filtering and aggregate verdict remain separate owners. |
| Stored representation and limits | `architecture/bifrost-design.md` Storage format and Variant, verification results; spec REQ-011 | `verification/results.rs::ResultPayloadBuilder::{build,summary}` serializes the complete outcome into the existing Variant column; no table migration, new column or store. `vala-bifrost-redux/src/tables/verification/results.rs` retains the existing sensitive outcome and selected-result identity contract. Per-data 64 KiB/16-container limits remain inside the existing whole-Variant 8 MiB/64-container limits, which may still refuse a whole oversized result as previously documented. |
| Direct and queued writes | Bifrost durability/visibility authority; existing direct/queued lifecycle | `components/verification/service.rs` direct execution and `stage_direct`; `verification/runner.rs::result` and result transition; `scribe_outbox.rs` Result rows, attribution, batch ordering, weight and delivery. Direct output uses the same accepted outcome it stages; queued completion chooses the payload result ID; batches retain tenant and attribution. Retry/staging/durability semantics are unchanged. No custom-data copy enters group activity or bounded Operator failure summary. |
| Ordinary status/readback | Spec REQ-011; security posture Tenant and data isolation | `components/verification/service.rs::{get_execution,read_result}` uses tenant-bound registry status and exact selected result ID, then authenticated ordinary Bifrost query and Variant decoding. Absent publication remains absent result. `oracle/mod.rs::authorize_payload_columns` gates gateway call payloads specifically; verification data retains query-read plus tenant scoping. |
| Old/plain results | Spec REQ-011 compatibility | Generic `RequirementResult<T>` keeps `data: Option<T>` omitted on serialization; Serde's Option missing-field behavior accepts historical results. The new pure typed-result test decodes requirements without `data`; plain named-format SDK journeys assert no data. No compatibility alias or durable-state conversion was added. |
| Typed reads and stored-state safety | Spec REQ-012 | `wyrd-client/src/observe/mod.rs::verify_typed`, `verify/execution.rs` typed status/wait/deadline siblings call their ordinary operations before consuming/decoding the returned result. Canonical `VerificationResult::decode_data` preserves every other field and returns no partial result. Python `verify.rs` converts a fresh JSON projection while retaining its Rust result; TypeScript `typedResult` maps the fetched response locally. None writes a result or initiates a second verification after mismatch. Error paths use requirement ID and safe root. |
| SDK and public persistence proof | Spec AC-002/005/006/007; original task; R1 FIND-5 | Rust/Python/TS building-block readback and mismatch stories, shared scored-review fixture, Agent integration test, HTTP tenant test and MCP round-trip test inspected. See proof assessment below. |

Read the root instructions, review skill, committed workflow, architecture authority map, maintainer guide, approved specification, original task and previous validated findings/remediation. CodeGraph was invoked before source navigation; fuzzy results were supplemented by ordinary source reads. No SQL schema, retention mechanism or migration was changed by this feature.

## Accepted payload durability and isolation assessment

**PASS within this domain.** The complete result owns the data throughout; persistence does not reconstruct or selectively copy custom fields. Generic conversion does not change the selected server outcome. Existing tenant-qualified outbox staging and authenticated Bifrost reads remain the authority, so the new field creates no alternate access path.

The source proof includes all siblings: direct execution uses `stage_direct`, queued execution uses `runner::result`, both use `ResultPayloadBuilder`; wait/status read the selected result through the same service and ordinary client workflows. Raw Bifrost SQL keeps its generic behavior. Retention and hot/published visibility inherit the existing table lifecycle, with no separate custom-data storage to outlive or bypass it.

The SDK stories compare queued status to wait, and compare the complete published outcome with the ordinary result after `flush_bifrost`. They exercise accepted Agent, hosted-code and step data, absent plain data, failed requirements and exact hosted-code size/depth cases. The HTTP case refuses another tenant's execute/status; MCP includes query-read refusal and foreign execution refusal. Typed mismatch stories perform ordinary rereads and count provider calls to show no rerun. These tests support the data path; their recorded successful executions were not independently rerun here.

## Material proposed finding

### DATA-1 — Required Agent exact-byte boundary proof misses both adjacent values

- Classification: **MISSING** verification; partial closure of prior **FIND-TASK-001-5**, not a new production defect.
- Obligation: original task scenario 2 explicitly requires code **and Agent** exact/over-limit values, Unicode, escapes and wide arrays; spec AC-005 requires exact 65,536 success and immediately larger failure. R1 requires corresponding Agent cases to be completed where needed.
- Location: `crates/vala/vala-verify/tests/integration/blocks_agent.rs:604–605`, accepted case at line 615, refused case at line 641.
- Evidence: the test defines `note(bytes) = "é".repeat(bytes / 2)` and computes `exact = 65_536 - json!({"score": 1, "note": ""}).to_string().len()`. The empty data object is 21 compact UTF-8 bytes, leaving 65,515. Integer division discards one byte, so the allegedly exact accepted data is **65,535**, and adding `s3cr3t-payload` makes the refused data **65,549**, not 65,537. A read-only compact UTF-8 self-check independently confirmed all three numbers. Other Agent accepted cases are small values or depth cases. The three SDK EXACT/OVER markers now exercise hosted code; the Agent returns its ordinary small score for those markers.
- Observable consequence: all current tests can pass without an Agent result ever demonstrating the required inclusive 65,536-byte boundary or rejection at 65,537. The comment/evidence claiming an exact Agent size boundary is stronger than the actual proof. Shared production validation looks correct; this finding does not claim truncation or lost stored data.
- Minimum safe correction: correct the existing Agent integration case values so their compact UTF-8 JSON is explicitly asserted to be 65,536 and 65,537 bytes before execution. Keep the real Agent executor/provider path and its valid-output/no-data-on-refusal assertions. Complete any remaining explicitly requested Agent escape/wide-array boundary variants in this existing test; reuse the canonical shared validator and test setup. No production guard, new storage/API, new harness or repeated HTTP/MCP read checks is justified by this gap.
- Focused closure proof: the existing exact selector `blocks_agent::check_agent_validates_custom_data` must pass with assertions establishing those lengths and expected accepted/refused behavior. The hosted-code stories already contain the missing code boundary/readback evidence and need no unrelated expansion for this finding.

## Prior-finding closure relevant to this domain

- FIND-1 schema-resource preservation: changed resolver embeds custom schemas as their own resource; shared validation and Agent regression inspect authored-root refs and dialect behavior. No data-to-envelope reference confusion found in the inspected paths.
- FIND-2/4 safe local/server refusal: SDK conversions report only requirement/root and server validation reports schema path, avoiding returned dynamic keys. Rejected data never reaches `Judged.data`; accepted stored data is unchanged by typed mismatch.
- FIND-3 declaration compatibility: inspected cumulative validation changes and tests against canonical plain/required-object shapes; no separate persistence representation was introduced.
- FIND-5: hosted-code malformed/exact/over/depth cases now exist in the checked-in fixture and all three SDK tables, including published readback. Agent exact-byte proof remains DATA-1 above.
- FIND-6 rustdoc: outside this domain's acceptance verdict; no storage consequence.

## Verification limits

Static review and one read-only byte-count self-check only; no Cargo, Postgres, SDK runtime, crash, rolling-replacement or retention-expiry tests were rerun. The first byte-check command used unavailable `python`; the same check succeeded via `mise exec -- python3`. Recorded journey evidence was read as a claim and cross-checked against actual assertions. Broader provider-schema compatibility, repository-wide standards and deployment availability belong to the other independent reviewers. This audit makes no claim that existing visibility semantics guarantee immediate durable publication of direct results.
