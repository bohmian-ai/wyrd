# Durable Review Format

Write one artifact for every filesystem-backed review:

```text
.dev/review/{REVIEW_ID}/review.md
```

Use `{7-lowercase-hex}-{YYYYMMDD-HHMMSS}-{short-target}-{gate}` where `gate` is
`spec-gate`, `plan-gate`, or `fullsweep`. A re-review gets a new ID and cites
the prior ID.

## Header

Include:

- review ID;
- target and gate;
- overall verdict;
- architecture axis: `PASS`, `FAIL`, or `Not applicable`;
- executability axis: `PASS`, `FAIL`, or `Not applicable`;
- Critical and Major counts;
- prior review ID when applicable.

## Evidence and scope

Record:

- files and explicit dependencies in scope
- goal, workflow, declared scale/readiness, affected owners and public surfaces
- changed durable contracts, data paths, migrations, and safety boundaries
- authority and routed references used
- explicit out-of-scope items

Keep this concise and factual. Do not manufacture a full compliance matrix or
specialist inventory for a normal gate.

## Verdict and findings

State `Approve`, `Revise`, or `Stop/rethink`. Order findings by severity.

Each finding contains:

- **Location:** the affected plan or spec section.
- **Issue:** the unresolved or incorrect material decision.
- **Impact:** the concrete contract, safety, workflow, operational, or rework
  consequence.
- **Evidence:** repository authority and source evidence.
- **Required edit:** one specific plan change that closes the finding.

Consolidate consequences sharing a root cause and required decision.

## Architecture and execution readiness

For a plan gate, record the independent evidence for both axes:

- architecture decisions, contracts, ownership, safety, and workflow;
- change-impact closure, task compilation, command/setup validity, and cold
  rehearsal.

Overall approval requires both axes to pass.

## Traceability and executability gaps

For a plan gate, summarize missing or confirmed mappings among:

- requirements;
- executable tasks and assigned models;
- acceptance criteria, required tests, and focused commands;
- integrated journeys and closeout gates.
- impact-graph nodes and affected consumers;
- repository-managed setup and rehearsal evidence.

Do not duplicate a blocking traceability gap both here and as several
findings. The finding is authoritative; this section shows the affected
mapping.

## Implementation handoff notes

Include at most three explicitly non-gating notes likely to prevent material
rework. Omit the section when there are none.

Record open authority conflicts, targeted verification for novel or risky
decisions, and re-review closure status when applicable.

## Full Sweep

Only an explicitly requested full sweep adds specialist-lens evidence,
cross-file contradiction analysis, merged/eliminated candidate findings, and a
targeted rule-to-evidence matrix. The same Critical/Major threshold still
applies.

Before returning, verify `review.md` exists and its verdict and counts match
the chat handoff.
