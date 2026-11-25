# TASK-005 verdict: FIX_REQUIRED

Reviewed base `89d9638d4986d8ed833bfc99cd4ce28007afdb8b` through candidate `cb6269a5143cf95210e88412f71cd8972080ad3d` against approved SPEC-verdict-response-mapping revision 7, REQ-015–020, original TASK-005 and the TASK-004 example it carries forward. Source remained clean and immutable in `/tmp/wyrd-task005-review-cb6269a51`; the newer main checkout and unrelated dirty work were excluded. This is the first TASK-005 review, with no prior findings to close or convergence assessment needed.

Behavioral acceptance is **FIX_REQUIRED**. Five validated findings remain. Their full scenarios, authority, source locations, evidence and correction boundaries are recorded once in [findings-validation.md](findings-validation.md#validated-findings):

- **FIND-TASK-005-1:** the delivered Decisions example uses custom labels and an outcomes map rather than REQ-015's approved canonical Choice shape.
- **FIND-TASK-005-2:** reference specialization also rewrites supported raw Verdict schemas, outside REQ-020's named-format boundary.
- **FIND-TASK-005-3:** evidence search drops the directory traversal cut and can report an incomplete search as complete.
- **FIND-TASK-005-4:** materialized input evidence units outlive their memory charge during reads/searches.
- **FIND-TASK-005-5:** maintained tool/Verifier references prescribe the obsolete workspace citation protocol.

The single reconciled requirement/scenario matrix is [the validation matrix](findings-validation.md#reconciled-acceptance-matrix), incorporated into this verdict. REQ-016–017 pass on inspected implementation/assertions and available execution records. REQ-015 and REQ-018–020 fail only for the referenced validated findings. The remediation handoff is [TASK-005-R1-evidence-contract-and-resource-corrections.md](TASK-005-R1-evidence-contract-and-resource-corrections.md). Review does not initiate its implementation.

## Scope, authority and independent coverage

[subject.md](subject.md) records immutable identities, navigation and current maintainer steering. Governing sources were the candidate's approved spec/task/design, applicable root and SDK/docs AGENTS, `changes/spec-driven-development.md`, `TESTING.md`, maintainer/rust/error/agent-surface guides, the UI skill for its presentation boundary, and owning Verifier, hosted-resource/tool, recording, and security architecture sections. The main checkout's CodeGraph was used only for preliminary navigation; its current sources differ. The detached candidate has no index and was inspected directly without indexing.

- `behavior_rev` independently reviewed requirements, scenarios, assertions, classification/requiredness, schema paths and preserved workflows: [behavior report](task-review-behavior.md).
- `invariant_rev` independently reviewed original coordinates, extraction invalidation, reference lifetime/isolation, cancellation, concurrency, resource admission and waits: [invariant report](task-review-invariants.md).
- `repo_rev` independently reviewed SDK/public contract projections, retained reads, MCP/UI/docs, the provider example and refusal confidentiality: [repository report](task-review-repo.md).
- Fresh `ponytail_rev` independently checked every proposed finding, coverage and proof: [validation](findings-validation.md). Duplicate raw-schema proposals were consolidated. Its source check corrected the repo report's inaccurate claim that Decisions already uses the canonical example. No new validator-only finding requires a further discovery pass.

No separate maintainer, system or domain reviewer was necessary: maintainability remained with the primary; shared-state, failure/cleanup, resource and tenant boundaries were assigned to the invariant specialist; refusal confidentiality and result access were assigned to the public-contract specialist. No migration/new durable storage boundary was introduced. All selected reports were obtained; no independent review boundary remains missing.

## Verification and accepted limits

The review inspected actual test assertions and candidate-owned implementation records, together with the maintainer's reported execution results. It did not rerun Cargo/journey suites, code generation or paid calls. The invariant reviewer ran an assert-based self-check using the exact candidate traversal helper: 65 valid files at a supported 1024-byte output budget yielded 64 visited files and a cut. Source independently establishes that evidence mode loses that cut. The memory finding follows exact allocation/guard ownership; no observed OOM is claimed. `git diff --check 89d9638d4...cb6269a51` passed, and final candidate HEAD/status remained exact and clean.

Scenario 8 has real connected/owner assertions, covered by the final shared-mock/owner execution summaries, but no appended focused evidence entry. This limitation does not create another finding or a claim that a dedicated focused execution transcript exists. Existing SDK/MCP/UI results support the unchanged projections; this review does not claim a fresh candidate-wide green run. Broad integrated lanes belong to change review.

The current request's task-specific decisions dropping L1 measurement and replacing the held-out retrieval experiment with the live OpenAI evidence judge are accepted overrides. Neither creates remediation. One live policy-selection case proves that case, not general retrieval quality.

The [validated wait inventory](findings-validation.md#wait-reconciliation-and-verification-limits) remains applicable: existing batched pinned-evidence validation moved into classification; continuing after a known-invalid citation may reach later sources as REQ-017 requires. It retains existing admitted File/Artifact metadata and pinned-source reads. No new PostgreSQL, Scribe, object-storage or Vault wait category was found. Workspace reads/hash/extraction use the existing tracked blocking route; references issue/resolve synchronously; settlement remains unchanged and recording stays asynchronous. The waiver makes no measured performance claim.

TDD compliance: **VIOLATED**, historical and nonblocking. Preserve the task's recorded production-before-test cases separately; the maintainer explicitly does not require TDD remediation. No retroactive RED or production churn is requested.

Duplicate-upload SQL behavior, UI bootstrap checks and unrelated checkout work remain outside this correction boundary. No implementation, commit, push, merge or deployment occurred.
