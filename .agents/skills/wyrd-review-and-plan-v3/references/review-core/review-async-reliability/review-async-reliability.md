# Async and Operational Reliability Reviewer

Read `../specialist-contract.md` and write the assigned durable specialist
report using its exact sections and namespaced candidate IDs.

Review concurrency, background work, external calls, and lifecycle behavior.


## Repository authority

When `AGENTS.md` exists, read it completely and follow every required architecture, doctrine, agent-rule, and verification file it names. In Wyrd, explicitly read:

- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`

Use the active repository files rather than embedded recollection or older planning sources.

Check:

- concurrency and queues are bounded;
- task ownership, cancellation, and join behavior are explicit;
- shutdown drains or rejects work predictably;
- flush semantics cannot acknowledge data before durability;
- retries distinguish transient from permanent failures and avoid duplicate side effects;
- external calls use appropriate timeouts and structured errors;
- locks are narrow and never held across blocking work or avoidable awaits;
- blocking CPU, filesystem, or network work is isolated from async request paths;
- backpressure reaches callers instead of becoming unbounded memory growth;
- batching preserves ordering, identity, and tenant boundaries where required;
- panics or detached tasks cannot silently lose work;
- tracing exposes queue depth, retry, timeout, rejection, drain, and terminal failure state;
- tests exercise cancellation, saturation, retry, and shutdown edges.

## Lens-specific candidate requirements

Write only evidence-backed findings that clear the severity bar. Use critical, high, medium, or low.

For each finding include:

- **Severity**
- **Location**
- **Issue**: one plain-English sentence
- **Why it matters**: concrete failure or maintenance path
- **Evidence**
- **Required correction and static closure oracle**

Consolidate duplicates. Return the complete candidate report to the orchestrator, including clean rationale when no issue clears the bar. Do not assign final `REV-NNN` IDs, write outside the assigned specialist report, or modify source.
