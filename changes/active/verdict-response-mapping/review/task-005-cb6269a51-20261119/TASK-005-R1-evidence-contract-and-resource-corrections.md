---
id: TASK-005-R1
kind: remediation
status: proposed
spec: SPEC-verdict-response-mapping
spec_revision: 7
requirements: [REQ-015, REQ-018, REQ-019, REQ-020]
depends_on: []
parent_task: TASK-005
remediates: [FIND-TASK-005-1, FIND-TASK-005-2, FIND-TASK-005-3, FIND-TASK-005-4, FIND-TASK-005-5]
---

# Preserve the agreed citation boundary and admitted evidence lifecycle

Correct the five validated gaps so an author can use the agreed native Choice example, retain a raw Verdict schema unchanged, and let a workspace judge search original evidence with honest cuts and memory accounting. Keep named-Verdict reference selection, canonical result recording, optional/required classification and the live OpenAI evidence judge.

Original task: `changes/active/verdict-response-mapping/tasks/TASK-005-optional-citations-and-provider-smoke.md`. Reviewed base: `89d9638d4986d8ed833bfc99cd4ce28007afdb8b`; candidate: `cb6269a5143cf95210e88412f71cd8972080ad3d`. Use its complete revision-7 packet and governing architecture, not the main checkout's different packet snapshot. [findings-validation.md](findings-validation.md#validated-findings) is the canonical finding record and supplies exact locations and source evidence; [verdict.md](verdict.md) records the accepted task-specific verification overrides.

## Diagnosed gaps and correction boundaries

1. **FIND-TASK-005-2, REQ-020/raw preservation.** An accepted raw canonical JSON schema containing `$defs/Citation`, with workspace/fs.read or fs.search, enters reference mode because `AgentExecutor` tests only the check role, transform and tool/workspace availability. Its existing canonical response is rejected against a changed schema. Restrict specialization at that existing executor boundary to the authored named Verdict format. Reuse current binding/schema/Agent validation; preserve raw canonical schemas and responses, declared data, per-invocation isolation and named-format references. No schema-normalization framework is needed.

2. **FIND-TASK-005-3, REQ-019.** At the supported 1024-byte result budget, 65 scratch files exceed the 64-entry walk ceiling. Evidence search discards the walk's cut and returns false when the match result does not fill its byte budget. Carry that existing signal through the Workspace evidence response. Preserve file mode, result-byte cuts, unavailable counts and issuance only for returned evidence. Do not add another traversal/index mechanism.

3. **FIND-TASK-005-4, REQ-019–020/resource authority.** Selected JSON and ordinary mounted inputs return owned unit collections after their local memory guard drops. Consumers keep those allocations while matching/rendering, and concurrent calls can reuse the released capacity. Keep materialized input units charged through their actual read/search lifetime, using the existing ByteLedger/admission/busy machinery and accounting for the materialized representation. Reuse the current workspace ownership pattern; preserve selected-value visibility, exact coordinates, registered extraction freshness and release on completion/interruption. No new quota, registry or pool.

4. **FIND-TASK-005-1, REQ-015.** The existing Decisions smoke uses ancillary Noul/Score questions, custom Choice labels and an outcomes map. Restore the approved canonical Choice question/transform in the same visible example and adjust the shared mock only where necessary for that answer selection. Preserve five Agents, one Verifier, one Service, one verification call, the real photograph, explicit mock/live selection and the newer Responses evidence-reading OpenAI judge. Do not redesign native provider behavior or add payload/credential matrices.

5. **FIND-TASK-005-5, REQ-018–020.** The existing Agent-tools reference describes raw-only reads/searches and claims nothing is parsed without extraction; the Verifier reference directs named workspace models to emit canonical slot citations. Correct these two existing reference sections against actual descriptors and the fixed named/raw gate: file/evidence argument and availability, returned attempt-local references versus canonical final citations, extraction provenance/freshness and existing unavailable-view refusal. This is a bounded factual correction, not a new documentation system.

## Observable closure scenarios

Add focused tests at the existing owners and record exact new names/selectors before execution; do not manufacture test names or historical evidence here. Keep Cargo-backed invocations sequential. No broad journey aggregate is required for this remediation.

1. **Raw and named output contracts — FIND-TASK-005-2.** Through the existing Vala Agent executor/provider mock seam, author a raw canonical Verdict schema with `$defs/Citation`, mounted input and fs.read. Inspect its outgoing request, return canonical slot/locator output and assert the successful canonical result. Contrast the existing named format: tools return a reference, the outgoing citation item remains reference-only, and the final result resolves canonically. Include declared data where the existing fixture supports it. Owner: `vala-verify` Agent integration tests; runner: `mise exec -- cargo nextest run --locked -p vala-verify --test integration -E '<exact new test filter>'`. Existing named-reference connected Python assertions can be reused; do not substitute a schema-helper-only assertion for this executor path.

2. **Traversal cut survives evidence search — FIND-TASK-005-3.** At the existing hosted Workspace owner, create more valid work entries than its traversal limit under a supported small output budget; search evidence mode with no or few matches. Assert the response says truncated even without a full output. Keep assertions for ordinary uncropped results, output-byte cuts, unsupported-view accounting and returned-reference correctness. Runner: `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E '<exact new test filter>'`. A workspace unit test exercises this in-process branch; no new real-server harness is necessary.

3. **Input-unit admission persists until release — FIND-TASK-005-4.** In the same owner tests, retain materialized ordinary and selected-JSON input evidence and verify its memory charge remains held until the consumer releases it. A second allocation that cannot fit must use existing busy behavior; release restores capacity. Check the materialized representation, rather than only the initial byte read or reference registry, and preserve selected-coordinate assertions. Use the same focused server-library runner. Reuse existing settlement/interruption coverage unless the correction changes that lifecycle; if it does, add the affected release assertion at its existing owner. No synthetic host load or timing-based memory test.

4. **Agreed five-provider artifact — FIND-TASK-005-1.** Inspect the visible Decisions declaration for canonical Choice labels and a plain ChoiceTransform with no outcomes map. Run `mise run test:verification:smoke`, then the original task's explicitly authorized live verification `mise run test:gateway:smoke:live:pass -- -k test_five_providers_verify_one_service` in its credential-owning environment. Both must produce five passing named requirements and the final passed verdict, with the real photo and the OpenAI policy line-3 citation preserved. Missing live access leaves live closure incomplete; no mock fallback, skips or retry-until-pass. Reuse the task's model/credential wrapper without printing secrets. If shared-mock branches change, rerun only affected consumers identified by those branches.

5. **Truthful reference instructions — FIND-TASK-005-5.** Content-review the two corrected reference sections against file/evidence tool descriptors, named/raw outgoing schemas and canonical result reads. Run the existing `mise run docs:check`. No runtime test is needed for prose.

Run format/lints appropriate to the changed owners (`mise run fmt`, `mise run lints`, and Python checks if that example/mock changes). Run `mise run codegen:check` if schema contracts or generated consumers change; regenerate through their existing owners rather than editing generated artifacts. Record the focused commands/results and any changed waits, allocation or lifecycle behavior. Full integrated SDK gates remain change-review work, not this remediation's default runner.

## Preserved scope and handoff

No migration, dependency, new locator/input format, semantic evidence guarantee, resolver, reference registry, runtime, retry loop, synchronous persistence or new storage wait is authorized. Preserve all existing security/tenancy/capture controls, provider refusal sanitization, historical results, whole-source handling and requiredness behavior. The benchmark and held-out retrieval experiment remain waived for this task. TDD history is nonblocking and requires no retroactive repair. Duplicate-upload SQL behavior and UI bootstrap work remain excluded.

Return the corrected immutable candidate and focused closure evidence to the maintainer for remediation review. Do not automatically launch review, commit, push, merge or deploy.
