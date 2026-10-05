# TASK-001 round-five review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Candidate tree: `b5dd2f061959724877c385cd759ee13924da2213`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 11, including
  `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Reviewed remediation:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`

The candidate remained at the stated commit throughout review. CodeGraph had
no usable repository index, so reviewers used Git, `rg`, and direct source
inspection.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Declarative Agent-only Workflow contracts, explicit bindings, pure validation, and one execution plan | Cumulative `wyrd-spec` and `skald-workflow` source plus retained contract and validation evidence | PASS |
| Deterministic namespaced execution, outputs, retries, deadlines, cancellation, bounded tasks, and complete terminal snapshots | Cumulative executor source and focused lifecycle, deadline, panic, and budget evidence | PASS |
| Native, WyrdGateway, and ExtGateway routing with isolated attempt context | Route owners and cumulative route tests | PASS |
| ExtGateway SSRF, transport, credential, reflection, retry, and tenant/request boundaries | Network-security review and focused provider/Workflow checks | PASS |
| Rust/Python local authoring and portable result projection | Shared Rust engine, SDK-owned wrapper, recorded Python/type/codegen lanes | PASS |
| Revision 11 Observer deletion and payload-free semantic tracing | Observer surfaces remain deleted; telemetry review validates hierarchy, outcomes, privacy, and retry spans | PASS |
| `FIND-TASK-001-27`: permanent Skald dependency guidance | Overview, dependency prose, and diagram now agree with manifests and doctrine | PASS — CLOSED |
| `FIND-TASK-001-28`: every harness-owned task completes or is aborted before fixture release | Bound serve-task abort-and-join is fixed, but the shared fallback can cancel `Bifrost::abort`, record settlement early, and release the fixture | **FAIL — OPEN / REVISED** |
| `FIND-TASK-001-29`: effective post-callback request model on the call span | Dispatch and span both use `request_model(request)` with the existing model-less fallback | PASS — CLOSED |
| Prohibited scope remains excluded | No compatibility path, second engine, remote Workflow surface, production lifecycle API, new dependency, or Observer replacement entered the range | PASS |

## Independent review results

| Report | Result | Material finding |
|---|---|---|
| `task-review-behavior.md` | FAIL | `BEH-R5-001` |
| `task-review-invariants.md` | FAIL | `INV-R5-001` |
| `standards-review.md` | FAIL | `STD-R5-001` |
| `maintainer-review.md` | FAIL | `MNT-R5-001` |
| `system-review.md` | FAIL | `SYS-R5-001` |
| `domain-review-concurrency.md` | FAIL | `CONC-R5-001` |
| `domain-review-network-security.md` | PASS | None |
| `domain-review-telemetry-privacy.md` | PASS | None |
| `findings-validation.md` | FIX_REQUIRED | `FIND-TASK-001-28` |

All required reviewers and reports were available. No reviewer filled more
than one role.

## Follow-up decision

No focused follow-up was needed. Discovery materially agreed on one reachable
lifecycle source and exposed no conflicting or unreviewed path. The structured
Ponytail validator independently traced and deduplicated that claim.

The user-reported residual is not itself a defect: an in-process server never
runs the Forge supervisor owned by `BoundServer::run`, so it cannot claim a
graceful Forge drain and may correctly select the existing abort fallback. The
defect is cancelling that abort completion fence and proceeding as if it had
settled.

## Validated finding ledger

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-28` | REVISED | INCORRECT | Keep the grace budget on serve/Bifrost drain attempts, then await the existing `Bifrost::abort` fence without that elapsed deadline and mark settlement only after clean drain or completed abort. Prove active-runtime implicit drop with live governed storage work. |

The source trace, rejected expansions, exact correction boundary, and focused
proof are in `findings-validation.md`.

## Prior-finding closure

- `FIND-TASK-001-1` through `FIND-TASK-001-26`: remain closed.
- `FIND-TASK-001-27`: closed by the corrected Skald architecture guidance.
- `FIND-TASK-001-28`: open and revised as recorded above.
- `FIND-TASK-001-29`: closed by effective-request model attribution and its
  focused test.

## Verification limits

- Independently rerun callback-model proof: 1 selected, 1 passed.
- Independently rerun teardown proof with repository-managed Postgres: 2
  selected, 2 passed.
- Cumulative `git diff --check` passed, and the candidate records green format,
  lint, Skald, Wyrd, docs, and client-tier lanes.
- The bound test credibly proves serve-task abort-and-join. The in-process test
  uses idle storage off-runtime, so it does not exercise a pending abort under
  active-runtime zero-budget drop and cannot close `FIND-TASK-001-28`.

## Verdict

**FIX_REQUIRED**

One bounded correction remains within approved Revision 11. The remediation
task is `TASK-001-R4-await-bifrost-abort-before-fixture-release.md` in this
directory.
