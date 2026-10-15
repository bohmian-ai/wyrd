# TASK-001 independent review verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`
- Base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`
- Candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`
- Approved specification: [spec.md](../../spec.md), revision 2.
- Original task: [TASK-001](../../tasks/TASK-001-framework-agnostic-telemetry.md).
- Range: complete cumulative base-to-candidate, including the original bridge,
  revision-2 implementation, and committed integration/dependency follow-up.
- This is the first independent task review. No previous FIND IDs or remediation
  closure is assumed. Owner-directed ambient auth, docsite, Python-floor and
  ordinary integration-lane changes are part of the reviewed authority.

The candidate was pinned from a clean tracked working tree. HEAD, source,
specification, original task and index remained unchanged through review. Only
this new review directory was written. No merge, push, staging or deployment
was performed. PASS/task completion is not granted.

## Independent review results

The orchestrator did not act as a discovery reviewer or validate its own
findings. Each role used a separate fresh agent with the same starting map,
complete diff, applicable authority and available verification. Discovery
reviewers received no other reviewer's conclusions or intended verdict.

| Role | Report | Discovery result |
|---|---|---|
| Behavior acceptance | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Invariant acceptance | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/tenant/payload trust boundary | [domain-review-security.md](domain-review-security.md) | PASS; no material proposal |
| Concurrency | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| OTEL/framework interoperability | [domain-review-otel.md](domain-review-otel.md) | FAIL |
| Durability/delivery lifecycle | [domain-review-durability.md](domain-review-durability.md) | FAIL |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; six retained root causes |

All required reports are complete. The validator independently dispositioned all
17 material discovery proposals. Its source-backed, deduplicated ledger is the
only finding authority used below; agreement counts are not proof.

## Claim comparison and follow-up decision

[claim-comparison.md](claim-comparison.md) groups findings by violated behavior
and owner. No focused followup-rev was needed: no opposing reachable source
facts remained; broad PASS rows were narrowed by specific cases. The stock
ConcurrentMultiSpanProcessor recommendation was an explicitly uncertain
alternative, independently rejected as an unsafe whole-provider substitution.
Validation selected a bounded managed-lifecycle wait that preserves ordinary
callback context and existing exporter ownership. No new public/product,
security, persistent-data or resource-ownership decision is needed.

## Reconciled acceptance matrix

The validator's [complete matrix](findings-validation.md#reconciled-complete-acceptance-matrix)
records source and proof for subobligations; this compact table records overall
status for every spec ID. A FAIL concerns only its retained gap, not adjacent
passing behavior. PASS is subject to the verification limits below.

| Obligation | Result | Reconciled evidence / remaining finding |
|---|---|---|
| REQ-010 | FAIL | Real bridge/callback fidelity substantially supported; optional TS declarations and supplied Python Responses usage remain: 1, 4 |
| REQ-011 | PASS | Three actual Python frameworks and TS OpenAI use native/bridged OTEL and optional Run correlation |
| REQ-012 | FAIL | Owned-provider preservation/retry and sequential setup supported; overlapping same-state TS setup fails: 6 |
| REQ-013 | FAIL | Returned-error precedence/drain supported; effective Python bound and live-flush continuation proof remain: 5, 3 |
| REQ-014 | PASS | Native OTLP fidelity, scope, sampling/content controls and version-qualified limitations supported |
| REQ-015 | FAIL | Framework installation/construction prerequisites incomplete: 2 |
| REQ-016 | FAIL | Real framework/negative execution and lane collection supported; successful managed live-flush journey absent: 3 |
| REQ-017 | PASS | Existing ambient auth/current-token/export and stock-client gateway mechanisms reused |
| INV-001 | PASS | Authentication owns tenant/publisher; Card attribution and read authorization retained |
| INV-002 | PASS | No new server protocol/storage/runtime/registry/export retry queue; Rust/log/metric redesign excluded |
| INV-003 | PASS | Payload-free bridge/errors and application exception semantics retained |
| INV-004 | PASS | Managed/owned provider boundaries and Run non-lifecycle role retained |
| AC-001 | PASS | Actual three-framework Python and TS OpenAI model/tool readback |
| AC-003 | FAIL | Existing gates miss generic TS consumer declarations without optional peers: 1 |
| AC-007 | FAIL | Optional typed-consumer contract and safe supplied Responses usage: 1, 4 |
| AC-008 | FAIL | Same-state overlapping setup: 6 |
| AC-009 | FAIL | Actual Python flush bound and live-flush/provider continuation proof: 5, 3 |
| AC-010 | PASS | Delivered generic/nested/concurrent/enclosing/error/stream identities and no-duplicate assertions supported |
| AC-011 | PASS | Sampling/content/auth/tenant-negative boundaries supported; usage omission is not a payload leak |
| AC-012 | FAIL | Executable framework recipes incomplete: 2 |
| AC-013 | PASS | Ambient gateway/credential refusal/fresh exporter token paths supported |

Non-goals remain excluded. Owner-directed Python 3.11 and dev-dependency
collection changes are authorized. Missing historical RED records and test
environment observations are nonblocking notes, not invented runtime defects.

## Validated finding ledger

| Stable ID | Status / class | Required correction boundary |
|---|---|---|
| FIND-TASK-001-1 | CONFIRMED / REGRESSION | Keep optional Agents types out of generic TS root declarations; extend existing clean-package noEmit proof |
| FIND-TASK-001-2 | CONFIRMED / MISSING | Complete existing framework recipe installations/construction with ambient-auth/current docsite conventions |
| FIND-TASK-001-3 | REVISED / MISSING | Prove managed live flush, native writer continuation, active spans, and global export after state shutdown through existing real journeys |
| FIND-TASK-001-4 | REVISED / INCORRECT | Preserve supplied payload-free Python Responses usage in the existing mapper with full-response fallback |
| FIND-TASK-001-5 | REVISED / INCORRECT | Bound managed Python lifecycle waiting despite the pinned stock processor's ignored timeout, preserving synchronous callbacks and stock export ownership |
| FIND-TASK-001-6 | CONFIRMED / INCORRECT | Recheck same-state installation after the TS dynamic import await |

Exact locations, producer-to-consumer reachability, all discovery source IDs,
rejected portions of recommendations and focused closure commands are preserved
in [findings-validation.md](findings-validation.md). No unvalidated finding or
optional improvement is added by the orchestrator.

## Verification evidence and limits

Fresh review-time checks in [verification.md](verification.md):

- Cumulative diff whitespace check: PASS.
- Exact Python setup/owned/lifecycle/callback/fresh-token selectors: 14 passed.
- TS telemetry unit file: 5 passed.
- Exact shared Rust lifecycle failure-settlement test: 1 passed.

Discovery independently reproduced ordinary TS consumer TS2307 without the
optional Agents package. Validation probed actual pinned Python dependencies:
a 1 ms stock force-flush request returned True after 150 ms, and payload-free
ResponseSpanData usage was omitted by the current mapper. Controlled work was
released; no host-load test or source change was used.

Committed task evidence supplies historical broader integration, format, lint,
type, codegen, dependency and docsite results. Those aggregates were not rerun
in this review; fresh checks used installed development artifacts, with an
explicit focused Rust build. Neither green mocks nor shutdown-only readback
proves the retained cases. Final integrated/shared-infrastructure verification
remains change-review work, not an extra remediation-wide test battery.

Python's stock SDK logging a refused export while force_flush returns true is
a documented best-effort limitation, not an independently retained finding.
Upstream TS Chat Completions missing usage and TS not stamping already-active
spans are not expanded into new requirements. No prior finding closure exists.

## Remediation handoff

One self-contained task under approved r2:
[TASK-001-R1-telemetry-contract-closure.md](TASK-001-R1-telemetry-contract-closure.md).
Route it directly to `$wyrd-implement`. It is a bounded implementation correction,
not a specification redesign or another planning gate. Re-review the cumulative
base-to-new-candidate range against the original task and this ledger afterward.

Broad aggregate reruns and implementation changes were skipped. The six retained
gaps block task completion, particularly the ineffective Python flush deadline.
