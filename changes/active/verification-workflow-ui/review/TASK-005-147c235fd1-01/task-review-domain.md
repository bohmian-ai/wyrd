# TASK-005 domain review

Subject: base `05fc05138a0a72d59a2fd9b96fae5e6e4ad6f285`, candidate `147c235fd19dd4a400da1515a908db0d5521477d`. Reviewed immutable source/diff; only this report written. Scope: retained-parent permissions, query membership/current/history/bounds, unavailable-definition inspection. This is discovery evidence for independent validation, not the final verdict.

## Authority and covered flow

Read approved verification-workflow-ui revision 8 REQ-001/002/003/004/005/006/011/012/014/020 and AC-002/007/008, original TASK-005 scenarios 1–5, resumption design, authorization-model REQ-004/006/007, architecture/wyrd-security-posture.md “Current authority and freshness” and “Resource and retained-data visibility”, architecture/wyrd-design.md Repository, repository workflow/testing/maintainer rules and UI skill. The maintainer accepted the listed AC-009 differences; that ruling does not remove recorded-fact selection requirements. REQ-021 retained status after scheduling cleanup belongs to TASK-007 and is excluded.

- Query admission: `wyrd-server/src/query/service.rs:75` admits callers with a Bifrost read or managed execution read capability; it does not authorize rows itself. `vala-bifrost-redux/src/oracle/mod.rs:3276` applies `restrict_verification_rows` before binding/optimization. Its implementation at 3955 rewrites every managed verification TableScan, including nested subqueries, to visible trusted parent kind/name/UID before user expressions. Family and exact version grants differ correctly; columns of excluded parents are masked; unprovable parents are hidden even for tenant-wide readers. This includes input, checkpoint, result and output scans used by set queries, so joins cannot bypass visibility.
- Domain reads: `verification/service/inspection.rs:512` reads queue/accepted state within tenant context; scopes come from queue Cards or retained exact identity. At 587 it authorizes `executions_read(parent)`; inaccessible reads become the same execution-not-found as unknown. Invocation/publication reads call the same retained owner. Separate definition Card reads in UI `execution.ts:123` do not grant or withhold execution facts.
- PR set source: `observe.ts:1425–1431` uses retained pull-request origin + Repository name + PR number; sorts history by recorded observation, independently of the Verify range. Explicit generation has a separate identity query, including beyond the raw 200-record history bound; absent pins do not fall back. Current derives from first sorted generation, not row arrival. Server frozen membership is reused without timestamp/SHA inference.
- Member projection: `observe.ts:1472–1484` reads the same canonical execution population (`schemas.ts:248`) by frozen IDs and admission observation time (millisecond-expanded interval preserves source microseconds). Checkpoint-selected result drives verdict; latest retained lifecycle event drives status; neither set nor loader computes a combined verdict. Every frozen ID is retained in presentation, and a missing row makes `complete` false. History truncation is explicitly surfaced. Relevant backend test asserts every member of one generation shares the exact `recorded_at` (`pg_github_pull_requests.rs:2847–2853`).
- Retained source independence: PR route/set loader does not require a current Repository or GitHub configuration read. `loadExecution` preserves independently available parts when definition is denied/missing, and now reads invocations for completed no-definition work. Existing publication owner preserves exact producer/attempt selection and bounded previews/downloads.

## Assertions and available evidence

Inspected assertions rather than relying on test names:

- `ui-library.integration.test.ts:2197`: real public connection/declaration/admission with two Verifiers; Node and MCP set reads match accepted generation/policy/source/members and individual domain status/result reads, assert no combined verdict and exactly two Bifrost queries. Built artifact calls the real MCP server; custom host uses the same loader without returning credentials.
- `:2322`: rerequest at unchanged head, source push and subsequent rerequest while pin is open; newest changes, exact prior members do not, absent generation stays absent under unrelated Verify range.
- `:2462`: real family and minimal custom Card-read readers see only their Repository, sharing a Verifier with a hidden Repository; facets exclude hidden PRs. Exact-version reader sees only its admitted policy generation; table-only permission exposes no managed rows; revoke/regrant changes subsequent reads. Both Node/MCP are exercised for family/custom readers; exact/table-only readers use Node (shared Oracle path).
- `:2390`: deletes Repository policy version and its Verifier, then loads frozen members and exact execution/progress with a truthful definition-not-found. This test covers administrator retained visibility; scoped retained Card-family behavior is independently exercised by the authorization tests/Oracle source.
- `:2431`: subject-only reader previews retained step report and uses production BFF download despite definition-not-found.
- `ui-pages.integration.test.ts:2830` onwards: Alice scoped reads agree through SQL/CLI/Python/MCP; production browser opens only payments sets, compares hidden vs unknown PR/execution, selects recorded terminal requirement with reload/Back, and grant changes affect session reads.
- `ui-library.test.ts:3775/3812`: pin beyond bound, missing pin/no dependent member read, partial frozen membership, refused member status without verdict, history bound, explicit denied/empty outcomes. `:4006` proves fallback only for a terminal result with one instance per requirement, leaving the two concrete cases below uncovered.
- Existing backend `pg_github_pull_requests::retained_reads_resolve_current_and_history_membership` covers interrupted handoff/recovery, frozen membership observation time, older records read later, bounded history and authorization. Its retained sort proof is reusable for the consumer query ordering; no new server behavior is requested.

No commands/tests were executed by this reviewer; root coordinates runtime verification. Root subsequently reported current-candidate focused U1 PASS (5 selected: `TASK-005|a reader of the subject but not the Verifier`, 17 skipped), `ui:check` PASS (113), and Python typecheck PASS in `verification.md`. These executions strengthen the traced cases but do not include the two unsupported fallback variants below. Root also reported a `codegen:check` failure concerning the StatusMark brand contract; the primary reviewer owns that separate boundary. TDD remains VIOLATED independently of behavioral acceptance.

## Proposed findings for independent validation

### DOMAIN-1 — repeated requirement IDs break no-definition fallback (INCORRECT)

**Requirement:** REQ-004/005 and AC-002/008: unavailable/deleted definition must preserve readable recorded facts and selection; repeated/fanout work is supported.

**Location:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/components/VerificationExecution.svelte:302`.

**Starting state and trigger:** A Repository-scoped reader (no Verifier Card grant), or a reader after Verifier deletion, opens a completed execution whose step has a fanout or repeat-attached requirement. Its legitimate compact result contains several judgments with the same requirement `id` and distinct `item_index` or `iteration_index`.

**Expected:** The definition refusal remains explicit, the recorded requirement is selectable, and its distinct judgments remain inspectable.

**Actual/source proof:** The new fallback iterates every result requirement using `(each.id)` as a Svelte each key, without deduplication. IDs repeat for legitimate fanout/repeat outcomes: public `RequirementResult` (`wyrd-spec/src/verification/execution.rs:646–659`) explicitly carries instance coordinates. Production engine tests assert three `per` results with distinct item indices (`vala-verify/src/blocks/engine/tests.rs:3543–3553`) and three `seen` results at iterations 0/1/2 (`:3784–3793`). The existing inspector intentionally filters all judgments by a chosen requirement ID (`VerificationExecution.svelte:75`) and renders them by index, so the fallback key contradicts this supported shape. Duplicate keys cause Svelte keyed-each rendering failure/inconsistent updates; development mode reports `each_key_duplicate`.

**Consequence:** The new retained no-definition inspection path cannot reliably render normal repeated/fanout verification, precisely for Repository readers who have no reason to be granted definitions.

**Smallest correction boundary:** Within `VerificationExecution.svelte`, make recorded requirement navigation use one stable entry per requirement ID (or uniquely identify displayed instances consistently with selection); keep all recorded instance judgments in the existing inspector, without fabricating graph topology or adding a definition grant. Add a focused component test using a denied definition and at least two same-ID judgments from separate items/iterations, then select the requirement and assert all judgments remain visible. No server/SDK change needed.

**Proof limit:** Source-backed proposal; no runtime reproduction executed here. Fresh validator should check the supported result shape and render path independently.

### DOMAIN-2 — preterminal recorded requirements disappear without a definition (MISSING)

**Requirement:** REQ-004/005, AC-002/008, TASK-005 scenario 4: keep independently readable recorded facts selectable without the definition, including bounded invocations. REQ-005 explicitly distinguishes recorded requirement verdict/reason from execution status; unfinished execution is not an excuse to discard available progress.

**Locations:** `VerificationExecution.svelte:71`, `:75`, `:298–304`, and `:446–448` (empty judgment message).

**Starting state and trigger:** A queued execution has settled a requirement on one branch or repeat iteration while another branch/iteration continues or waits. The reader has the trusted Repository/subject grant but no Verifier definition grant. The existing invocation read returns `state: requirement`, its outcome, exact producer and reason; the execution has not produced its final compact result.

**Expected:** The available recorded requirement is selectable and its available outcome/reason/producer can be inspected, while execution status remains running/waiting and no final overall verdict is fabricated. Definition stays refused and topology absent.

**Actual/source proof:** `loadExecution` already reads these invocations without a definition (`execution.ts:206–212`). The server projects settled check records to `InvocationProgress::Requirement { outcome }` plus reason (`verification/service/inspection.rs:205–225`) from accepted checkpoint state, irrespective of terminal completion (`:778–815`). Production engine checkpoints each requirement before returning its outcome (`vala-verify/src/blocks/engine.rs:1809–1850`); repeat progression explicitly occurs after earlier iteration checks (`engine/tests.rs:3767–3783`), making a settled check with unfinished work a supported normal state. Nevertheless `ran` drops every `state === requirement` item, fallback requirement buttons use only `status.result?.outcome.requirements`, and `judgments` does the same. Without a final result no requirement button appears. Even an exact `node=req:<id>` URL displays “Not judged yet.” despite the available judged invocation. A started check may earlier appear mislabeled as a Step; settlement then removes it from navigation.

**Consequence:** A restricted reader inspecting unfinished work loses already recorded check outcomes and reasons until the whole workflow finishes, and may see a false “Not judged yet” statement. The new fallback promises recorded selection but tests only terminal results.

**Smallest correction boundary:** Reuse the existing bounded invocation records in `VerificationExecution.svelte` to offer recorded requirement selection and show the progress fields actually available when no selected result exists. Preserve final-result authority once present, no combined/final verdict invention, no authored-node reconstruction, no new API/full-result synthesis. A focused component test should return a requirement invocation with outcome/reason and no final result under a refused definition, select it, and verify progress remains readable across refresh while status stays unfinished. A focused existing real-server no-definition journey should exercise that supported checkpoint state where practical; no broad suite/new product obligation.

**Proof limit:** Source-backed proposal, not a claim that a new test was run. Fresh validator must confirm this falls within the existing assigned fallback promise; no need to expose unrecorded check details or actual/citations absent from the invocation wire shape.

## Boundary conclusion

No cross-Repository/version leakage, new authorization evaluator, required provider lookup, membership inference, selected-result mismatch or false complete-set behavior found in the traced set loader. Two fallback implementation proposals above remain for independent validation. No production change, benchmark, broad journey aggregate or historical TDD repair was attempted.
