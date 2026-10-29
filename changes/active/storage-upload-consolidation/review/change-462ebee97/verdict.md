# Integrated TASK-001–003 review: FIX_REQUIRED

The source audit is complete and the full repository gate was executed.
Eleven findings remain, separated by owner. They concern required handler/docs
gates and incorrect test expectations/observation boundaries; a failing test
alone is not classified as a production defect. No source correction was made.

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`.
- Target: `462ebee97c81772103054f7d090053ac7054bafa`.
- Approved authority: `changes/active/storage-upload-consolidation/spec.md`, revision 2, original TASK-001/002/003, applicable repository and architecture rules.
- Separately delivered foundation authority: approved benchmark-experiments revision 19 and its recorded delivery context.
- Current maintainer instruction removes benchmark measurement remediation. No benchmark run, extension or new latency requirement is prescribed.

The complete active packet and task reviews are present at the target. The
accepted TASK-001 (`5e7dec697`), TASK-002 (`793912475`) and TASK-003
(`57219dee3`) candidates are ancestors. The audit rechecked subsequently
modified shared owners and the benchmark merge rather than treating ancestry
as proof of unchanged behavior. Detailed patch correspondence is preserved in
[the independent audit](change-review.md).

## Independence, coverage and evidence

One fresh reviewer, independent of implementation/integration, owns
[change-review.md](change-review.md). It inspected the complete actual Git
range, surrounding owners/callers, approved specs, original tasks, prior
verdicts and evidence, cross-task seams, public/durable contracts, tenant and
permission boundaries, recovery, architecture reconciliation and obsolete
owner removal. Its complete matrix covers all 18 REQ, five INV, eight AC,
constraints, non-goals and mandatory repository gates. No selected report is
missing. The root orchestrated verification and records this verdict from that
independent audit, without implementing findings.

The shared upload lifecycle, sole Bifrost inventory, upload-only ACK boundary,
File/Artifact validation and metadata reuse, canonical input/output/result
owners, accepted authority, deletion dominance and fresh-install scope have
source-backed acceptance. Passing focused and journey evidence is retained;
mandatory red gates prevent whole-change acceptance. The source-audit matrix
does not claim that all full journey suites passed.

`mise run -c gate` finished with exit 100 after 3,760.75 seconds. Six tasks
failed: Wyrd family, ignored Bifrost/MCP journeys, Rust SDK journeys, Python
integration, TypeScript integration and installed CLI journeys. Checks,
documentation, codegen, packaging, lower-level families and storage emulator
lanes passed. [verification.md](verification.md) preserves exact commands,
counts, logs, focused reproductions and explicit unexecuted scenarios.

The eleven-selector focused Rust run reproduced ten failures; group lookup
passed when its lawful visibility deferral did not occur, corroborating the
incorrect fixed global query-count assumption. TypeScript staging reproduced
its count failure, Python reproduced early fixture failure, and CLI reproduced
the invalid slave-descriptor observation. Separate Darwin/Linux positive and
negative PTY probes support using the existing master without dropping echo
restoration proof or introducing a new supervisor.

## Findings and remediation ownership

Full source-local evidence, classifications, consequences, correction burdens
and closure proof are in [change-review.md](change-review.md).

| Finding | Required correction / ownership |
|---|---|
| FIND-CHANGE-001 | Scrubbed tracing on three materially changed upload handlers; storage. |
| FIND-CHANGE-002 | Exact restart accounting includes both existing cache reservations; inherited foundation test. |
| FIND-CHANGE-003 | Batch concrete missing item/panic docs, preserving storage versus benchmark provenance. |
| FIND-CHANGE-004 | Keep the MCP code/retry-hint contract check small using existing reservation injection after setup; no one-record queue or new harness requirement. SDK fixture scope remains separately recorded in R3. |
| FIND-CHANGE-005 | Two concurrency tests derive their bound from the actual configured publication owner; inherited Scribe tests. |
| FIND-CHANGE-006 | Settle the actual producer before fault windows or retained audit assertions; integrated tests. |
| FIND-CHANGE-007 | Close the preceding writer before corrupting/snapshotting WAL, retaining full exact byte equality; inherited recovery test. |
| FIND-CHANGE-008 | Rust retained-output helper reads canonical execution outputs, preserving separate full status equality; inherited SDK helper. |
| FIND-CHANGE-009 | Account exactly for the generated group execution input, excluding refused executions; integrated SDK stories. |
| FIND-CHANGE-010 | Prove exact source/read shape at a settled boundary, without asserting scheduling/visibility timing stronger than the contract; integrated group test. |
| FIND-CHANGE-011 | Observe PTY echo through the still-valid master, retaining both signal/exit/security assertions; inherited CLI test. |

Route these self-contained packets to the implementor; no implementation is
invoked by this review:

1. [Storage-specific gates](TASK-CHANGE-R1-review-gates.md).
2. [Inherited Scribe/resource gates](TASK-CHANGE-R2-inherited-scribe-gates.md).
3. [Integrated test boundaries](TASK-CHANGE-R3-integrated-test-boundaries.md).

The packets retain exact obligations and assertions; they do not authorize
production queue changes, arbitrary counter/budget widening, sleeps, ignored
errors, compatibility aliases, unrelated cleanup or performance work. Local
private implementation mechanics remain implementor-owned. Owner overlap in
findings 003/004 is explicit, so each packet fixes only its named surfaces.

## Limits, stack and handoff

One full-suite TypeScript policy invocation received a retryable capacity
refusal, while its exact isolated debug run passed. The producer was not
established; no speculative retry, worker-limit change or invented finding is
retained. Record and reconcile that verification limit on closure.
Later installed-CLI commands were not reached after its first Python failure.
Live-cloud storage was not run; emulator coverage passed and credentialed
limitations remain explicit. Timing remains unmeasured under the maintainer's
accepted scope. These limits are not reported as passing evidence.

[delivery-context.md](delivery-context.md) records the checked PR stack and
recommended stable foundation. Keep inherited benchmark work distinct from
storage-authored fixes; freeze the ready benchmark foundation before landing
dependent PRs bottom-up. No branch or PR change was made here.

**FIX_REQUIRED.** Completion is withheld. On a new immutable corrected target,
review the bounded remediation diffs and focused closure proof, preserve
unchanged source-backed acceptance, and reconcile required unexecuted journeys.
Do not restart unrelated discovery or demand optional benchmark evidence.
No source/tests, staging, commit, merge, push, deployment or active-packet
completion was performed. The only working-tree addition is this review directory.
