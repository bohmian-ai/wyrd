# TASK-001 R1 implementation verdict

**FIX_REQUIRED**

## Immutable subject

- Original base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`.
- Prior reviewed candidate: `827ec9f15f64cf308749e17fcd5fe914acf64723`.
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`.
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`.
- Approved spec: `changes/active/verify-attribution/spec.md`, revision 3.
- Original task: `changes/active/verify-attribution/tasks/TASK-001-distributed-trace-continuity.md`.
- Prior verdict/ledger and R1 remediation:
  `changes/active/verify-attribution/review/task-001-827ec9f15/`.

The cumulative original-base-to-candidate result, the R1 remediation commits,
both later main merges where they touch task behavior, all original
requirements and non-goals, prior findings, current callers, and recorded and
fresh verification were reviewed. The candidate commit/tree remained
unchanged; review artifacts are outside it.

## Independent review results

| Review | Result | Material outcome |
| --- | --- | --- |
| Behavior | FAIL | R1 context accounting double-reserves at seal and can prevent a full traced queue from ever flushing. |
| Invariants | FAIL | Same ownership-transition defect; FIND-2/3/4 close. |
| Repository standards | FAIL | Same queue headroom violation; metadata/style notes are nonblocking. |
| Maintainer | PASS | Remediation stays on existing owners with no unjustified machinery. |
| System resilience | PASS | The role-separated outage test credibly exercises one real outbox through Scribe loss and recovery. |
| Security and tenancy | PASS | Verified identity, tenant isolation, allowlisting, and INV-004 remain intact. |
| Concurrency and durability | FAIL | Same queue charge-transition defect; span permit and outbox recovery are sound. |
| Persistent data | FAIL | Timeline dedup closes, but Python's authored canonical output type is too broad. |
| SDK contracts | FAIL | Same Python required-key mismatch; Rust/TypeScript and runtime paths pass. |
| Focused follow-up | RESOLVED | Python required keys and status union are one material static-contract finding. |
| Fresh Ponytail validation | FIX_REQUIRED | `FIND-TASK-001-1` reopened/revised; new `FIND-TASK-001-5` confirmed. |

Every required report is present in this directory. The validated ledger is
`findings-validation.md`; report agreement was not used as proof.

## Reconciled acceptance matrix

| Obligation | Evidence and result |
| --- | --- |
| REQ-004/005/007/009 and INV-001–005: attribution, continuation, dual export, isolation, correlation documentation | **PASS.** Current source retains the prior accepted implementation; fresh outage and three-language action-timeline proof is green. |
| REQ-006 queued contexts stay bounded and accepted work remains publishable | **FAIL — `FIND-TASK-001-1`.** Context memory is counted, but it is reserved again during sealing and can exceed the fixed headroom at full admission. |
| REQ-008 timeline query, order, identity uniqueness, and Rust/TypeScript projection | **PASS.** Shared HashSet dedup is correct and the focused query plus action journeys pass. |
| REQ-008 canonical Python projection | **FAIL — `FIND-TASK-001-5`.** Runtime dictionaries are correct, but public authored typing makes guaranteed keys optional and admits impossible status variants. |
| AC-001–004 user/agent journeys and negative/recovery paths | **PASS for exercised behavior.** Rust, Python, and TypeScript action timelines and the Scribe outage recovery test pass on this candidate. |
| AC-005 repository checks | **FAIL pending the two source corrections.** Recorded format/lint/typecheck/codegen/docs checks are green, but those gates do not exercise either retained defect. |
| Prohibited changes and non-goals | **PASS.** No second Scribe writer, trace route, auth/scope change, compatibility alias, mesh propagation, sub/act work, migration, or speculative sampling machinery entered the task. |

## Validated findings and prior closure

| Finding | Status | Result |
| --- | --- | --- |
| `FIND-TASK-001-1` | **REOPENED / REVISED** | Admission charges context bytes, but sealing simultaneously reserves them again; transfer the existing charge into sealed ownership. |
| `FIND-TASK-001-2` | **CLOSED** | Sort followed by one identity set removes nonadjacent duplicates; focused test passes. |
| `FIND-TASK-001-3` | **CLOSED** | Existing cluster seam proves cap, sibling availability, same-outbox recovery, permit return, and later admission; focused test passes. |
| `FIND-TASK-001-4` | **CLOSED** | `stored` and `fixed_width` have correct rustdoc; format/lint evidence is green. |
| `FIND-TASK-001-5` | **OPEN / CONFIRMED** | Authored Python timeline typing does not precisely match canonical Rust output. |

The stale original/R1 frontmatter, function-local test imports, commit trailers,
and accepted linked-route clone were independently rejected as material task
findings. No retained finding requires a spec revision or migration.

## Fresh candidate verification

- Queue retained-context retry test: **PASS, 1/1**.
- Focused Postgres timeline dedup test: **PASS, 1/1**.
- Focused Scribe outage/recovery integration test: **PASS, 1/1**.
- Rust action-timeline journey: **PASS, 1/1**.
- Python action-timeline journey: **PASS, 1/1**.
- TypeScript action-timeline journey: **PASS, 1/1**.
- Candidate/tree identity and review-artifact `git diff --check`: **PASS**.

These runs close the post-merge runtime uncertainty and prove prior findings
2 and 3. The queue test does not fill admission before sealing, and runtime
journeys do not validate Python static required-key/union precision, so they do
not close findings 1 or 5.

## Follow-up decision and verdict

One follow-up was required because implementation/maintainer reviews accepted
the post-merge Python projection while two domain reviews rejected its static
contract. It inspected the canonical producer and public projections and
resolved both the required-key and tagged-status claims as one bounded defect.

**FIX_REQUIRED**

The single decision-complete remediation task is
`TASK-001-R2-close-queue-transfer-and-python-contract.md`, routed directly to
`$wyrd-implement`. It reuses existing owners and adds no architecture,
configuration, compatibility, migration, runtime conversion, or task split.
