# Independent findings validation — TASK-002

Recommended verdict: **FIX_REQUIRED**. One material documentation finding is
confirmed. No runtime or client implementation correction is required.

## Subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-code-mode`.
Base: `e5af2ce5caef2d00303e1c980b12c8a9bca4af0e`.
Candidate: `ff78b51b61fea8f215a2139b17840044f65b17e4`.
This is the first review, using the complete `git diff base...candidate`.
HEAD was checked and remains the candidate. Approved specification revision 2
was read from candidate Git, and the original task from base Git. The working
revision 3 and other task packets/reviews are excluded. There are no prior
TASK-002 findings or closure decisions to preserve.

Read the review skill, scope, verification, all four discovery reports, and
claim comparison. Checked the proposed finding against actual producers and
consumers and checked the empty ledgers against their assigned coverage.
Read applicable workflow, maintainer, SDK/docs rules and the Verifier's tool
authority. There is no CodeGraph index. Source inspection used the immutable
diff and unchanged runtime dependencies; no source was edited. This report is
the only write by this validator.

## Specialist applicability and coverage validation

| Boundary | Required coverage and independent validation | Result |
| --- | --- | --- |
| Public nested execution, declaration, success/denial/recovery, citations and documentation | Required behavior reviewer covers the complete task. Inspected the Python/TypeScript/MCP additions: each uses existing public execution APIs, the common Card declarations and extracted programs, real server/worker setup, scoped principals, provider request inspection, and normal retained output. Shared-program extraction retains the original Rust program operations. | Covered; documentation exception below |
| Multi-stage call records, historical defaults and custom data | Invariant reviewer is required and present. Independently checked `Dispatch::call`, shared `RequirementResult`/`StepResult`/`ToolCallRecord` serde defaults, Python `PyVerificationResult::typed/requirements/steps` and `decode_data`, TypeScript `typedResult`, `ResultPayloadBuilder::build`, and `InputResolver::result`. Whole slices/maps are retained; typing replaces only `data`; no alternate persistence or decoder is added. | Covered |
| Cross-language/repository rules | Repository reviewer is required and present. Its table covers every diff surface, authored stubs, type fixtures, Rust test/helper rustdoc, schema test, shared programs and generated docs. Independently inspected these diffs and rustdoc. No manifest, binding implementation, production SQL, platform feature or handler is changed. | Covered |
| Accepted authority, protected reads and result visibility | Security reviewer is required and present. Independently checked `Invocation::call`: exact tool authorization precedes dispatch, Cards use the accepted caller, and Bifrost uses `BoundedQuery` with that caller. `Dispatch::call` retains failure records without delivered bytes/digest. Public tests perform actual denied and granted invocations and assert the catalog code, recovery and no nested Card content in provider requests. No new identity selector, raw output record or result-reader route exists. | Covered |
| Ownership/abstraction | Separate maintainer reviewer correctly omitted. Production changes are authored Python result types; added MCP helpers prepare/assert caller-owned test work. No new runtime, storage, lifecycle, identity or transport owner exists. Existing fixture, provider and result owners are reused. Behavior review supplies the local maintainability assessment. | Omission justified |
| Failure propagation and deployed recovery | Separate system reviewer correctly omitted. No executable runtime, process topology, worker owner or deployment setting changes. Documentation claims about suspended workers and ordinary/host outcomes are bounded descriptions of unchanged dependencies. Full TASK-001 fault matrices are not TASK-002 acceptance proof. | Omission justified |
| Persistent-data and concurrency domain | No additional specialist required: no storage/SQL/transaction/coordination/concurrency implementation changes. Invariant and security reviews cover retained projection and authority, respectively. | Omission justified |

All selected reports are present. Their empty ledgers are supported by scoped
source coverage, rather than accepted merely because they are empty. The
repository review's local import/signature placement observation has no
material failure or mandatory-gate consequence and creates no remediation.
The other reports' broad documentation PASS assessments do not rebut the
specific last-expression path identified by the behavior reviewer. No material
conflict requires another discovery pass. No new blocker was discovered by
this validator, so separate source validation of a new proposal is unnecessary.

## Proposal validation

### BEH-002-1 — CONFIRMED

**Violated obligation.** The original task's documentation acceptance requires
readers to understand bounded outer output and intermediate visibility.
Approved revision 2 REQ-001 preserves existing outer response shapes, and
REQ-014 explicitly allows the bounded outer `code.run` value/printed output.
The published reference's claim that the model sees only printed text is
stronger and false. This is distinct from automatic delivery of every nested
result, which the implementation correctly avoids.

**Changed locations.**
`docs/src/content/docs/reference/agent-tools-and-authorization.md:34`,
`fixtures/cards/verifier_building_blocks/programs/nested-investigation.py:14`,
and the generated projections in `docs/public/llms-full.txt` (the prose at
line 3794 and the embedded program comment).

**Producer-to-consumer proof.** `Sandbox::exec`
(`hosted/sandbox.rs:189–222`) explicitly returns the last expression and
bounded printed output; its `TurnEvent::Complete` branch returns the worker
value. `Invocation::code` (`hosted/tools.rs:331–374`, full body inspected)
passes source to this owner, computes `repr(value, tool_result_max_bytes)` and
returns `{value, value_truncated, output}`. The existing CodeRun descriptor
at `hosted/tools.rs:608–615` promises printed output and last-expression repr.
`Dispatch::call` (`vala-verify/src/blocks/tools.rs:612–700`, full body inspected)
bounds the entire returned JSON without stripping `value`.
`RecordedTool::invoke` (`tools.rs:922–928`) delivers that value with recipient
`Model`. Both bridge-enabled and ordinary code execution share this output
owner; Python, TypeScript and MCP do not filter it.

**Supported reachability.** A declared authorized program can assign the
result of `await call_tool('cards.get', ...)` to `card` and end with the
expression `card`, with no print call. The same exact declared Card lookup is
already used by the shared investigation program and public journey. A small
successful result fits the ordinary result cap. Its repr enters the outer
`value` field and the next model turn. No malformed input, foreign tenant,
speculative API or failed-result path is needed.

**Consequence and materiality.** The principal public guide teaches a false
selection rule for the feature's core purpose: a reader can leave authorized
intermediate data as the last expression believing omission of `print` keeps
it out of model context. Bounded raw content then reaches the model despite
the described reduction/visibility rule. This does not demonstrate an
authorization bypass or automatic nested-result leakage, and neither is
claimed. It materially misstates the supported output contract and is not a
stylistic wording preference. The current journeys end in `print`, so their
green results cannot prove the published general claim.

**Correction-burden ladder.** Deleting code-mode guidance would lose required
documentation. The existing runtime descriptor and response shape already
solve execution correctly; reuse their explanation. A few prose/comment
edits plus existing generated-doc regeneration suffice. No new dependency,
guard, API, test harness or runtime behavior is warranted. A consumer guard
would incorrectly alter preserved response semantics. The program-specific
comment in `reduce-notes.py` accurately describes that actual program and
does not require a blanket rewrite.

## Final deduplicated ledger

### FIND-TASK-002-1

- Discovery source: `BEH-002-1`.
- Validation: **CONFIRMED**.
- Classification: **INCORRECT** public documentation.
- Obligation: TASK-002 documentation acceptance; approved revision 2
  REQ-001 and REQ-014.
- Locations, source evidence, reachable scenario, consequence and materiality:
  as established above. One cause owns the prose, embedded example comment
  and generated copies; these are one finding, not per-surface findings.
- Smallest decision-complete correction: in the existing agent-tools source
  reference, explain that the bounded printed output **and repr of the last
  expression** are model-visible; intermediate results stay inside Python
  unless included in either outer channel. Reconcile the investigation
  program's general print-only comment to the same semantics without changing
  its operations. Regenerate `docs/public/llms-full.txt` through the existing
  `docs:generate` owner; do not hand-edit the generated file. Preserve runtime
  response fields, bounds, all security/capture/record behavior, and the
  tested program's output. This requires no new product or architecture
  decision.
- Correction burden: a bounded documentation/comment change and existing
  generation/check workflow. Changing interpreter, dispatch or SDK output
  behavior would be larger, unnecessary and contrary to approved REQ-001.
- Focused closure proof: compare revised text/comment with the existing
  CodeRun descriptor and `Invocation::code`, verify no general print-only
  claim remains in source/generated copies, and run `mise run docs:check`
  using its normal generated-drift discipline. No new runtime test or full
  journey suite is required for this explanation-only correction.

## Reconciled acceptance

| Acceptance group | Reconciled evidence | Result |
| --- | --- | --- |
| Same declared preparation/check program, existing catalog and bounded selected outer output | Python/TS/MCP public journeys and shared fixture consumers; source dispatch/output owners; focused passing execution evidence | PASS |
| Downstream denial, exact code, recovery and retained honest records | Public principals/calls and assertions; unchanged common authorization/dispatch path; security report | PASS |
| Immediate/queued/status/retained results, custom data, historical defaults | Generic SDK conversions and serde defaults; retained producer/reader; invariant report and focused journeys/typechecks | PASS |
| Original pinned citations | Public check assertions include the notes evidence slot, `/n07` citation and custom hits; no new citation source | PASS |
| HTTP/MCP/stub/generated parity and budget catalog projection | Served schema assertions and actual MCP outputs; authored Python aliases/optional fields; generated TS owner and unchanged error catalog projection | PASS |
| Public guidance: authority, limits, occupancy, errors, citations, upgrade and realistic savings | Existing source references and runtime owners; no universal guarantee | PASS except output visibility |
| Public guidance: exact model-visible output channels | False print-only claim remains | FAIL — FIND-TASK-002-1 |
| Ownership, non-goals, required repository rules and preservation of existing consumers | Complete diff and selected standards/security coverage; no new runtime/client owner, endpoint, transport, store or platform behavior | PASS |

The review is not blocked and requires no specification revision. The bounded
documentation correction above is sufficient for the retained finding.

No builds or tests were run by this validator. Verification outcomes in the
candidate task and `verification.md` remain supplied execution evidence;
the latter independently records passing Python/TypeScript typechecks and
range whitespace validation. Full journey suites and TASK-001 fault matrices
were not repeated. The remaining risk is the published false output-selection
guidance until FIND-TASK-002-1 is corrected.
