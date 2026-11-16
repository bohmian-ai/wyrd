# TASK-002 behavior acceptance discovery

Overall result: **FAIL** (one proposed material documentation finding).

## Immutable subject and coverage

Base: `e5af2ce5caef2d00303e1c980b12c8a9bca4af0e`.
Candidate: `ff78b51b61fea8f215a2139b17840044f65b17e4`.
Reviewed the complete `git diff base...candidate`, the original task at base,
and approved specification revision 2 at candidate. The working specification
revision is excluded. No prior TASK-002 evidence is carried forward. Candidate
source was not edited; no builds or tests were run by this reviewer.

Read the recorded scope and specialist selections, root and affected nested
AGENTS rules, spec-driven-development, maintainer-style, Rust, errors and
agent-surfaces guidance, applicable testing/fixture guidance, docs contribution
and product rules, Verifier/Agents-tools-workspaces architecture, and rollout
compatibility authority. Existing accepted-authority and recording owners were
traced where the public tests depend on them; the security and invariant
specialists separately cover their assigned boundaries. No CodeGraph index
exists. No other reviewer report or implementation completion response was used.

Verification evidence below is supplied evidence from the candidate task's
Implementation Evidence section, assessed against actual assertions and source;
it is not an independent execution claim.

## Navigation and reuse assessment

The Python test uses the existing policy-desk bundle/provider, public
`WyrdState`/`WyrdClient`/observe execution, `Execution`, and public Bifrost
readback. Its generic requirement helper now preserves the custom data type;
the existing retained helper returns the whole outcome and all old callers
were adjusted. The TypeScript test extends the existing serverTest/provider
and uses the same public client owners, with typed result parsing through the
existing `typedResult` spread. MCP uses the existing real rmcp connectivity,
execute/enqueue/settle/structured/rows helpers, real runtime-enabled
WyrdTestServer, and public registration. Its added helpers prepare scripted
provider replies and check one journey's output; they do not introduce a
runtime, identity, or persistence owner.

Both new Python program files extract the existing Rust journey's literal
programs into one shared fixture owner. Rust, Python, TypeScript and MCP load
those files; docs embed the tested investigation file. The existing
`policy-desk.yaml`, `evidence-reduction.yaml`, and tool-review declarations
remain the common story. No new dependency, abstraction around production
dispatch, schema copy, client engine or transport appears in the diff.

## Obligation matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Same declared nested program on Python, TS and MCP; REQ-001–004, AC-001–002/016 | Shared program loads in each existing story; existing evidence-reduction and tool-review declarations | All three focused journey commands reported passed; records prove actual worker-hosted nested calls occurred | PASS |
| Preparation and whole-Verifier check use the bridge | `assert_reduced`/`assertReduced` inspect `over-limit` step and `within-limit` requirement | Each asserts outer model record plus fs.list, twenty fs.read, fs.search code records | PASS |
| Provider catalog and direct workflow remain available | No declaration/catalog/runtime change; existing tool-review direct provider branches retained | Existing Rust hosted_tools regression reported passed; new journeys exercise nested targets from existing declared catalogs | PASS |
| Only bounded selected outer output crosses the model boundary; REQ-014 | Programs end in selected print output; provider request inspection rejects note text (`kept.`) and nested Card payload (`document-formats`) | Python/TS/MCP journeys report passed and inspect actual provider requests | PASS |
| Caller authority remains server-owned; REQ-006–007, INV-004 | Scoped keys/roles reach ordinary public execution and common Dispatch/hosted-tool path, no client grant logic | Real missing `bifrost_query:read` then authorized execution in all three journeys; exhaustive authority matrix remains dependency-owned | PASS |
| Denial code, no successful protected value, repair; REQ-008, AC-004–005/017 | Shared nested-investigation catches RuntimeError JSON code; denied query followed by allowed Card read | Every surface checks exact denial code substring, absent digest, zero bytes, expected children and printed recovery; authorized rerun succeeds | PASS |
| Honest flat records and ordered recipients; REQ-015–016, AC-012 | Journey helpers check exact sequence; existing common Dispatch records before execution and chooses Model/Code at respective entry points | Real journey digest/byte assertions plus exact recipient/tool order | PASS |
| Agent check records and custom Verdict data; REQ-017 | Python generic JSON mapping and decode_data mutates only data; TS typedResult spreads all remaining fields | Immediate typed/untyped checks, typed queued wait, status and retained outcome comparisons; all supplied focused checks passed | PASS |
| Immediate, queued and retained normal public APIs preserve step/check records | Python verify/verify_typed/submit/wait/wait_typed/status/retained; TS equivalents; MCP execute/enqueue/get_execution/Bifrost | Each focused journey reads both step and requirement arrays back through durable normal result APIs | PASS |
| Historical absent fields remain accepted; AC-013 | Unchanged Rust serde defaults remain authoritative; stubs add no required requirement field; generated TS has optional calls | Dependency-owned historical JSON matrix; compile-only fixtures and served OpenAPI optionality checks; no historical rewrite in diff | PASS |
| No raw tool result retained in record; existing sensitive-capture controls retained | No production capture or result-access change; record schema has only bounded args, recipient, digest, bytes and safe error | Served schema exact property-set assertion; runtime mapping preserves existing record contract | PASS |
| Citation integrity; REQ-018, AC-001/011/016 | Shared declaration pins notes; test Verdict cites notes JSON pointer `/n07`; result asserts pinned evidence slot and same citation | Every focused journey checks supported original citation and custom hits; no scratch/live read is promoted to input evidence | PASS |
| Stub and generated/public contract parity; AC-013 | Python authored ToolRecipient/recipient and optional check calls; existing generated TS wire declarations; no unnecessary binding registration | Python/TS type fixtures, py:lints/typecheck, ts:typecheck/napi/pack and codegen:check reported passed | PASS |
| HTTP/OpenAPI and MCP result shape agrees with Rust | Served OpenAPI test checks array refs, exact record fields, historical optionality and recipient enum; MCP checks advertised execute schema and real structured output | Focused served OpenAPI and real MCP journey reported passed | PASS |
| Budget code projects from existing catalog | No duplicate catalog; generated TS error code remains Rust-owned | codegen:check and SDK checks supplied | PASS |
| Public docs correctly describe outer output and intermediate visibility | Agent-tools reference says only printed text reaches model, but code.run also returns last-expression repr | Source `hosted/tools.rs::code` returns both `value` and `output`; see BEH-002-1 | FAIL |
| Docs describe limits, failure classes, worker occupancy and authorization costs | Runtime limits include argument/call settings and suspended waits; agent-tools has codes/recipients; runtime configuration explains worker retention and per-child checks/audit | Static comparison with Dispatch, ToolBridge, hosted code path and architecture; docs:check supplied | PASS |
| Upgrade/compatibility guidance, no universal savings guarantee | Configure-runtime requires upgraded verifier processes; existing deployment manifest authority governs overlap; reference explicitly disclaims guaranteed tokens/time savings | Static comparison with approved compatibility section and rollout authority | PASS |
| Runtime ownership and non-goals; INV-001/006 | Diff contains tests, authored stubs, shared fixture programs, docs and evidence only; no endpoint/engine/UI/tool/Agent field/network/storage change | Complete diff inspection; format/lints/codegen/docs supplied | PASS |
| Preserve existing regressions and fixture ownership | Rust literal program bodies extracted without changing operations; Python retained helper callers adjusted; existing provider branches maintained | Rust reduction/hosted grants and Python hosted/custom-verdict regressions supplied; other SDK full suites deferred to final change review as task requires | PASS |

## Proposed finding

### BEH-002-1 — INCORRECT: printed output is not the only model-visible code result

- **Obligation:** TASK-002 requires public guidance that correctly explains
  intermediate visibility and output recipients; specification REQ-001 keeps
  existing code.run response semantics and REQ-014 explicitly permits the
  bounded outer value/printed output. Docs contribution rules require a
  published reference to match its runtime catalog.
- **Changed location:**
  `docs/src/content/docs/reference/agent-tools-and-authorization.md:34` says
  “The model sees only what the program prints.” The same claim is copied
  into generated `docs/public/llms-full.txt`; the investigation fixture's
  general explanatory comment also says only printed output reaches the model.
- **Source proof / reachable path:**
  `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs:361–374`
  receives `(value, output)` from the sandbox, computes a bounded repr of
  `value`, and returns `{value, value_truncated, output}`. Its CodeRun
  descriptor at line 609 explicitly promises printed output **and the repr
  of the last expression**. `RecordedTool::invoke` in
  `crates/vala/vala-verify/src/blocks/tools.rs:922–928` delivers that entire
  value to the model. A supported declared program ending in
  `await call_tool('cards.get', {...})` or a final expression referring to an
  intermediate result returns that result's repr without calling print.
- **Observable consequence and materiality:** a reader following the new
  reference can deliberately omit print while leaving a large intermediate
  value as the final expression, expecting it to stay in Python. The bounded
  value still enters model context, defeating the stated reduction/visibility
  contract. This is a false instruction about the central public workflow,
  rather than a wording preference or request for stronger isolation.
- **Sibling owners/correction boundary:** both direct and bridge-backed
  code.run share HostedTools::code, and all SDKs/MCP receive that existing
  server output. Runtime semantics and wire fields already match the approved
  specification. Do not change them. Correct the source reference to state
  that bounded printed output and the last-expression value reach the model;
  intermediate values stay private unless included in either. Reconcile the
  shared example explanation where it asserts the general rule and regenerate
  the existing machine-readable docs.
- **Correction burden and focused proof:** a bounded documentation/comment
  correction using existing owners suffices, with no dependency, wrapper or
  runtime change. Compare the wording with the existing CodeRun descriptor
  and HostedTools::code, and run docs:check. Existing journey programs ending
  in print remain valid. No new broad server matrix or per-language test
  duplication is needed.

## Verification limits and disposition

No independent test execution was necessary to establish this documentation
contradiction; the runtime producer and model delivery consumer are explicit.
Supplied green journeys are credible proof of the required successful,
permission-denied, typed and retained flows, but all chosen programs end in
print and therefore cannot establish the stronger published print-only claim.
The full fault, concurrency, historical JSON and byte-reduction matrices remain
TASK-001/shared-Rust proof, as required; full public suite execution belongs
to final change review. No other material behavior or ownership finding is
proposed. Independent source/materiality validation must decide BEH-002-1.
