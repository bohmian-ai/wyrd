# Maintainer review

Result: **FAIL** — one bounded documentation contract gap; no runtime redesign proposed.

## Subject and method

- Candidate: `8fa32190b26b11588721e8db44b3eb94139e3739`.
- Cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Latest correction locator: `8376bb2aa..8fa32190b`.
- Authority: approved spec revision 20, original TASK-003a, amended R2, AGENTS.md, agent-rules.md, maintainer-style.md, spec-driven-development.md, and the relevant grouped-verification/authority sections of wyrd-design.md and Bifrost durability/visibility design.
- Reviewed actual cumulative diff and expanded its owner/caller map using CodeGraph before source navigation. Source remained immutable. No PostgreSQL, Cargo, or test lane was started by this reviewer.

## Changed-surface coverage

| Surface | Connected owners/callers inspected | Maintainer assessment |
|---|---|---|
| Trigger duration and source-slot contracts | TriggerDuration parse/serde/schema, InputSlot and TypedInputSlot decoder, InlineableRef decoder, registration BindingProjector | One pure contract owner; tagged decoder reuses remote derive rather than a second validator. Current visitor documentation correctly separates verbatim source text from later grammar validation. |
| Group snapshot preparation | GroupPoller, Reader, Sources, Visible, Earliest, Payloads; queue PinnedGroup and source provenance | State belongs to concrete owners. Private source-selection helpers stay beside their caller. Incomplete detection happens before pinning and hashing, rather than adding a second execution lifecycle. |
| Scheduling and terminal refusal | VerificationScheduler group/cron loops, hand_off_groups/hand_off; BackgroundAuthority; due claim/activity/consume/refuse SQL | Existing scheduler and queue remain the workflow owners. Bounded passes, permit lifetime, transaction commit boundary and retry versus terminal outcomes are discoverable in one place. Accepted loss/overlap tradeoffs do not justify new machinery. |
| Internal query authority | GroupPoller call to SystemReadAuthority::resolve/serving, scheduled query caller, owner recovery | Reuses the existing scheduled-Drift table-scoped authority owner; does not introduce a parallel group token/permission subsystem. |
| ACK-to-scheduling plumbing | ScribeWrite::group_activity, ScribeSink::schedule_groups/write, boot composition and local outbox callers | Reuses existing Scribe outbox and batched queue owner. Fallible activity recording precedes submission-reservation consumption. Optional Postgres handle lets unrelated recording fixtures opt out without a second writer. |
| Capacity, runner, and Operators | ClaimLoop, runtime builder, VerifierRunner, OperatorWorker, settlement comparison and dispatch fields | Existing claim loop gains shared semaphore ownership; no new queue/configuration/trait. Group ceiling and legacy coverage consumers are removed through runner, settings and schema projections. |
| Persistent shapes and R2 correction | Original execution/binding migrations, pending migration, new refused-shape migration, sole refused INSERT and ordinary writers/status decoder | New migration tightens exactly the authority-free exception, avoiding a changed applied checksum. Ordinary authority-present shapes remain outside this correction. |
| R2 proof readability | Existing P2 activity/snapshot/settlement test, session DST construction, six refusal mutation cases | POSIX zone fixture uses the real activity query; its own calendar-day assertion proves the intended DST premise. Exact equality of day/hour rows plus their 23-hour difference from the one-hour binding proves 86,400 seconds. Savepoints isolate rejected mutation attempts. No new harness or production clock API. |
| Runtime integration proof | pg_verification_groups Support and scheduler tests, incomplete_snapshots_retry_then_error; fault seam and test server helpers | Existing server harness proves durable pending/error readback and the shared scheduler boundary. Existing port reserver replaces CLI-specific Linux-only probing. |
| First-class SDK/MCP/HTTP consumers | Rust/Python/TS grouped journeys, mixed-runtime turn helper, OpenAPI shape checks, MCP grouping changes, Python authored stub and generated TS declarations | Source snapshots expose only sources; tests consume ordinary execution state. Renamed fixtures and source references remain synchronized. Invalid fixtures are tested as invalid and use the actual Verifier kind. |
| Current docs/fixtures | Grouped architecture prose, how-verification-works, runtime limits, generated contract diff and renamed/rewritten YAML fixtures | Legacy idle/max-age/finality/ceiling descriptions are removed, but revision 20 is not reflected in the durable prose described below. |

## Existing workflow comparison

The baseline already had VerificationScheduler, GroupPoller, VerifierExecutionQueue, SystemReadAuthority for scheduled Drift, ScribeOutbox, ClaimLoop, and the shared port reserver. The cumulative candidate extends those owners: pending rows replace repeated Bifrost discovery; snapshots reuse the ordinary execution queue; internal reads reuse the existing authority; ACK notifications reuse the existing writer; the shared capacity remains the existing execution semaphore; and CLI tests reuse the existing cross-process port reservation. No competing graph store, transport, lifecycle service, retry queue, generic interface, or credential cache was found in these changes. The temporary source-decoding and snapshot-selection values earn their local invariants and do not retain IO resources independently.

## Prior correction closure

| Prior finding | Current source evidence | Assessment |
|---|---|---|
| FIND-TASK-003a-2 | RECORD_GROUP_ACTIVITY_SQL adds extracted epoch multiplied by a one-second interval; P2 drives the production query under a DST-crossing session | Closed in source; the dynamic-date fixture proves the actual operation and is a credible substitute for the literal March date. |
| FIND-TASK-003a-6 | InputSlot Slot visitor documents Value and expecting/visit_str/visit_map with accurate Errors sections | Closed; no behavior or extra parser was introduced for documentation. |
| FIND-TASK-003a-7 | 20261019000000_verification_refused_shape.sql requires all input/window/record/snapshot columns NULL in the authority-free branch; P2 rejects the six mutations | Closed in source. |
| FIND-TASK-003a-1 / 8 | GroupPollError maps all query failures to retryable Read; existing SystemReadAuthority is used for snapshot preparation | No obsolete owner-query denial requirement restored; approved revision 19 supersedes that old expectation. |

## Proposed finding

### MREV-R2-01 — Durable grouped-verification prose still promises the pre-revision-20 behavior

- Classification: **REGRESSION / public-contract documentation mismatch**.
- Obligation: original TASK-003a A9 (`tasks/TASK-003a-simplify-grouped-verification.md:483`) requires current docs to agree with the implemented contract; the approved spec's closure section (`spec.md:1036–1046`) includes documentation and explicitly carries revision 20's incomplete-snapshot retry/deadline.
- Exact locations: `architecture/wyrd-design.md:1324` says source lists contain null for missing inputs/outcomes; `:1339–1343` describes unrestricted visible evidence without the new exception. The user guide `docs/src/content/docs/understand/how-verification-works.md:176–180` likewise says the due execution judges the visible records and describes terminal errors only for owner refusals.
- Source trace: selected Bifrost result-only execution → `Visible::missing_inputs` (`groups.rs:439–448`) → `GroupPoller::poll` (`:156–160`) returns Incomplete before pinning → `VerificationScheduler::hand_off` (`scheduler.rs:488–507`) retains pending before the PostgreSQL five-minute threshold or writes `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` after it. `incomplete_snapshots_retry_then_error` exercises that terminal path/readback. It no longer produces a pinned list entry whose entire input is missing.
- Observable consequence: a caller or maintainer following the current durable guide expects a judgment over available rows, or continued transient retries, where the shipped implementation instead eventually consumes the group with an execution error and requires new source activity to schedule another attempt. The architecture also tells a maintainer that a snapshot shape now explicitly forbidden by revision 20 is allowed. This is the changed failure contract, not a wording preference.
- Smallest correction: update only the existing grouped-verification paragraphs in the architecture and how-verification-works guide to state the result-without-input exception, retry while no more than five minutes past due, then one terminal errored execution with the named code and pending consumption. Preserve normal input-without-result nulls, missing individual selected values, new-activity scheduling and all accepted ACK/loss/overlap tradeoffs. No new runtime guard, configuration, machinery, test harness or product decision.
- Focused closure proof: read those paragraphs against the existing source and revision 20, then run the existing docs check as part of the bounded documentation correction. Existing runtime proof need not be rerun for prose-only edits.

## Verification and limits

Root verification.md records P2 1/1, P1 2/2, migration tests, fmt, lints, tenant-isolation and diff whitespace passing on this candidate. Source inspection supports the narrow R2 fixes and the DST fixture substitution. These passing runtime checks do not prove that current documentation describes the new failure case.

No fresh full SDK/MCP journeys, benchmark qualification, PostgreSQL proof or build lane was run by this reviewer. No runtime defect or additional machinery is proposed; the remaining proposed correction is the current documentation contract.
