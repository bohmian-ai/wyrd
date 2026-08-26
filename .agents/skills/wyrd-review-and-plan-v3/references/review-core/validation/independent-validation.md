# Independent Candidate Validation

Challenge a preliminary full-review ledger without producing a competing
verdict or implementation plan.

Read the immutable packet, specialist reports, coverage, preliminary ledger,
approved intent, active authority, `adversarial-contract.md`, and
`maintainer-gate.md`. Inspect the target snapshot independently. Do not accept
the orchestrator's proposed decision as evidence.

Use a reviewer identity that produced no specialist candidate report. Before
challenging candidates, audit the seven-domain baseline roster, required
capability matches, trigger completeness, prompt and target digests, and
distinct reviewer identities. Any gap blocks merge-readiness.

For every candidate selected by the independent-validation floor, challenge:

- source and caller reachability;
- rejection and materiality reasoning;
- stale-intent or authority claims;
- deduplication that may hide independent root causes;
- severity, disposition, hard-gate treatment, and final mapping;
- unresolved dissent among specialist reports.

Also challenge every high-risk boundary declared clean. Construct the strongest
realistic counterexample for its governing invariant, inspect whether its guard
dominates every reachable mutation or durable effect, and conclude `survived`,
`candidate required`, or `static limit`. A clean specialist report is evidence
to challenge, not proof of correctness.

Return one section per candidate containing:

- candidate ID;
- challenged preliminary decision;
- source, callers, consumers, tests, contracts, and authorities inspected;
- conclusion: agree, revise, split, merge, or unresolved;
- concrete rationale and evidence;
- any material dissent that remains.

Return a separate section for each challenged clean high-risk boundary with
the invariant, counterexample, entry point, inspected evidence, and conclusion.

Write only to the orchestrator-assigned validation evidence file. Do not assign
`REV-NNN`, write `review.md`, create a plan, run project commands, launch other
agents, or modify source.
