# TASK-005 public-surface and gateway boundary review

Subject: base `89d9638d4986d8ed833bfc99cd4ce28007afdb8b` → candidate `cb6269a5143cf95210e88412f71cd8972080ad3d`, SPEC-verdict-response-mapping revision 7. Source and direct cumulative diff were inspected in `/tmp/wyrd-task005-review-cb6269a51`; HEAD remained the candidate and its working tree was clean. No production files were modified.

## Scope and authority

Reviewed REQ-015–020's SDK authoring/result projections, retained execution reads, MCP, UI, documentation, five-provider smoke and provider-refusal propagation/confidentiality. Read root and Python/TypeScript/docs AGENTS, task-review skill, relevant workflow/testing/style/rust/agent-surface guides, UI skill, and owning Verifier/recording/security architecture. Workspace state/resource implementation is assigned separately; overall engine coverage belongs to the primary reviewer.

The maintainer's current directions override older packet completion text: no L1 benchmark is required for this task; the live OpenAI evidence-reading case replaces the separate retrieval experiment. TDD history is preserved separately and creates no remediation here.

## Boundary coverage

| Boundary | Source and assertion evidence | Assessment |
|---|---|---|
| Authoring boolean | `CheckAgent` contract and validation; client `fields!(Check)`; Python `definition_class!`, CheckFields and setter stub; TS native field and wrapper setter. Python required/optional cases in `test_required_citation_needs_one_accepted_reference`; Python and TS native-transform refusal unit tests. | Shared owner and projections agree. Python has connected requiredness proof. Rust/TS do not add a connected boolean declaration/read-back case; source shows their existing generic authoring paths carry it. This review does not claim such a case ran. |
| Canonical result contract | `execution.rs::Citation`, `RejectedCitation`, `RequirementResult`, typed-data conversion; TS generated card types; Python authored TypedDict stubs. Historical-result test checks exact round trip without rejection metadata. | Whole-source locator is optional/null; rejected list omitted when empty. No second envelope/storage mechanism. |
| Result consumers | Rust `invalid_judge_verdicts_are_inconclusive` checks both immediate result and details; Python invalid/required citation cases; TS rejected-citation and code cases; MCP `custom_verdict_data_round_trips` checks immediate and settled whole/precise/rejected references with existing retained-data/permission/foreign-tenant coverage. | Assertions exercise actual outputs. Recorded task results report those focused journeys passed. No execution was rerun by this specialist. |
| UI | `VerificationExecution.svelte` labels null/absent locator “whole source”, separates accepted and rejected lists, renders reasons through escaped Svelte interpolation. `ui-library.test.ts` asserts both accessible lists against expanded fixtures. | Appropriate presentation-only change; task records ui:check, 81 passed. |
| Provider refusal | `caller.rs::provider_refusal/provider_message` retains only `/error/message`, caps UTF-8 bytes at 512 on a character boundary, and keeps existing error categorization. Gateway `adapter/mod.rs::refusal` removes exact and JSON-decoded credentials, and refuses encoded-secret bodies before caller receives them. Existing gateway encoded/exact-secret tests plus updated Python/Rust/TS Google refusal assertions cover this seam. | No new credential exposure found; bounded diagnostic text follows existing refusal error path. No new storage wait. |
| Five-provider example | One visible test with five Agents, one Verifier, one Service, one verify call, real JPEG, canonical Choice transforms for Decisions/Jev. OpenAI Responses judge searches policy and final assertion pins canonical line 3. Task records mock and explicit live passes plus model IDs and shared-mock consumers. | Consistent with maintainer's newer evidence-reading judge direction, which supersedes original “four ready checks” shape. One live case does not establish retrieval quality generally. |
| Raw/custom schema preservation | `AgentExecutor::invoke` by_reference gate and schema editing versus `validate.rs`'s accepted handwritten schemas. | Proposed defect REPO-01 below. |
| Maintained tool instructions | Existing reference docs against new descriptor/schema behavior and named-Verdict reference selection. | Proposed documentation correction REPO-02 below. |

No Cargo builds, tests, paid provider calls, or generation were run by this specialist, as instructed to coordinate builds. Candidate evidence records contain focused unit/journey/UI/type/lint results; no fresh codegen/napi execution is claimed here. Changed production code in this boundary adds no PostgreSQL, Scribe, object-storage or Vault wait: provider-message extraction, schema/type projection and UI formatting are synchronous; result recording remains the existing asynchronous owner.

## Proposed findings for independent validation

### REPO-01 — REGRESSION: reference specialization changes supported handwritten Verdict schemas

- **Authority:** REQ-020 restricts specialization to direct **named-Verdict** checks; task implementation boundary explicitly preserves raw behavior. REQ-014 preserves existing direct-Verdict workflows except the approved changes.
- **Location:** `crates/vala/vala-verify/src/blocks/agent.rs:472` (selection), `:381` (schema edit), `crates/wyrd-spec/src/verification/execution.rs:1669` (replacement of `$defs/Citation`).
- **Starting state/trigger:** An existing direct Agent check declares `fs.read` and a workspace, and authors a raw canonical Verdict JSON schema containing `$defs/Citation`, rather than the named format. This is supported: `validate.rs:898–905` accepts handwritten compatible schemas; `verifier_spec_requires_check_agents_to_produce_a_verdict` explicitly includes the generated canonical schema as a raw schema in its accepted cases.
- **Expected:** Preserve the raw canonical citation schema/response path. Only the named form changes to `{reference}`.
- **Actual:** `by_reference` checks only no-transform Verdict role, workspace and fs.read/search. It never checks named format. `bind` replaces the raw schema's Citation definition with `{reference}`. A previously valid `{slot, locator}` answer is then invalid against the actual model-facing schema; a provider following the authored canonical response contract reaches structured-output correction/error instead of its prior judgment. An inline raw schema without that particular definition happens to avoid replacement, making behavior depend on JSON-schema layout.
- **Consequence:** A supported existing raw-format workspace judge changes its output protocol outside the approved boundary.
- **Smallest correction/proof:** At existing `AgentExecutor` specialization selection, require the authored named Verdict format. Preserve raw schemas and canonical responses; add one regression exercising raw canonical schema with `$defs/Citation`, workspace and fs.read alongside named reference-mode proof. No new resolver or schema normalization is required.
- **Evidence strength:** Source-proven boundary mismatch with existing supported-schema acceptance test; not dynamically reproduced in this review.

### REPO-02 — INCORRECT: shipped reference instructions prescribe the obsolete workspace citation protocol

- **Authority:** REQ-019–020's evidence view/reference contract; REQ-018's truthful execution/inspection contract; docs AGENTS requires reference fields/options and shipped facts. This is one bounded documentation correction, not a style finding.
- **Locations:** `docs/src/content/docs/reference/agent-tools-and-authorization.md:20–23,50`; `docs/src/content/docs/reference/verifier-card.md:180`.
- **Starting state/trigger:** An author follows the maintained Agent-tools/Verifier references to build a named-Verdict workspace judge with fs.search/read.
- **Expected:** Instructions distinguish default file lines from `view: evidence`, show that evidence calls return references, and tell this judge to return `{reference}` while final results remain canonical. Extraction provenance/invalidation and unavailable evidence views should be stated at the existing reference owner.
- **Actual:** The Agent-tools reference describes only raw lines and states unconditionally that “Nothing is parsed unless the Agent calls fs.extract”; it omits the new view argument and reference result. The edited Verifier reference still says a model using response_format: verdict cites bare slots, without the workspace reference exception. Actual descriptors (`hosted/tools.rs:759–803`) expose evidence units/references and the outgoing schema rejects slot/locator objects in named workspace reference mode.
- **Consequence:** Following the shipped instructions produces the wrong model-response contract for the new supported journey and hides the evidence-view operation the task delivers. These are false operational instructions, not missing explanatory polish.
- **Smallest correction/proof:** Correct only these existing reference sections to distinguish file/evidence modes, returned attempt-local references versus canonical final citations, extraction availability/freshness and the existing evidence_view refusal. Content-review against actual descriptors and named/raw gates, then the existing focused docs checks; no new guide/framework/example suite is required.

## Limits and process history

No finding asserts codegen drift, a Rust/TS boolean runtime bug, or absence of all SDK journeys. Structural parity and existing connected consumers were inspected; the exact new boolean is dynamically exercised through Python and TS authoring validation. TDD violations recorded in the task remain historical disclosures, with no requested churn. Duplicate-upload SQL behavior and UI bootstrap tests are outside this boundary. Final behavioral acceptance belongs to the primary report and independent validator, not this specialist.
