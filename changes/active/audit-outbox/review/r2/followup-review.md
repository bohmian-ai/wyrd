# Focused follow-up review — shutdown admission, terminal pending, and dependency ownership

## Immutable subject and scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd/.claude/worktrees/agent-aad682fbca5074900`
- Base: `6714ae35d732814240fdcc42fc226c079b14d3f0`
- Candidate: `5a5542cbb965af99e92a3983a2ddf586412cea73`
- Approved authority: `changes/active/audit-outbox/spec.md`, revision 2
- Original task relevant here: `changes/active/audit-outbox/tasks/02-one-outbox.md`
- Remediation task: `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md`

`HEAD` matched the assigned candidate during this pass. The repository has no
`.codegraph/` directory. This follow-up investigated only the three assigned
uncertainties and did not revisit the other discovery findings.

## Source paths inspected

- Contract and workflow authority:
  - `AGENTS.md`, especially §§2, 3, 5, 6, 11, 12, 15, and 16
  - `architecture/agent-rules.md`
  - `architecture/references/languages/spec-driven-development.md`
  - `architecture/references/languages/maintainer-style.md`
  - `architecture/references/architecture/patterns.md:23-38,243-273`
  - `architecture/bifrost-design.md:845-890`
  - `architecture/wyrd-design.md:480-497`
  - `changes/active/audit-outbox/spec.md:40-48,84-147,166-221,232-241`
  - `changes/active/audit-outbox/tasks/02-one-outbox.md`
  - `changes/active/audit-outbox/review/r1/TASK-AUDIT-OUTBOX-R1-remediation.md:34-136`
  - `changes/active/verified-change-contract/spec.md:774-785,1488-1492,1988-1997`
- Generic outbox and tests:
  - `crates/shared/wyrd-runtime/src/outbox.rs:1-405,407-606`
  - `crates/shared/wyrd-runtime/src/lib.rs`
  - `crates/shared/wyrd-runtime/Cargo.toml`
- Current server composition and next named consumer:
  - `crates/wyrd/wyrd-server/src/app/server.rs:484-650,760-888`
  - `crates/wyrd/wyrd-server/src/app/supervise.rs:142-200`
  - `crates/wyrd/wyrd-server/src/boot/mod.rs` audit-outbox composition sites
  - `crates/wyrd/wyrd-server/src/state.rs` audit-outbox ownership sites
  - `crates/wyrd/wyrd-server/src/verification/observations.rs:1-165`
- Dependency ownership and reverse cones:
  - workspace `Cargo.toml` and candidate `Cargo.lock`
  - `scripts/checks/client-tier.sh`
  - manifests for `wyrd-runtime`, `wyrd-client`, `wyrd-auth-verify`,
    `wyrd-auth-issue`, `wyrd-queue`, `vala-sql`, `wyrd-server`, `skald-agent`,
    `skald-tool`, `skald-workflow`, `wyrd-sdk-python`, and
    `wyrd-sdk-ts-testing`
  - locked reverse trees for `metrics`, `tokio-util`, and `tracing` from
    `wyrd-client`, `wyrd-auth-verify`, and all-feature `skald-agent`, plus the
    workspace reverse tree of `wyrd-runtime`
- Conflicting discovery claims:
  - `review/r2/domain-review-concurrency-lifecycle.md`
  - `review/r2/maintainer-review.md`
  - `review/r2/standards-review.md`
  - `review/r2/task-review-behavior.md`
  - `review/r2/task-review-invariants.md`
  - `review/r2/system-review.md`
  - `review/r2/domain-review-durability-persistent-data.md`
  - `review/r2/domain-review-security-tenancy.md`

## 1. Shutdown admission (`CONC-R2-003` / `MAINT-R2-03`)

### Current deployed audit path

The behavior and system reviewers are correct about the current production
server composition. `BoundServer::run` first cancels serving supervision and
joins or aborts its retained tasks, then closes and waits for MCP and gateway
task trackers, then drains or aborts Bifrost. Only after those producer-owning
boundaries have finished does it call
`self.state.audit_outbox.shutdown(deadline)`
(`app/server.rs:784-878`). No current production audit producer was found that
is intentionally allowed to remain active after that call. Consequently, the
specific production claim that a normal Wyrd request can stage after the audit
outbox shutdown invocation was not established. REQ-007/AC-007's deployed
audit drain path is therefore satisfied by composition for this interleaving.

### Generic owner contract and reachable path

That system-level sequencing does not close the generic contract. `Outbox` is
a public shared owner returned behind `Arc`; any holder can call `stage` while
another holder awaits `shutdown`. `shutdown` only cancels `self.stop`
(`outbox.rs:180-181`). `stage` has no admission state and succeeds until the
background writer eventually executes `requests.close()`
(`outbox.rs:139-148,259-262`). The writer uses a biased select whose write
completion and `recv_many` arms precede the stop arm
(`outbox.rs:244-262`). A direct, deterministic caller race is therefore
reachable without relying on a hypothetical server route: start shutdown while
a sink write is blocked, then call `stage` before the writer observes `stop`.
That item is accepted and increments pending even though shutdown has begun.

Revision-2 REQ-007 explicitly says graceful shutdown stops accepting new
events. REQ-008 assigns graceful-shutdown flush and loss counting to the
generic type, and AC-008 requires focused tests of that generic owner. The
module's own public contract likewise says staging after shutdown is lost and
counted (`outbox.rs:17-22,136-145`). The existing shutdown test stages only
before shutdown and after `shutdown(...).await` has returned
(`outbox.rs:559-581`); it does not cover the invocation-to-receiver-close
window. Thus current server quiescence is an important mitigation, not an
implementation of the generic owner's promised admission transition.

The named Eval context does not need to be used speculatively to reach this
conclusion. The currently implemented verified-change-contract path still
uses a bounded `TaskTracker` and explicitly says to add no outbox in that
change (`verified-change-contract/spec.md:774-785`; `verification/observations.rs`).
Separately, the approved audit-outbox revision 2 explicitly makes Eval run
requests the second consumer of this generic type. That future composition
reinforces why the contract is generic, but its producer topology is not
needed as evidence for the present race.

### Resolution

The conflict is resolved by narrowing the discovery claim:

- Reject the assertion that the current server's ordinary audit producer path
  remains live after `audit_outbox.shutdown` begins; source shows it is drained
  first.
- Retain the admission-race claim as a generic-owner defect under REQ-007's
  stated stop-accepting behavior together with REQ-008 and AC-008. AC-007's
  presently composed audit scenario is not independently falsified by this
  race.
- The smallest required outcome is a one-way admission boundary owned by
  `Outbox` itself, with a focused concurrent shutdown/stage proof. This report
  does not prescribe the private synchronization mechanism.

**Uncertainty 1: RESOLVED.**

## 2. Deadline-abandoned items and pending state (`MAINT-R2-02`)

`pending` is documented as items “not yet written or lost”
(`outbox.rs:75-82`), and every successful or panic-classified terminal release
decrements both the atomic and `outbox_pending` gauge through `release`
(`outbox.rs:321-383`). Deadline abandonment differs: `abandon_remaining`
loads the atomic, increments `outbox_events_lost_total`, and logs, but neither
decrements the gauge nor clears the atomic (`outbox.rs:385-399`). The writer
then returns and drops its queued and in-flight ownership. `shutdown` waits for
that termination and returns `self.pending()` (`outbox.rs:180-190`). After that
point, the returned number is a useful loss count, but the same number left in
`pending()` and `outbox_pending` no longer represents live or retryable work.

REQ-003a and REQ-007 are met narrowly for counting and reporting the deadline
remainder: the counter is incremented and `shutdown` returns the remainder.
They do not authorize representing already abandoned work as still pending.
REQ-008 separately defines the pending count and gauge and AC-008 expressly
requires `pending` to return to zero. The broader telemetry authority also
requires every active gauge to decrement on cancellation and failure
(`architecture/bifrost-design.md:887-890`). The deadline test asserts only the
returned value and never checks `pending()` or the gauge after writer exit
(`outbox.rs:583-605`).

The maintenance claim is therefore source-supported, with its obligation
stated precisely: this is not a failure to count loss under REQ-003a/REQ-007;
it is a failure to terminally settle the generic pending state under REQ-008,
AC-008, and the active-gauge authority. Any correction must preserve the exact
loss result separately while bringing the atomic and gauge to zero after the
writer abandons ownership.

**Uncertainty 2: RESOLVED.**

## 3. Dependency ownership (`STD-R2-003`)

The approved behavior requires one generic SQL-free outbox in a shared crate;
it does not require placing the implementation in the already broad
`wyrd-runtime` package. The remediation task proposed
`crates/shared/wyrd-runtime/src/outbox.rs` and supplied that shape, but the
approved specification's expensive-to-reverse decision is only “the generic
outbox type and its sink trait in a shared crate.” A task-level proposed path
does not erase the higher repository rule that ownership includes dependency
cost and specialized behavior stays in the narrowest owner.

The candidate makes `metrics`, `tokio-util`, and `tracing` unconditional normal
dependencies of `wyrd-runtime` and unconditionally exports `outbox`. The lock
diff shows these three edges added to the `wyrd-runtime` package. The workspace
reverse tree shows that package already feeds server crates but also
`wyrd-client`, `wyrd-auth-verify`, `wyrd-auth-issue`, `wyrd-queue`, the Rust,
Python, and TypeScript client/package paths, and optional all-feature Skald
bindings. Only `vala-sql` currently uses `Outbox`; the approved Eval sink is
also server-side. No client or Skald consumer inspected uses the outbox.

The original standards wording should be made more exact about incremental
package cost:

- `metrics` reaches `wyrd-client`, `wyrd-auth-verify`, and all-feature
  `skald-agent` solely through `wyrd-runtime` in the locked reverse trees. This
  is a clear new outbox-only edge in those selected cones.
- `tokio-util` and `tracing` are unconditional edges of `wyrd-runtime`, but
  several cited consumers already receive them through direct dependencies or
  HTTP/provider stacks. Their addition to the runtime manifest still couples
  every minimal runtime consumer to outbox implementation needs, but they are
  not always newly present packages in the final resolved graph.
- `scripts/checks/client-tier.sh` remains green because its denylist targets
  server/database/engine dependencies; it does not enforce the broader
  dependency-cost rule. A green boundary script therefore neither proves nor
  disproves this ownership concern.
- All three crates were already workspace-pinned, so this is not a new registry
  source or dependency-version/supply-chain finding.

The behavior still needs a shared SQL-free implementation, so deletion is not
an answer. The source-supported issue is the chosen owner: a narrow shared
owner used by the actual Audit and Eval sinks can satisfy REQ-008 without
placing outbox metrics and task-lifecycle machinery in every consumer of the
runtime/auth/client shell. Whether that is a dedicated shared crate or another
already narrow owner is an implementation choice; creating a new broad Cargo
feature is not implied and would itself have to satisfy the repository's
feature-cost rule.

Accordingly, `STD-R2-003` remains supported after revision for precision: its
strongest demonstrated consequence is the unconditional new `metrics` edge
through broad client/auth/Skald cones, not that every cited cone gains all
three packages for the first time.

**Uncertainty 3: RESOLVED.**

## New proposed findings

None. This pass narrows and resolves the three supplied discovery claims; it
does not introduce a fourth finding.

## Overall follow-up status

**RESOLVED**
