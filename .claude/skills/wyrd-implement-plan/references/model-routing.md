# Model Routing and Escalation

Use this reference before every implementation and review dispatch and whenever
a task stalls, changes scope, or receives a material finding.

## Contents

- [Fixed model policy](#fixed-model-policy)
- [Risk tiers are not models](#risk-tiers-are-not-models)
- [The escalation ladder](#the-escalation-ladder)
- [Agent lifecycle](#agent-lifecycle)

## Fixed model policy

Model assignment is a property of the **role**, not the task:

| Role | `subagent_type` | Model | Why |
|---|---|---|---|
| Root orchestrator | — (main context) | Opus | Owns material decisions, plan repair, integration, and the review audit |
| Implementation | `wyrd-implementor` | Sonnet | Executes a decision-complete task packet; the packet supplies the judgment |
| Task and final review | `wyrd-reviewer` | Opus | Must independently reconstruct conformance from source, which is the hardest read in the loop |
| Material plan revision | `wyrd-plan-reviewer` | Opus | Gates architecture and executability of orchestrator-authored revisions |

There is no per-agent reasoning-effort control. Do not attempt to encode one.

Reviewers are never weaker than implementors. That asymmetry is deliberate: the
implementor works from an explicit contract, while the reviewer must derive
conformance from cumulative source with no contract to lean on.

## Risk tiers are not models

`Luna`, `Terra`, and `Sol` appear in the `Assigned model:` field of a task
packet. The field name is retained for schema compatibility with
`validate_plan_artifacts.py` and existing `.dev/plan/` artifacts. The value is a
**risk tier**.

A tier governs three things, none of which is model selection:

| Tier | Task scope | Review rigor | Escalation |
|---|---|---|---|
| Luna | Mechanical, one established repository pattern | Standard traceability + citation | Ladder from step 1 |
| Terra | Ordinary implementation and review work | Standard traceability + citation | Ladder from step 1 |
| Sol | Security, public/persisted contracts, migrations, concurrency, cross-owner or cross-language work | Additionally inspect tenancy, audit, cancellation, partial-progress, and recovery paths | Skip to step 3 on first substantive failure |

A Sol task is not handed to a stronger implementor. It is scoped tighter,
reviewed harder, and escalated sooner.

Assign Sol from the outset when any of these is material:

- public, wire, generated, or persisted contract design;
- migration, destructive state, compatibility, or recovery;
- authentication, authorization, tenancy, policy, audit, or secrets;
- concurrent ownership, cancellation, fencing, leases, or cross-store
  atomicity;
- several ownership boundaries or first-class language projections;
- a novel implementation seam without one clear current precedent.

## The escalation ladder

Because every implementor runs on the same model, escalation raises **scope and
authority**, not capability. Climb one rung at a time; never skip to the top to
save effort, and never abandon a task.

1. **Binding remediation, same agent.** `SendMessage` the implementor a
   specific correction. Its context is intact, so do not restate the task.
2. **Enriched context, same agent.** Supply the reviewer's findings verbatim,
   the failing diff, and a directed hypothesis about the cause. Most repeated
   failures are an under-informed agent, not an under-powered one.
3. **Fresh implementor, durable artifacts only.** Spawn a replacement seeded
   from the canonical task, current diff, findings, and evidence — never a
   conversational summary. This defeats context poisoning, which is the failure
   mode that model promotion used to mask.
4. **Controller takeover.** You implement the task yourself in the main
   context. This is the top of the ladder and the only remaining capability
   escalation. State explicitly in the task's evidence that the controller
   implemented it, so the reviewer knows the author changed.
5. **Task or plan revision.** The task itself is wrong. Revise it, obtain
   `ADVISORY_APPROVE` from `wyrd-plan-reviewer`, and restart the cycle.

### Repetition trigger

There is no retry limit, but there is a step-up requirement. Track remediation
attempts per task. After **three** remediations at the same rung without a
substantiated `APPROVE`, advance to the next rung. Repeating a rung is how the
loop spins forever now that there is no model axis to escalate.

Escalate immediately, without exhausting the count, when:

- the implementor repeats the same failed diagnosis;
- a `BLOCKED` result plus your own inspection confirms non-local complexity;
- review finds a missed owner, consumer, or material invariant;
- task scope grows across another ownership or contract boundary;
- integrated verification reopens an accepted task.

## Agent lifecycle

- Spawn a fresh implementor for each new task with `run_in_background: true`
  and record its `agentId` from the spawn result.
- Keep that implementor through ordinary implementation and remediation by
  resuming it with `SendMessage({to: agentId, ...})`. Context survives; a new
  `Agent` call does not preserve it and starts fresh.
- Spawn a fresh independent reviewer for the first review of a task.
- Keep that reviewer for focused re-review of its own findings.
- A nonterminal progress report is not grounds for replacement. Neither is
  automatic context compaction nor a self-reported turn limit — those are agent
  assertions, not platform facts. Replace only at ladder step 3, or when the
  agent is genuinely unreachable.
- Replacements receive durable artifacts and evidence, not a conversational
  summary.
- Across a session boundary every prior agent is gone. Do not reattach a stale
  `agentId`; respawn from durable artifacts. See the resume section of
  `SKILL.md`.
