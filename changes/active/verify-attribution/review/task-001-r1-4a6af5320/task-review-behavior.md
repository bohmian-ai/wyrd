# TASK-001 R1 behavior review

## Immutable subject

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`
- Prior reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`
- Remediation task: prior review directory
  `TASK-001-R1-close-trace-continuity-gaps.md`

The candidate remained immutable during this review. I reviewed the complete
base-to-candidate result, using the remediation commits to locate the changed
owners and the prior findings only as hypotheses. No Cargo, mise, database,
Python, or TypeScript command was run while the parallel review was active.

## Review Findings

### Critical

None.

### Important

#### BR-R1-001 — INCORRECT / REGRESSION — the context charge can consume sealing headroom twice

- **Violated obligation:** `FIND-TASK-001-1` requires the admitted context
  charge to transfer to the sealed batch while keeping the existing
  `ClientByteBudget` ceiling and its guarantee that every admitted record can
  be sealed. `ClientByteBudget::for_config` explicitly reserves exactly one
  `max_message_bytes` of headroom so admitted rows cannot prevent their own
  frame from being created.
- **Location:** `crates/shared/wyrd-queue/src/producer.rs:83-95,545-568` and
  `crates/shared/wyrd-queue/src/queue.rs:546-602`.
- **Evidence:** admission reserves `charge + context_bytes` and retains that
  reservation on the staged rows. While those rows and their context charge
  are still live, `RecordQueue::seal` clones the selected contexts and tries
  to reserve `max_message_bytes + trace_bytes`. The configured headroom is
  only `max_message_bytes`. Consequently, when other admitted work has filled
  the admission side of the budget, a traced row needs more than the full
  ceiling during sealing and is restored with `Backpressure`; retrying the
  same flush cannot free the staged charge, so it cannot make progress. The
  existing focused test deliberately leaves approximately one context's worth
  of admission slack (`state_len = (admission_limit - record) / 2 - 48`) and
  therefore does not exercise this reachable full-admission case. The existing
  `admitted_rows_cannot_take_the_sealing_headroom` test covers the invariant
  only for a context-free row.
- **Observable consequence:** a valid traced record that was already accepted
  can become permanently unsealable when the shared handle budget is full;
  `flush`/`shutdown` returns backpressure instead of publishing work that the
  queue promised its reserved sealing headroom could always frame. Normal
  Rust, Python, and TypeScript traced `record`, `observe`, and table `insert`
  calls reach this path.
- **Smallest correction:** transfer the selected contexts' already-admitted
  reservations into the sealed owner instead of reserving those bytes a
  second time. Keep one `ClientByteBudget`, the existing byte ceiling,
  first-32 selection, sealed context vector, and retry identity; do not enlarge
  the configured headroom or add a second limiter. Extend the existing
  sealing-headroom regression case with a traced row that fills the admission
  ceiling and prove it seals, then retain the current ambiguous-retry
  settlement assertions.

### Suggestions

None.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1 / REQ-005: HTTP continues valid W3C context and ignores malformed context | Current request-edge `serve_request` and request-span construction remain in the cumulative candidate; no remediation or merge removed the single continuation path. | Prior candidate server integration and gateway journeys; no current-tip runtime rerun. | PASS |
| Scenario 2 / REQ-005, AC-001: gRPC and MCP continue context | Shared gRPC request edge and MCP request path remain wired to the same continuation owner. | Prior gRPC smoke and MCP journey evidence; current-tip runtime proof not rerun. | PASS |
| Scenario 3 / REQ-004/005: verifier to judge to gateway remains one subtree | Eval execution carries the active span and gateway attempts remain beneath the gateway call. | Prior server and three-language verify journeys; Python/TypeScript journeys changed after that proof. | PASS |
| Scenario 4 / D8: deferred runs link, rather than parent, to their cause | Persisted verifier-run trace context and runner link path remain present; remediation did not change them. | Prior `pg_verification_runtime` and eval-verification evidence. | PASS |
| Scenario 5 / REQ-007, INV-004: operator and tenant sampling remain independent | `ServerSampler`, operator filtering, and sampled-parent-or-link tenant eligibility remain the one decision path; producer tracestate/reserved bits remain excluded as approved. | Prior telemetry matrix (20/20) and integration proof. | PASS |
| Scenario 6 / INV-001: authenticated tenant routing and safe catalog projection | Route identity remains keyed by the server `(TraceId, SpanId)` and carries the verified tenant/principal plus request-local sink; timeline stays on authenticated Oracle SQL. | Prior same-trace/two-tenant, unrouted, and operator-rejected integration proofs. | PASS |
| Scenario 7 / INV-005: 2,048-span cap, sibling availability, same-outbox recovery | `sustained_scribe_outage_caps_tenant_spans` now uses the role-separated cluster, holds its Scribe back, observes overflow, probes HTTP/gRPC/MCP, confirms every verifier result, starts that Scribe, settles the same outbox, checks exactly 2,048 copies, and accepts a later copy. Production was not changed for this finding. | Named test recorded green three times before the merges; the merged candidate has not rerun it. | PASS |
| Scenario 8: captured attempts remain under the exported gateway call and are not duplicated | Gateway attempt and tenant catalog paths remain distinct; post-merge gateway authentication changes do not add another attempt writer. | Prior Rust/Python/TypeScript gateway journeys; post-merge Python/TypeScript execution is unproved. | PASS |
| Scenario 9 / REQ-006: Rust request propagation and presigned exclusion | Shared authenticated transport injection and gRPC metadata path remain; external presigned requests stay outside it. | Prior shared/client tests and journeys. | PASS |
| Scenario 10 / REQ-006/D4: queued writes keep stable, bounded producer links | Admission, first-32 identity selection, sealed vector, retry transport, and Gate links remain present. | Prior queue/Gate/server proofs. The new budget remediation breaks the existing sealing-progress invariant; see `BR-R1-001`. | FAIL |
| Scenario 11: Python and TypeScript propagation and W3C propagator | Python still captures at the native boundary and exposes telemetry from its new authored package layout; TypeScript still projects through the native client. No source-level contract break was found in the merge. | Typecheck/lint passed on the merged tip, but neither runtime propagation journey was rerun after the package/generator merge. | FAIL |
| Scenario 12 / REQ-008: tenant-scoped one-hop timeline, canonical SDK shape, total order, identity dedup | Shared Rust query still uses a typed parameter and one-hop closure; `HashSet<(TraceId, SpanId)>` now retains only the first identity after the required sort. Python sync/async return the authored `SpanRecord` `TypedDict` shape from the same native result; TypeScript returns its generated canonical `SpanRecord`. | Strengthened Rust fixture passed before merges; merged-tip Python and TypeScript timeline journeys were not rerun. | FAIL |
| Scenario 13 / AC-002: complete action timeline in all three SDKs and operator parity | The three journeys still assert ordered trees, producer-to-consumer links, gateway attempts, and verifier/judge ancestry; Rust retains operator comparison. | Rust/Python/TypeScript journeys passed before the main merges. The Python and TypeScript implementations and fixtures materially changed afterward and have no current-tip runtime result. | FAIL |
| FIND-TASK-001-1: context memory is charged from admission through settlement without breaking the sole budget | Admission and retry accounting now include context strings, but sealing reserves the selected context a second time before releasing the staged owner. | New focused test passes only with deliberate slack and misses a full-admission traced seal. | FAIL |
| FIND-TASK-001-2: nonadjacent duplicate identities collapse after deterministic ordering | `trace_timeline` sorts, then retains the first identity with a local `HashSet`; fixture places another span between differing-time copies. | Focused fixture recorded green before merges. | PASS |
| FIND-TASK-001-3: same outbox caps, stays available, and recovers | Named integration test exercises one real outbox and restores its delayed Scribe route, with sibling probes and result persistence. | Recorded green before merges; no current-tip rerun. | PASS |
| FIND-TASK-001-4: both SQL helpers have their correct rustdoc | `fixed_width` and `stored` now each have the correct adjacent documentation and `# Errors` contract. | Current-tip lints passed. | PASS |
| AC-003: malformed/unsampled/foreign-tenant/attribute/deferred/no-context negatives | Source paths and dedicated negative tests remain present; no remediation weakened trust-boundary validation. | Prior owner and journey evidence. | PASS |
| AC-004: revision-1 attribution closeout | Actor/origin and judge-card/run attribution paths are unchanged by remediation and merge work in scope. | Prior server/SQL evidence. | PASS |
| AC-005 and completion standard: current resulting repository has credible targeted proof | Static format, lint, typecheck, codegen, and docs checks passed after both merges. | The three remediation tests and materially changed Python/TypeScript runtime journeys were not run on `4a6af5320`. | FAIL |
| Prohibited changes/non-goals | No migration, compatibility alias, second Scribe writer, trace-read route, auth scope change, sampling option, or speculative sampler-to-processor channel was added by remediation. | Source/diff inspection. | PASS |

## Prior-finding closure

- `FIND-TASK-001-1`: **not closed**. Context allocations are now counted, but
  the implementation double-reserves selected context bytes during the
  staged-to-sealed transition and can strand admitted traced work. This is one
  root finding, not a separate accounting finding.
- `FIND-TASK-001-2`: **closed by source**. Deduplication no longer relies on
  adjacency, and the strengthened fixture represents the prior counterexample.
- `FIND-TASK-001-3`: **closed by source**. The test uses an existing realistic
  role-separated cluster seam rather than adding a production fault switch;
  it exercises the same outbox before and after recovery.
- `FIND-TASK-001-4`: **closed**. The rustdoc is attached to the intended item.

## Open Questions

None that require a product or specification decision. The remaining queue
defect has a bounded correction inside the already-approved behavior.

## Verification notes

The merged candidate has static evidence only. Before acceptance, rerun the
three exact remediation tests on `4a6af5320`, add/run the full-admission traced
sealing regression for `BR-R1-001`, and rerun the Python and TypeScript action
timeline and gateway/propagation journeys because the merges materially
changed those SDK implementations, generated/authored types, authentication,
and fixtures. These are existing focused proofs, not a request for a new suite
or broad gate.

The stale `TASK-001` (`proposed`, spec revision 2) and R1 (`ready`) frontmatter
is non-blocking record metadata: the approved spec and implementation evidence
unambiguously identify revision 3 and no runtime behavior depends on those
fields.

## Overall result

**FAIL**

`BR-R1-001` leaves prior finding `FIND-TASK-001-1` open. No other material
behavior defect was established from source; the post-merge Python and
TypeScript runtime surfaces remain a verification gap until their existing
journeys run on the immutable candidate.
