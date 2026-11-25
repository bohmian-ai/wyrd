# R2 independent cancellation boundary review

Reviewer: fresh `r2_domain_rev`. Candidate:
`7cf5bc4abb1462e5783df67ff6d0a2a5f2a06b14`. Baseline:
`41f2419c8` (the previously reviewed R1 tree committed before R2); the
cumulative `79a4ef661..7cf5bc4ab` range and original `7e2b3f6fa` remain
available. HEAD matched the candidate during inspection. This reviewer changed
only this report and ran no build, test, package, lint or codegen command.

## Authority and navigation

Read the task-review skill, applicable TS SDK AGENTS, root rules and Rust
rustdoc guide, prior validated FIND-TASK-004B-4, R2 task/evidence, and the
client/server cancellation owners. CodeGraph preceded direct source tracing.
The bounded scope is the materially changed public TS/native cancellation
boundary and its actual UI consumer, not a reopened analytical-query audit.

Flow: page AbortController -> host-private support fetch ->
`untilDeparted` socket/request signal -> shared loader context -> Node executor
-> TS `abortable` -> native `NativeAbort`/`AbortWatch` -> existing shared
Rust client operation -> in-flight HTTP future or retained response stream.

## Coverage and source assessment

| Boundary | Source/assertion evidence | Assessment |
|---|---|---|
| Pending SQL startup receives cancellation | `node.ts:63–64` passes signal into `Bifrost.stream`; TS `index.ts:488–510,1437–1470` installs the listener before starting native query; `NativeBifrost::query` watches cancellation around the existing query future | Correct client-owned cancellation path; missing R1 startup phase addressed |
| Pending Card lookup receives cancellation | Node `cards.list` branch at `node.ts:56–59` passes signal; TS `Cards.list` uses `abortable`; native `cards.rs:389–411` selects abort against existing `cards.list` future | Correct client-owned pending lookup cancellation path; missing R1 Card phase addressed |
| Pre-aborted signal and abort/completion ownership | TS throws before native IO; listener removed in finally; watch retains explicit abort state; biased native select resolves abort as no answer, with the existing Rust future dropped. A completed answer belongs to its caller | No concrete ownership/race defect established. The supported signal is synchronous JS AbortSignal; there is no await between initial check and listener installation |
| Running stream remains cancellable | Node attaches stream.return after startup, handles an already-aborted signal at handoff and removes listener in finally. Existing native stream close wakes collection and drops its retained stream | Existing post-header behavior preserved |
| Error projection | Successful native answer is projected as before; native structured errors remain catalog-backed. Explicit local cancellation throws the signal reason and Node rechecks its signal before attempting problem projection | No invented server error code or partial success path introduced |
| N-API ownership and layering | Native calls clone Rust handles and transfer only watch receivers/futures into spawned work; no V8 handle held over await. Shared client still owns auth, connection pools and HTTP. No second transport, auth path or runtime | Fits existing SDK boundary |
| Generated projections | Generated `.d.ts` and `.d.cts` each add NativeAbort plus optional abort arguments and optional cancelled-native outcome. Handwritten public wrappers expose AbortSignal options; existing callers can omit them | Inspected projections match source intent. Reported ts:napi:check/typecheck passed; generator parity was not independently executed |

`NativeAbort` adds a demonstrated shared owner for native cancellation of two
real client operations. Its watch state and private receiver reuse Tokio's
installed primitive and avoid a second auth/transport abstraction. This review
does not propose alternative machinery merely because another shape is possible.

## Executable proof inspected

The SDK's new `tests/unit/abort-pending.test.ts:47–81` uses the public built SDK
against a loopback HTTP endpoint that never supplies a response. For SQL and
Card listing it waits for the request, aborts, asserts rejection with the
signal's reason, and awaits the actual socket close. A separate case proves
an already-aborted Card signal sends nothing. These are binding/transport tests,
not mocks of the new native function. Mutation testing is reported in the task.

The production-BFF browser gate now supports holding a response before headers
and includes `GET /v1/cards`. Observe assertions at
`ui-pages.integration.test.ts:1095–1109` observe all held SQL answers abandoned
on supersession and departure. Verify at `:2133–2151` observes both `cards`
and `sql` requests abandoned for both transitions. The existing body-hold cases
still cover running-stream cancellation. The task records the six focused
browser journeys and SDK unit/check executions against the R2 source.

These tests establish that the client-owned pending requests actually close,
rather than waiting to their deadline or merely ignoring a late result. They
do not establish when the real server releases CPU, planning state or every
downstream task: the gate holds response delivery, not an unfinished server
calculation. The implementer explicitly withdrew a vacuous running-query check.
Do not claim server computation termination from this evidence.

That limitation does not reopen the original defect by itself. The shared Rust
client's existing query cancellation contract is request-future abandonment
before the response and response-body drop after it (`bifrost/query.rs:187–217`).
R2 now connects the missing UI phases to that same established transport
ownership contract. There is no concrete new server lifecycle defect here and
no authority to invent a new server cancellation API. FIND-TASK-004B-4 has
credible closure at the required client/executor/transport boundary, within
the supplied runtime evidence. Final acceptance still depends on the primary
reviewer's complete matrix and explicit visual rulings.

## Proposed C-01 — VIOLATION: new Rust cancellation owner misses mandatory rustdoc

This proposal requires separate validation before retention as a stable finding.

- Authority: root AGENTS Rust documentation gate at lines 152–154, and
  `agent_docs/rust.md:235–241`, explicitly require rustdoc for every new or
  materially modified item regardless of visibility, including fields.
- Location: `sdks/wyrd-sdk-ts/native/src/lib.rs:324` (`NativeAbort.aborted`)
  and `:346` (`Default::default` for the new owner).
- Starting condition/trigger: the candidate adds a native cancellation owner;
  those new items are reviewed for the mandatory changed-item documentation
  gate. The surrounding struct/method docs do not document these declarations.
- Expected/actual: the field should describe the shared retained explicit-abort
  state and its role; default should document creation of a not-yet-aborted
  owner. Both currently have no rustdoc. No trait-implementation exemption was
  found in the applicable rules.
- Materiality: this is an explicit merge gate, not a preference for additional
  commentary or a claim of runtime failure.
- Smallest correction: add the two accurate rustdoc comments at the existing
  owner, preserving every behavior and generated API signature. No new helper,
  test, production rewrite or historical RED is required for documentation.

No other new blocking finding is proposed in this bounded review.

## Added/moved waits and performance

| Change | PostgreSQL / Scribe / object-store / Vault waits |
|---|---|
| TS AbortSignal wrapper | None; synchronous signal setup and listener cleanup around existing native promises |
| NativeAbort/AbortWatch | None; process-local watch cancellation drops an existing Rust client future |
| Native query/Card wrappers | None added or moved; existing shared-client HTTP requests are unchanged and cancellable |
| UI executor forwarding | None; forwards the existing context signal to those two SDK operations |

No server production hot path changes in this diff, no new PostgreSQL write,
and no new audit-acknowledgement or storage/Vault wait. The earlier hosted L1
exception remains a recorded limit, not a benchmark performed here or a waiver
for unrelated future work.

## TDD and verification limits

R2's SDK unit test was written after native/TS signal forwarding. Reported
mutation failure strengthens its assertions but does not establish test-first
history; preserve the violation separately from behavioral acceptance. Original
and R1 violations remain historical facts.

This is source/assertion review of supplied candidate-bound executions. No
runtime tests or generated checks were independently rerun. No server-side
computation-release claim, visual approval, implementation change or commit is
made by this reviewer.
