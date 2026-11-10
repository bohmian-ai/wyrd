---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-software-factory
spec_revision: 2
spec_status: approved
requirements: [REQ-010, REQ-011, INV-006, AC-009, AC-010]
depends_on: [TASK-003, TASK-004]
---

# Oversee packet execution from Slack

## Outcome and Value

The maintainer can leave the browser, receive a packet's milestones and
questions in Slack, answer them, provide feedback, and pause/resume execution.
The same decision answered through either interface applies once. Local use
continues when Slack is disconnected or unconfigured.

## What Changes and What Must Stay True

Use Bolt for JavaScript with Socket Mode and one configured workspace, user,
and channel. Slack tokens stay outside tracked files and persisted transcripts.
One packet has one Slack thread. Automatic notifications include stage/status,
safe task identifiers, and the question needed for a decision, not source or
full transcripts. Only explicitly selected feedback can become remembered
preferences; Slack feedback defaults to its task like browser feedback.

Slack calls the existing control/message/decision actions. It is not a second
scheduler, authorization model, or shell interface. Slack receipt acknowledgment
is distinct from an action's success. Do not expose the local service publicly.

## Approach

1. Add optional local Slack configuration and connection/health state.
2. Deliver ordered milestone/decision notifications to packet threads.
3. Route authorized controls, answers, and scoped feedback to existing actions.
4. Deduplicate delivery and reject stale/foreign/resolved decisions.
5. Preserve local operation during outages and document installation/recovery.

## Ordered Implementation Scenarios

### Scenario 1 — Notify and control one packet through its thread

**Behavior.** A packet emits stage milestones and a pending decision to its
thread. The configured maintainer can answer, provide feedback, and pause or
resume through Slack. The browser immediately reflects the same stored state.
Feedback is saved before native delivery and defaults to task scope. The
maintainer can explicitly choose repository/global scope using the existing
preference actions. No message text is interpreted as a shell command.

**RED.** Add the planned Slack/service integration test using the real service,
SQLite, and a controlled transport boundary. Assert thread association, existing
action invocation, persisted feedback, and browser-visible state. Include text
with shell metacharacters and assert it remains literal feedback.

**GREEN.** Implement the Socket Mode boundary and adapt payloads to existing
service operations and notification events.

**REFACTOR.** Reuse controls, decisions, and preference scope from TASK-003/004;
avoid an LLM router or general chat-command execution feature.

### Scenario 2 — Apply a decision once and only for its current subject

**Behavior.** Duplicate Slack delivery and simultaneous UI/Slack answers apply
once. Reject a foreign user/workspace/channel, a resolved/expired decision, or
a decision belonging to an obsolete candidate/spec revision. Slack connection
access does not grant authority to other workspace members. Credential-shaped
payload/error content is redacted before storage/streaming.

**RED.** Add the planned authorization/replay test with duplicate deliveries
and a deterministic simultaneous-answer barrier. Assert one transition, one
recorded resolution, no foreign mutation, and no configured secret in artifacts.

**GREEN.** Bind inbound events to configured identity, current pending decisions,
and the existing atomic action boundary.

**REFACTOR.** Use Slack's authenticated Socket Mode transport and event identity
with existing SQLite action state; no public webhook or separate approvals DB.

### Scenario 3 — Keep local execution usable during Slack failure

**Behavior.** With no Slack configuration, the factory runs normally. Disconnect,
rate limit, failed send, or restart shows connection/delivery state and queues
unsent milestones. The browser can answer all decisions while disconnected.
Reconnect does not reopen a resolved decision or repeat controls. The service
does not imply availability while the host sleeps or is offline.

**RED.** Add the planned outage test with controlled disconnect/reconnect and
rate-limit responses. Resolve a queued decision in the browser, reconnect, and
assert the notification reflects current state and cannot resurrect the action.

**GREEN.** Persist pending deliveries and use bounded SDK retry behavior,
separating transport receipt, delivery status, and completed application actions.

**REFACTOR.** Keep optional Slack delivery inside the service and its existing
event history; no external broker or background cloud relay.

## Acceptance Criteria

| Obligations | Observation and proof |
|---|---|
| REQ-010, AC-009 | Thread/control tests prove milestones, questions, literal feedback, explicit memory scope, and shared UI/Slack actions. |
| REQ-010, INV-006 | Replay/identity cases apply one answer, reject obsolete subjects and foreign actors, and never execute incoming text. |
| REQ-011, AC-010 (Slack boundary) | Persisted events identify the configured actor while redaction and notification selection exclude credentials/source. |
| REQ-010 (availability) | Outage cases preserve local use, honest pending delivery, and current decision state after reconnect. |

## Implementation Reference

TASK-003 owns durable messages, controls, decisions, and feedback delivery;
TASK-004 owns explicit memory scope. Their actual service actions/callers/tests
must be inspected before adding this transport. TASK-001's local-access and
redaction boundary is reused. No Slack Bolt/Socket Mode owner was found in the
inspected repository tooling. A small native SDK boundary is needed because
neither browser events nor coding-agent sessions receive Slack events.

Use the official Slack Socket Mode and acknowledgment contracts cited in the
spec. Existing Wyrd Operator delivery is a server-owned product workflow with
different credentials/tenancy/storage; do not introduce that dependency into
this standalone contributor application. Likely changes remain in factory
notifications/actions/UI health, local configuration documentation, and tests.

## Verification and Evidence

All selectors and smoke commands are **planned**. TASK-001 supplies the test
runner; confirm selectors before RED. Offline fixtures pin the Slack boundary,
not native coding-agent behavior or actual Slack delivery.

| Proof | Exact focused command |
|---|---|
| Thread/actions | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/slack.test.ts -t '^shares packet controls decisions and literal feedback with the browser$'` |
| Identity/replay | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/slack.test.ts -t '^rejects foreign stale and replayed Slack actions$'` |
| Outage | `mise exec -- pnpm --dir tools/wyrd-factory exec vitest run tests/integration/slack.test.ts -t '^preserves local control and current decisions through Slack outage$'` |
| Real Slack boundary | `mise run factory:smoke:slack -- --channel <channel-id>` |

Implement the explicit smoke task and document local app setup, required scopes,
configured identity, token supply, and cleanup. Run the real Slack journey only
when explicitly authorized to send messages to the supplied channel. Record
delivery, an actual decision/control, and browser agreement; missing access is
unverified, not PASS. Run focused proofs, factory:format:check, factory:lints,
factory:typecheck, factory:build, and git diff --check. The full offline factory
suite and integrated packet/native journeys run once at final change review.

## Material Stop Conditions

Stop if the design requires public service exposure, additional privileged
identities, autonomous approval inference, arbitrary shell commands, or sending
messages without explicit live-smoke authorization. UI text and ordinary Bolt
wiring are local implementation choices.

## Authority Links

- [Specification](../spec.md)
- [Control dependency](TASK-003-control-and-recovery.md)
- [Memory dependency](TASK-004-shared-personal-memory.md)
- [Slack Socket Mode](https://docs.slack.dev/apis/events-api/using-socket-mode/)
- [Slack acknowledgment](https://docs.slack.dev/tools/bolt-js/concepts/acknowledge/)
