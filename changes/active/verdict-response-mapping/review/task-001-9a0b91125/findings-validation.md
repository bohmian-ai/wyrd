# TASK-001 independent findings validation

## Subject and independence

Base: `08998b9e9b6f04825f7743b877391247e0ad9621`.
Candidate: `9a0b9112523252328813041c0b11be375f0eed29`.
Authority: SPEC-verdict-response-mapping revision 1 and original TASK-001 at base;
implementation evidence at candidate. Source inspected in the clean detached
checkout `/tmp/wyrd-task-review-9a0b91125`, whose HEAD and status were checked.
The shared checkout's unrelated edits were not reviewed. No implementation was
changed. CodeGraph had established that the detached checkout is unindexed;
inspection therefore used direct source and the committed diff.

This fresh validator read all three selected reports: `task-review-behavior.md`,
`task-review-repo.md`, and `task-review-system.md`, then independently traced
the proposed failures and affected producers/consumers. Applicable root/SDK
AGENTS, spec-driven workflow, maintainer style, errors/agent-surfaces guides,
TESTING, and architecture Prompt/Verifier/in-memory authoring, recording and
security tenant/trust boundaries govern this reconciliation. No report is
missing. No newly discovered validator-only blocker is retained.

The primary covers requirements, ordinary behavior, repository rules and TDD;
repo-rev covers SDK/native contracts and authoring allocations; system-rev
covers execution/recording, added waits, native-answer validity, tenant reads
and performance qualification. These assignments cover the materially affected
boundaries. No additional reviewer is needed solely for another vote: existing
state/recovery/auth/storage owners are reused, while primary plus system review
cover their affected consumers. The final reconciled acceptance matrix belongs
in `verdict.md`; discovery matrices are evidence, not a second authority.

## Retained findings

### FIND-TASK-001-1 — INCORRECT — malformed Score metadata can pass

**Discovery / validation:** SYS-1 — CONFIRMED.

**Authority:** REQ-010's existing check-error path for nonfinite or native-contract
violations; REQ-012's native selected-answer capture. The supported Score answer
includes optional confidence/probabilities (`crates/skald/skald-spec/src/wire/jev.rs:123–131`).
Confidence is documented in `0..1`; probability values use the same native unit
interval already enforced for Choice.

**Location and trigger:** `crates/vala/vala-verify/src/blocks/transform.rs:110–114`
selects `JevAnswer::Score { score, .. }`; `grade` checks only the score. A valid
three-level declared Score policy `greater_than_or_equals: 2` receives native
JSON with `score: 2`, valid legend, and `confidence: 1.5` (or a present probability
outside `0..1`). This is an expressly supported metadata-bearing answer shape.

**Expected / actual:** the selected malformed answer must produce the existing
terminal check error, yielding an inconclusive requirement with an explicit
reason. Instead, normal f64 decoding accepts the value; the Jev provider's typed
send/decode path imposes no later semantic bounds, `grade` passes score 2, and
`to_value(answer)` retains the invalid metadata. Typed nonfinite metadata is
also ignored. Confidence must not affect a valid judgment, but invalid metadata
cannot become an accepted pass. Existing malformed journeys cover invalid score
itself and miss these fields.

**Smallest correction / closure:** validate present optional Score numeric
metadata at the existing selected-answer transformer, reusing `unit`, while
preserving absent fields and valid low-confidence outcomes. Add owner cases for
absent/valid/out-of-range/nonfinite metadata and representative hosted malformed
metadata through the existing Jev journey, asserting isolated check error,
explicit reason and unchanged invocation count. No transport or storage redesign.
This conclusion is source-derived, not a fresh compiled runtime reproduction.

### FIND-TASK-001-2 — INCORRECT — typed Python probability construction defers required validation

**Discovery / validation:** REPO-1 — CONFIRMED.

**Authority:** REQ-011 and TASK-001 scenario 4 require self-contained typed-value
constraints, including probability bounds, at construction; shared Rust owns
behavioral rules.

**Location and trigger:** `sdks/wyrd-sdk-python/python/wyrd/verify/_transforms.py:49–67`.
Construct public `ProbabilityTransform(question="q", expected=True, pass_at=.2,
fail_at=.8)`, use `pass_at=1.5`, or use NaN. Plain float fields lack any hook to
the existing shared `VerdictTransform::validate`.

**Expected / actual:** the typed constructor must refuse with field/correction;
all three objects are accepted. A later `Check.agent` attachment runs the shared
native validator, so this does not claim that the invalid policy reaches Jev.
It violates the promised earlier repair boundary. Existing unit negatives pass
dictionaries to Check.agent and therefore do not prove typed construction.

**Independent executable proof:** the validator loaded the exact candidate
`_transforms.py` through importlib using the installed Pydantic environment,
with `sys.modules[spec.name] = module` before `exec_module`. Invoked via
`mise exec -- /home/thorrester/Documents/GitHub/wyrd/sdks/wyrd-sdk-python/.venv/bin/python`;
all three constructions printed `ACCEPTED`. An initial harness run without
module registration failed Pydantic forward-reference setup; correcting that
harness produced the stated result. This is candidate pure-Python source proof,
not a candidate wheel/native rebuild.

**Smallest correction / closure:** route typed construction through the existing
Rust self-contained validator, retaining lazy optional Pydantic imports and
mapping-only use. Public tests construct reversed/out-of-range/nonfinite policies
and a corrected valid policy. Do not maintain independent Python domain rules.

### FIND-TASK-001-3 — VIOLATION — TypeScript policy refusal loses structured Wyrd diagnostics

**Discovery / validation:** REPO-2 — REVISED classification to VIOLATION;
substance CONFIRMED.

**Authority:** REQ-011 structured SDK diagnostics, TS AGENTS Errors and
`agent_docs/errors.md` stable-code/field/remediation projection.

**Location and trigger:** new `transform` helper at
`sdks/wyrd-sdk-ts/native/src/authoring.rs:427–432`, `invalid` at 34, consumed
by `wyrd/src/definitions.ts:245–247` and Check.agent's options application.
A public probability policy with `.2/.8` bounds enters shared Rust validation.

**Expected / actual:** the shared validation error should reach the caller as
WyrdError with its catalog code and structured diagnostics. The helper replaces
it with `napi::Error(Status::InvalidArg, error.to_string())`; the public setter
calls native `.set` directly, without the existing error-envelope projection.
A caller receives an unstructured N-API exception and cannot handle the same
catalog code used by Python and registration. Existing `.toThrow(field)` tests
verify text only and conceal the loss. This is new policy-validation behavior,
not a request to rewrite unrelated pre-existing authoring errors.

**Smallest correction / closure:** preserve/project the canonical shared error
through the existing native error mechanism at this boundary. Public tests
assert WyrdError identity, exact `WYRD_SPEC_400_VALIDATION`, and field/remediation
for invalid self-contained bounds, then successful correction. Preserve ordinary
object-literal inputs and no new error family. Source validated; N-API artifacts
were not rebuilt or executed.

### FIND-TASK-001-4 — INCORRECT — optional policy's advertised null form is rejected

**Discovery / validation:** primary null proposal, independently corroborated by
repo-rev — CONFIRMED.

**Authority:** REQ-005 optional check policy and REQ-013 single projected contract.

**Location and trigger:** `crates/wyrd-spec/src/card/verifier/transform.rs:31–45`
derives schema for `Option<VerdictTransform>`;
`sdks/wyrd-sdk-ts/wyrd/src/card-types.ts:851` advertises
`verdict_transform?: VerdictTransform | null`, inherited by public
AgentCheckOptions. Supply an otherwise valid direct-Verdict Agent check with
explicit `verdict_transform: null`, a supported advertised value.

**Expected / actual:** schema/type and runtime must agree on the absent-policy
representation. Custom AgentCheck Deserialize removes a present member then
calls `from_value::<VerdictTransform>` at lines 67–70, rejecting null. The TS
native helper also decodes it as a nonoptional policy; omission succeeds.
Thus a schema-valid/publicly typed direct-Verdict definition refuses. This does
not assert that None serialization emits null: its serializer omits None.

**Smallest correction / closure:** align the shared optional decoder and native
projection with the existing optional contract; pin absent/null direct-Verdict
parity at the owner and affected public consumer, preserving malformed non-null
refusal and policy-free behavior. Change the owning schema/generator if needed,
never edit generated types by hand. No new product semantics are required.
Source validated; no schema build or compiled runtime was run.

### FIND-TASK-001-5 — EVIDENCE_GAP — authoring allocation qualification is missing

**Discovery / validation:** REPO-3 — CONFIRMED as evidence gap, without asserting
an empirically demonstrated performance regression.

**Authority:** approved spec SDK and server allocation review; original task
lines 69–79 and packet verification ownership. Changed ownership/allocation paths
require representative matched before/after measurements with existing tools.

**Location / trigger:** `crates/skald/skald-prompt/src/builder.rs:621–636` builds
an owned request, serializes the complete request into a fresh Value to derive
JevRequestView, validates, then finalizes the original request. Python and TS
new typed-question conversions also change the authoring path. Task scenario
1/2 notes acknowledge conversions, but contain no matched workload/allocation
results; scenario 4 supplies none either.

**Expected / missing proof:** candidate-bound ownership/copy analysis plus
required same-host measurements for affected construction/attachment and graph
operations, representative Prompt/question sizes and shared reuse, observed
allocation bytes/volume and retained memory where measurable, timing and tool
limits. Functional green tests and conversion counts do not establish this.
Existing shared CardDefinition captures and typed runtime Arc are reused, but
that does not waive the explicit changed-authoring proof requirement.

**Smallest closure:** document reused owners/new copies; use an existing owner
for avoidable intermediate copies when available, and supply the approved
matched profiling evidence and limits. This finding alone does not demand a
production change without an established defect. Do not add a profiling framework,
new dependency or arbitrary numeric budget. Runtime benchmarks are separate.

### FIND-TASK-001-6 — EVIDENCE_GAP — mandatory clean runtime benchmark qualification is missing

**Discovery / validation:** SYS-E1 — CONFIRMED.

**Authority:** approved spec performance acceptance, packet verification guidance
and maintainer hot-path rule require full clean `mise run bench:l1:verification`
and `mise run bench:l1:gateway`, verdict VALID, compared with newest records for
the same experiment and host under `benchmarks/history/`.

**Location / trigger:** this candidate changes synchronous verification execution;
the task's final verification references only the requirement, with no run
identity, VALID result or reference comparison. No qualifying results are supplied
by the user summary; history is absent in the candidate checkout. The validator
also checked the committed benchmark diff, which supplies no records.

**Expected / missing proof:** both complete clean candidate-bound runs and
specified comparisons. Available functional proof cannot substitute; partial,
smoke or dirty runs do not qualify. No slowdown is alleged and this is not an
external-access BLOCKED verdict.

**Smallest closure:** run existing lanes on the clean final corrected candidate,
provide actual record identities/results and newest applicable comparisons. If
history lives elsewhere, make actual qualifying records available; do not invent
a baseline or waive comparison silently. No new benchmark framework.

### FIND-TASK-001-7 — EVIDENCE_GAP — applicable MCP projection proof was excluded incorrectly

**Discovery / validation:** B-001 — CONFIRMED, bounded to focused task proof.

**Authority:** REQ-005/012/013, approved acceptance observations, original task
scenario 6's applicable MCP execute/read proof and scenario 7 runtime MCP checks.
Packet deferral of full integrated lanes does not remove those focused task checks.

**Location / trigger:** task evidence at lines 370–375 says no MCP tool reads
execution details. Candidate server `src/mcp/verification.rs:210–222` registers
`verification.get_execution_details` with VerificationDetails; dispatch reuses
VerificationControl. The existing MCP journey owner already provides a details
helper at `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs:724–741`.
An agent executes a declared Jev-policy Verifier then reads its retained details
through this existing public MCP path.

**Expected / missing proof:** representative typed runtime catalog and connected
MCP execution/read assertions establish canonical verdict/reason, policy,
permitted selected native capture and existing provenance/denial behavior.
No changed Jev/policy assertion or execution evidence exists in this MCP owner;
its existing details test uses assert checks. SDK HTTP proof cannot establish
the separate required MCP consumer. This is a factual evidence omission, not
a demonstrated production MCP defect.

**Smallest closure:** extend the existing MCP verification journey with focused
Jev leaf execution/details and relevant descriptor/schema assertions, reusing
its setup/helper and existing negative/capture semantics. Correct the task's
false inapplicability claim and record the exact focused run. No new MCP tool,
route, result envelope or broad suite is required at task review.

## Reconciliation and limits

All seven retained findings are within settled approved behavior. No requirement
revision or product redesign is needed. The no-new-hot-path-waits inventory in
system review is source-supported: transform/response retention are synchronous;
two new registration SQL read sites are non-hot authoritative resolution inside
TenantConn. They are not disguised execution cache misses. Recording remains
existing asynchronous Scribe staging, tenant result reads retain existing
authority, and no new storage/migration/recovery mechanism is introduced.

Saved-bundle-specific reconstruction is not retained as a blocker: the approved
native graph read-back/redeclaration promise has credible equivalent declaration,
hydration and stored-value assertions; no separate fromPath mechanism is promised.
Exact Prompt/Agent provenance assertions are weaker than task wording, but source
preserves the unchanged owner and no separate newly discovered blocker is added.
Required projection proof closure under finding 7 should assert that existing
provenance alongside the policy/capture.

Behavioral proof and TDD history remain distinct. Scenarios 3/5/6 ran their hosted
proof after scenario 1 already introduced the behavior: later GREEN does not
establish test-first history. Record that deviation without requiring fabricated
RED or production churn. Compilation/setup/import harness failures do not prove
product RED; scenario 1 also records the real missing-structured-Verdict failure.
Scenario 4's disabled-validator run is mutation evidence, not historical RED.
Full unexpected-failure diagnostic-process compliance is not independently
established by the available notes; this creates no retroactive code correction.

Recorded task results and inspected assertions support the covered ordinary SDK
journeys, including the maintainer's 18 passing Python and 18 passing TS cases.
The validator did not rerun native SDK journeys, Cargo, codegen, full suites or
benchmarks; installed native modules were not treated as candidate artifacts.
The only fresh executable reproduction was the exact-source pure-Python typed
constructor check above. No empirical throughput or real-model-quality claim
is made. Recommended verdict: **FIX_REQUIRED** for the seven retained findings.
