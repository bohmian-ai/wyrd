# TASK-001 repository / SDK boundary review

Subject: base `08998b9e9b6f04825f7743b877391247e0ad9621`, candidate
`9a0b9112523252328813041c0b11be375f0eed29`, SPEC-verdict-response-mapping
revision 1 and original TASK-001 at base, implementation evidence at candidate.
Source was read in `/tmp/wyrd-task-review-9a0b91125`; the shared main checkout
is not the candidate. No implementation changed. The detached checkout has no
`.codegraph`, so inspection used `git diff`, `rg`, and direct source reads.

Scope: cross-tier Prompt/question/transform authoring, language coercion,
exports/stubs/generated types, snapshot ownership and allocation evidence,
SDK assertion strength. Authorities include root and both SDK AGENTS,
spec-driven development, maintainer style, Rust/errors guides, TESTING, and
the Prompt/Verifier/in-memory-authoring architecture sections. Core execution,
hot-path waits, retention and tenant authorization have separate reviewers.

## Coverage

| Assigned boundary | Source and assertions inspected | Assessment |
| --- | --- | --- |
| REQ-001/002 native and typed Jev questions | skald-prompt builder.rs question structs/From conversions/jev; Python prompt.rs; decisions.py; TS definitions.ts classes/Prompt.jev and native authoring.rs::build_jev | Uses the actual Prompt; native dictionaries are distinguished from typed objects and lowering is shared in Rust. Constructor checks on full Prompt use JevRequestView bounds. |
| REQ-005/013 policy projections | spec AgentCheck and VerdictTransform; client canonical_values and Check fields; SDK native setters, exports and authored stubs; generated card-types.ts and N-API declarations | Leaf union is projected across SDKs; explicit-null contract mismatch independently corroborated below. |
| REQ-011 construction and assembled checks | Python _transforms.py and definitions.rs::transform; TS authoring.rs::transform; Python/TS verdict-transform-authoring unit files | Assembled graph tests cover wrong/missing questions and Choice maps. Typed Python probability construction misses self-contained bounds. TS malformed-policy rejection loses structured diagnostics. |
| REQ-001/002/013 typed/native/read-back proof | Python test_typed_jev_choice_check_becomes_the_verdict; TS typed jev choice check; public-exports no-Pydantic subprocess test | Connected declarations/read-back/redeclaration and literal upstream body assertions are substantial. Optional models are lazy; dictionary-only path is executable. Python declares typed/native forms, compares mixed Prompt locally; TS also declares mixed form. Saved-bundle reconstruction remains explicitly unexercised in task evidence. |
| Allocation / capture semantics | shared authoring CardDefinition Arc capture and canonical_values; skald Prompt native ownership; Python json_value; TS jsonSnapshot/NativeValue; new jev builder | Existing capture/snapshot mechanisms reused. New complete-request JSON materialization and SDK question conversions lack required matched profiling evidence. |
| Future scope | AllTransform/Google | Excluded by TASK-001 allocation and task boundaries; absence is not a defect. |

Recorded language journeys and checks are implementation-provided evidence; this
review did not rerun their native artifacts. The small pure-Python reproduction
below loads the candidate source itself and requires no server/native extension.

## Proposed blocking findings

### REPO-1 — INCORRECT: Python typed ProbabilityTransform accepts invalid bounds

Authority: REQ-011 explicitly requires typed value construction to check
self-contained constraints, including probability bounds; TASK-001 scenario 4
requires the same boundary. Location:
`sdks/wyrd-sdk-python/python/wyrd/verify/_transforms.py:49–67`.

Starting with the public Pydantic model, constructing
`ProbabilityTransform(question="helpful", expected=True, pass_at=.2, fail_at=.8)`
succeeds. So do `pass_at=1.5` and `pass_at=NaN`. Expected: construction refuses
with the offending field/correction. Actual: plain float fields and no model
validator leave the policy invalid until `Check.agent` eventually calls the
Rust validator (`src/verify/definitions.rs:64`). Existing negative tests pass
raw dictionaries into Check.agent; they do not test model construction, so
they do not establish the promised earlier boundary.

Smallest correction: route typed-model construction through the shared Rust
self-contained policy validator, preserving dictionary-only optional-Pydantic
use and assembled-graph validation. Add public model-construction cases for
reversed/out-of-range/nonfinite bounds and a corrected valid model. Do not
duplicate behavioral rules in an independent Python validator.

Reproduction executed through mise in the main repository, using its installed
Python environment solely for Pydantic; the loaded model source is immutable
candidate source:

```bash
mise exec -- /home/thorrester/Documents/GitHub/wyrd/sdks/wyrd-sdk-python/.venv/bin/python - <<'PY'
import importlib.util
import sys
spec = importlib.util.spec_from_file_location('candidate_transforms', '/tmp/wyrd-task-review-9a0b91125/sdks/wyrd-sdk-python/python/wyrd/verify/_transforms.py')
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
for bounds in [(0.2, 0.8), (1.5, 0.1), (float('nan'), 0.1)]:
    policy = module.ProbabilityTransform(question='helpful', expected=True, pass_at=bounds[0], fail_at=bounds[1])
    print('accepted', repr(policy))
PY
```

Output:

```text
accepted ProbabilityTransform(kind='probability', question='helpful', expected=True, pass_at=0.2, fail_at=0.8)
accepted ProbabilityTransform(kind='probability', question='helpful', expected=True, pass_at=1.5, fail_at=0.1)
accepted ProbabilityTransform(kind='probability', question='helpful', expected=True, pass_at=nan, fail_at=0.1)
```

This is a focused source reproduction with installed Pydantic, not a rerun of
the candidate wheel or its full SDK journey. Runtime: Python 3.12.3, Pydantic
2.13.4 at `/home/thorrester/Documents/GitHub/wyrd/sdks/wyrd-sdk-python/.venv/lib/python3.12/site-packages/pydantic/__init__.py`.
An initial importlib harness run
omitted sys.modules registration and failed Pydantic forward-reference setup;
registering the module corrected that harness defect before the above result.

### REPO-2 — INCORRECT: new TypeScript malformed-policy refusal drops catalog metadata

Authority: REQ-011 structured SDK diagnostics, `agent_docs/errors.md` per-surface
projection and validation failures, TS AGENTS Error contract. Location:
`sdks/wyrd-sdk-ts/native/src/authoring.rs:427–432`, with `invalid` at line 34;
consumer `wyrd/src/definitions.ts:245–247`.

Starting with a valid judge, the public
`Check.agent(judge, {verdict_transform: {kind: "probability", question: "helpful", expected: true, pass_at: .2, fail_at: .8}})`
enters the new Rust transform helper. `VerdictTransform::validate` returns the
canonical Wyrd validation error, but the helper replaces it with
`napi::Error(Status::InvalidArg, error.to_string())`. The public setter calls
native `.set` directly and does not project an error envelope. Expected:
WyrdError with exact catalog code and field/remediation; actual: generic N-API
InvalidArg with text. Agents cannot branch on the same catalog code as Python
or registration. Malformed tagged-union decoding follows the same generic path.

Smallest correction: preserve/project the existing canonical validation error
at this newly added policy boundary using the SDK's existing native error
mechanism; assert WyrdError identity, `WYRD_SPEC_400_VALIDATION`, and field data
for public constructor/setter refusal, then corrected valid construction. No
new error family or repository-wide authoring rewrite is required. Current TS
unit tests only assert `.toThrow(field)` and therefore conceal metadata loss.
This finding is source-backed; no candidate-bound N-API runtime was rebuilt.

### REPO-3 — EVIDENCE_GAP: required authoring allocation comparison is absent

Authority: spec `SDK and server allocation review`, original TASK-001 lines
69–79, packet README allocation guidance. Locations:
`crates/skald/skald-prompt/src/builder.rs:621–636`, Python prompt.rs::jev and
TS Prompt.jev/build_jev; task scenario 1/2 allocation notes (lines 128–131,
171–172) and scenario 4 evidence.

The new native builder serializes the entire owned Jev request into a fresh
JSON Value, then derives a borrowed view from that copy for validation. This
duplicates state/instructions/criteria before ordinary Prompt construction;
the boundary also newly converts each Python question via JSON text/Value or
TS snapshot/Value into native questions. The task records that construction
cost, but no matched before/after representative authoring workload, allocation
volume/bytes or retained-memory observations/limits. The spec explicitly
requires those observations when the affected ownership/allocation paths change;
being outside request execution does not waive the authoring requirement.

Consequence: acceptance lacks required evidence that new authoring, attachment
and graph operations preserve the approved allocation practices. This finding
does not claim a measured throughput regression. Smallest closure: inspect and
record the reused CardDefinition/Prompt/native-view owners and new copies;
remove avoidable intermediate copying through an existing owner if available,
then use existing profiling tools on an equivalent same-host before/after
workload with representative question sizes/reuse and report allocation/timing
and tool limits. Do not introduce a benchmark framework or arbitrary budget.
Verification/gateway benchmarks are separately owned and do not replace this
explicit authoring proof.

## Corroboration of primary-review proposal

The primary reviewer raised explicit-null policy handling. Source independently
confirms generated `wyrd/src/card-types.ts:851` exposes
`verdict_transform?: VerdictTransform | null`; the derived optional schema
therefore admits the canonical null form. Both native transform setters decode
the present value directly into VerdictTransform, while
`crates/wyrd-spec/src/card/verifier/transform.rs:68` removes a present field and
decodes it as a nonoptional transform. Thus a typed canonical null or the
serde-serialized None form fails, although omission succeeds. This is a
projection mismatch in REQ-005/013; it is the primary reviewer's proposal,
not a separately numbered duplicate finding here. A focused absent/null parity
case at the shared owner plus generated-consumer coverage would pin closure.

## Limits

No Cargo builds, broad SDK lanes, codegen, or benchmarks were rerun. Existing
recorded green evidence was inspected alongside assertions; the review does
not infer candidate runtime identity from locally installed native modules.
No changes made to code, tests, fixtures, migrations, dependencies or exports.
