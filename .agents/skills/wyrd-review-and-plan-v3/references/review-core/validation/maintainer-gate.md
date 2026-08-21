# Pedantic Maintainer Materiality Gate

Use this gate after cross-reviewer deduplication and source validation. The
maintainer is deliberately strict about production value, not about personal
style.

For every deduplicated finding, answer all of these questions:

1. What exact input, state, workload, deployment condition, or caller path
   triggers it?
2. Is that path reachable in Wyrd's self-hosted, SaaS multi-tenant, or
   enterprise single-tenant deployments?
3. What is the concrete impact: tenant or security boundary, named compliance
   control, data correctness/durability, distributed reliability,
   availability/resource bounds/performance, public contract/ergonomics, or
   maintainability/ownership?
4. What is the blast radius, likelihood, detectability, and recovery path?
5. Would a responsible maintainer block this change? If not, why should it be
   implemented in this slice instead of tracked as follow-up or known deferral?
6. Does the proposed fix preserve the supplied intent's user workflow,
   ownership, dependency cone, and future regression contract?

Assign exactly one disposition:

- `BLOCK_BEFORE_MERGE`
- `FIX_BEFORE_PRODUCTION`
- `FOLLOW_UP`
- `KNOWN_DEFERRED`
- `FALSE_POSITIVE`

Static uncertainty is not a disposition. If committed source, contracts, tests,
rules, and intent cannot establish a defect, omit it from required findings and
record a material limitation under the report's static-analysis boundary when
useful.

Do not promote a finding merely because it cites a checklist item, personal
style preference, or missing unit test. An explicit repository-rule violation,
public documentation contract, semantic readability failure, language-idiom
problem, or concrete simpler alternative is not personal preference when the
review cites the exact rule, source location, local precedent, and maintenance,
caller, performance, testability, or contract consequence. Confirmed rule
violations and plan deviations remain required when the rule is an explicit
non-negotiable gate or the deviation creates a concrete correctness, security,
contract, performance, operational, or maintenance failure. Otherwise preserve
them as `FOLLOW_UP` or `KNOWN_DEFERRED` observations with the rule and rationale
recorded; do not discard them as false positives solely because they are
categorized as style or idiom.

A current repository rule is a **hard gate** when it explicitly identifies
itself as `MUST`, non-negotiable, a hard acceptance criterion, incomplete when
violated, or prohibited from passing review. When static source inspection
confirms that new or materially modified code violates an applicable hard gate,
assign `BLOCK_BEFORE_MERGE`. The production trigger and material impact fields
must explain the maintenance, ownership, caller, performance, testability, or
contract consequence, but they may not downgrade the disposition. Reject or
escalate only when the rule is stale, contradictory, inapplicable, or the
review cannot determine whether the code is materially modified.

Only `BLOCK_BEFORE_MERGE` and `FIX_BEFORE_PRODUCTION` findings become v3
remediation or material-decision handoffs. A real but non-blocking issue remains
visible in `review.md` and includes an owner, planned phase, or reason for
deferral when that context is available.

Required validation fields:

```text
Production trigger:
Material impact:
Evidence and reachability:
Disposition:
Disposition rationale:
```

Before a finding survives, perform a maintainer-usefulness check: an engineer
unfamiliar with the branch must be able to identify the broken flow, reproduce
the failure, start at the named owner, implement the ordered correction, and
write the regression proof from the finding alone. If essential facts exist
only in specialist evidence, rewrite the finding before assigning `REV-NNN`.

When more than eight findings survive as required implementation work, perform
one additional consolidation pass. Do not impose a hard finding quota; instead
explain why each remaining item is independently required now and group shared
root causes.
