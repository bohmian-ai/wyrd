# Repository Standards Review

## Review Findings

### Critical

None.

### Important

- **STD-R5-001 — Test-server teardown can declare Bifrost settled while its
  required abort is still incomplete.**
  `crates/wyrd/wyrd-testing/src/server.rs:831-846` sets
  `bifrost_settled = true`, gives graceful shutdown one deadline, then bounds
  the fallback `Bifrost::abort()` with that same deadline. On graceful expiry
  the fallback therefore has no remaining settlement budget; a zero-budget
  `Drop` also only polls abort until the immediately expired timeout wins.
  This conflicts with the owner contract at
  `crates/vala/vala-bifrost-redux/src/storage/mod.rs:1041-1058`, where abort is
  intentionally unbounded because it must await every retained loader and
  governed request before reporting quiescence. The server is then free to
  release `PgFixture` after merely warning, recreating the database-after-drop
  race `FIND-TASK-001-28` required this remediation to close. The focused
  in-process test at `crates/wyrd/wyrd-testing/src/server.rs:5479-5505` has no
  active loader or request, so the abort completes in its first poll and does
  not exercise the reachable delayed-abort path. Preserve the bounded graceful
  attempt, but once it fails or is skipped, await the existing Bifrost abort to
  completion before setting `bifrost_settled` or releasing the fixture; add a
  deterministic barrier-backed test that holds real governed storage work
  active through teardown and proves fixture release waits for abort
  settlement. Do not add another lifecycle abstraction or another timeout
  around the abort owner.

### Suggestions

None.

## Open Questions

None. The existing Bifrost lifecycle contract resolves the correction: abort
is the final unbounded quiescence owner after bounded graceful shutdown.

## Overall Result

**FAIL**

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 11, including
  `REQ-053`
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Remediation task:
  `changes/active/skald-workflow-runtime/review/TASK-001-r4/TASK-001-R3-close-round-four-review-gaps.md`

The candidate remained at the stated commit throughout review. No
`.codegraph/` directory exists, so navigation used Git, `rg`, and direct source
and caller inspection.

## Authority Coverage

| Changed surface | Applicable authority | Result and evidence |
|---|---|---|
| Complete cumulative task range, approved Revision 11, original task, prior findings, and R3 remediation | `AGENTS.md` completion and verification rules; `architecture/agent-rules.md`; `languages/spec-driven-development.md`; `languages/testing-workflows.md` | **FAIL.** The cumulative range remains mapped and immutable, but the new teardown proof does not exercise delayed abort settlement and the implementation can still release the fixture before abort completes (`STD-R5-001`). |
| Skald/Wyrd dependency documentation | `AGENTS.md` ownership boundaries; `architecture/wyrd-doctrine.mdx`; `doctrine/architecture-constraints.md`; `architecture/patterns.md` | **PASS.** `docs/architecture/skald.md:3-68` now states the narrow foundation edges, includes `skald-workflow -> wyrd-spec`, and preserves the prohibition on `wyrd-server` and `vala-*` dependencies. The listed edges agree with the Skald manifests. |
| Agent GenAI call telemetry and callback-replaced requests | `AGENTS.md` tracing and payload rules; `domain/telemetry-observations.md`; Revision 11 `REQ-053` | **PASS.** `chat_span` derives the model from the effective post-callback request and uses the resolved Prompt model only for model-less request shapes. The focused test proves dispatch and span attribution while retaining payload exclusion and the existing provider/operation fields. |
| `WyrdTestServer` lifecycle owner, serve-task join, Bifrost drain/abort, fixture release, and teardown tests | `AGENTS.md` async/runtime, testing, gate-integrity, and completion rules; `architecture/agent-rules.md`; `architecture/bifrost-design.md`; `domain/analytical-operations-reliability.md` | **FAIL.** Timed-out serve tasks are now aborted and joined, but Bifrost abort is incorrectly put behind the expired graceful deadline and settlement is recorded before proof. The idle-storage test cannot establish the required live-work ordering (`STD-R5-001`). |
| Rust owner shape, errors, imports, rustdoc, and dependency cost in the final remediation | `AGENTS.md` sections 4-6 and 16; `architecture/agent-rules.md`; `languages/rust-core.md`; `languages/maintainer-style.md` | **PASS except for the lifecycle correctness finding.** The change remains on the existing `WyrdTestServer`, `Agent`, and Bifrost owners; it adds no trait, crate, dependency, feature, lint suppression, ignored test, or ad hoc runtime. Changed items are documented. |
| Cumulative contracts, generated schemas, SDK projections, provider/ExtGateway safety, Workflow engine, and Observer deletion | The authorities mapped in the prior cumulative standards audit: `AGENTS.md`, `wyrd-design.md`, `wyrd-doctrine.mdx`, security posture, language boundary references, error guidance, and external-network rules | **PASS.** Final remediation does not reopen those surfaces. Direct comparison of the cumulative diff and final delta found no new contract, security, tenant, audit, PyO3, generated-artifact, or client-tier violation. Prior `FIND-TASK-001-1` through `-26` remain closed. |

## Per-Rule Results

| Rule area | Result | Evidence |
|---|---|---|
| Wyrd/Skald/Vala ownership and locked dependency direction | PASS | The corrected architecture prose and diagram agree with current manifests and preserve the server/Vala boundary. `FIND-TASK-001-27` is closed. |
| Struct-centered Rust and abstraction discipline | PASS | Lifecycle behavior remains on `WyrdTestServer`; telemetry behavior remains on the Agent loop owner; no replacement abstraction was added. |
| Sync/async, cancellation, and owner settlement | **FAIL** | The graceful deadline is reused to bound the final abort, despite the existing abort owner's documented requirement to await all retained work (`STD-R5-001`). |
| Rust imports, errors, unwrap/expect policy, dependencies, and lint suppressions | PASS | The final delta adds no dependency, production `unwrap`, unjustified `expect`, `#[allow]`, or `#[ignore]`. |
| Rust documentation | PASS for presence; **incorrect lifecycle claim covered by STD-R5-001** | New rustdoc is substantive, but its claim that zero budget aborts and joins owners directly is not established when abort remains pending past its first poll. |
| Telemetry semantics and payload safety | PASS | The effective request model is traced; provider and operation attributes remain stable; payload markers and scalar finish reasons remain absent. `FIND-TASK-001-29` is closed. |
| Test taxonomy, runtime ownership, and proof credibility | **FAIL** | Tests are in valid owners and exact commands are recorded, but the in-process case proves only idle-owner closure, not delayed live-work abort before fixture release (`STD-R5-001`). |
| Gate integrity and verification scope | PASS with a behavioral proof gap | No gate was weakened. Recorded broad lanes are appropriate, but green suites do not prove the unexercised delayed-abort ordering. |
| Tenant SQL, authorization, audit, public contracts, PyO3, and generated artifacts | PASS / no new final-delta violation | The final remediation changes none of these durable boundaries. |

## Prior-Finding Closure

| Prior finding | Standards result |
|---|---|
| `FIND-TASK-001-27` | **CLOSED.** Overview, dependency prose, and diagram now agree on the narrow `skald-workflow -> wyrd-spec` edge and retain the no-server/no-Vala boundary. |
| `FIND-TASK-001-28` | **NOT CLOSED.** Serve-task detachment is fixed, and in-process shutdown reaches the Bifrost owner. However, the fallback abort can be cancelled by the expired graceful deadline and settlement is claimed early (`STD-R5-001`). The reported residual that in-process teardown normally takes the abort path is not itself a violation: Forge supervision is only marked drained by `BoundServer::run`, so an in-process harness must use the existing abort path; correctness requires awaiting that abort to completion. |
| `FIND-TASK-001-29` | **CLOSED.** The call span now records the effective callback-replaced model, with existing fallback and privacy controls retained. |

## Verification Notes

- Confirmed `HEAD` remained
  `09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596` before report creation.
- `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721 09e4b82c2a4cab3ea27e0889d3acf3ea22c2b596`
  exited zero.
- Ran the exact telemetry proof:
  `mise exec -- cargo nextest run --locked -p skald-agent --test agent_timeout -E 'test(=agent_run_chat_span_records_callback_replaced_model)'`;
  one test selected and passed.
- Reviewed the implementation record's green focused teardown tests and broader
  `fmt`, `lints`, `test:skald`, `test:wyrd`, `docs:check`, and
  `check:client-tier` results. Those tests do not hold a real governed storage
  request or retained loader pending across abort, so they cannot close
  `STD-R5-001`.
- The full cumulative base-to-candidate diff and final remediation delta were
  inspected by changed owner and applicable authority. No candidate source was
  modified by this review.
