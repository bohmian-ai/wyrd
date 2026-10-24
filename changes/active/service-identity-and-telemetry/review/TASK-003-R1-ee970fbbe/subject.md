# TASK-003 first remediation review

- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-3`.
- Original base: `ca2ac4a37e229a8f95667c073db11c70ad7ecb8b`.
- Prior candidate: `d1d02cced07582a8f817d3f5651e7895b96d046d`.
- Review-record commit / implementation base: `89449aa39be209b0fe34e2cfbdef87110d6f5ce8`; it adds only prior reports/task, with no source changes relative to prior candidate.
- Candidate: `ee970fbbef1511607a71c89ac16802b34724d422`.
- Review `git diff d1d02cced ee970fbbe`, separating record-only additions at 89449aa39 from six implementation commits `89449aa39..ee970fbbe`. Cumulative context is `ca2ac4a37...ee970fbbe`. Ancestry is unchanged; no patch correspondence/rebase issue.
- Initial HEAD matches candidate and worktree is clean. No `.codegraph/` directory. Source stays immutable; only new reports in this directory may be written.
- Approved spec: `changes/active/service-identity-and-telemetry/spec.md`, revision 16, and three supplements.
- Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-003-system-schemas-and-attribution.md`.
- Prior acceptance/findings: sibling review directory `TASK-003-d1d02cced/{verdict.md,findings-validation.md}` and its selected discovery reports.
- Remediation: `TASK-003-d1d02cced/TASK-003-R1-preserve-accepted-origin.md`, including committed implementation evidence.

## Bounded scope and carry-forward

Check closure of FIND-TASK-003-1 through -6, the remediation delta and affected callers/consumers/invariants/tests. Carry forward accepted unchanged catalog, schema/layout, typed projections, SDK/public declarations, stored-row lifecycle and evidence-reader behavior unless these fixes affect it. Do not repeat original comprehensive discovery. Any reopened area must identify the changed dependency or new source evidence requiring it.

| Changed owner / symbol starting points | Affected callers/consumers / proof |
|---|---|
| Gate AuthContext accepted workload; attribution frame_dimensions; Gate canonical/native dispatch; ScribeIngressFrame and ingress | HTTP/gRPC accepted-owner selection, delegated exact roots, descriptors/hash/stamp; OTLP negative and native server journeys; gateway and verification accepted-context siblings |
| Scribe execution_lanes decode_rows/enforce_builtin_source_contract/stamping | Public Frame versus private trusted Rows, drift shared dimensions, canonical resource/raw/signal validation, ServerSpans; focused unit/native/gateway readback tests |
| Scribe preprocess logical_data_identity/LogicalIdentity::for_rows and managed columns | Warm duplicate cache, WAL/fence/cold retry consumers; accepted_retry fixture and replay integration tests; changed stable origin vs volatile request/time |
| CardRegistry described/read_descriptor and retained Card owner | Live public observation_targets vs trusted row_dimensions, tombstone cold/warm/peer delivery; gateway deletion journey |
| Gateway CallCapture spans_batch/project and outbox Chunk::attributed | Each capture/attempt accepted UID/Run/environment/request/account, mixed origin, raw provider/trace preservation; configured gateway journey |
| Rustdoc across client/queue/runtime/spec/Vala/server/MCP/SQL/testing | Prior exact corrected inventory and newly changed items; only nested SQL AGENTS applies to this remediation delta; no Python/TS source changes |

This navigation map is a starting point, not a coverage result. Reviewers must trace actual source and sibling callers within their scope. Treat recorded test results and diagnoses as evidence to inspect, not an intended verdict.

## Reviewer selection

- Fresh remediation ponytail closure reviewer: all six prior-finding closures, changed code/callers, owner reuse, mandatory docs and verification adequacy. Report `remediation-review.md`.
- Domain security specialist: materially changed public Frame/trusted Rows validation and accepted delegated owner boundary, including public refusal preservation and tombstone membership separation. Report `domain-review-security.md`.
- Domain persistent-data specialist: materially changed logical identity/reuse and warm/durable replay preservation, fixture diagnosis and stable-origin propagation. Report `domain-review-persistent-data.md`.
- A separate fresh ponytail validator after those reports: independently reconcile/validate closure and any proposed findings into `findings-validation.md`.

No full first-round team is recreated. Separate repo/maintainer/system reviewers are not selected: the closure reviewer covers bounded changed-item docs, reuse and delivery recovery, with unchanged original specialist evidence carried forward. Add one focused independent source validator only if a reviewer/validator discovers a new material blocker requiring it.

No source edits, commits, broad suites or uncoordinated Cargo commands. TESTING.md forbids overlapping Cargo builds in the shared checkout: request an exclusive slot from orchestrator before Cargo/check/codegen. Clearly distinguish static inspection, actual reruns and recorded implementation evidence.
