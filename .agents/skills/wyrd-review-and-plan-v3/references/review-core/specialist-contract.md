# Specialist Evidence Contract

Apply this contract to every full-mode specialist report. Read
`adversarial-contract.md` before reviewing. Lens prompts add
questions and matrices but do not replace this schema.

Read `orchestration-contract.md` first. A report is evidence for exactly one
assignment and reviewer instance.

## Durable report

Write one report to:

```text
{REVIEW_DIR}/evidence/specialists/{LENS}.md
```

The orchestrator chooses a stable lowercase-hyphen lens name. Candidate IDs are
namespaced by that lens and assigned before validation:

```text
{LENS}/C001
{LENS}/C002
```

Never assign `REV-NNN`; those IDs belong to validated final findings.

## Required sections

The immutable packet and `coverage.json` carry assignment identity, model,
prompt digest, target SHA, and trigger metadata. Do not repeat them in the
specialist report. Use these sections exactly once and in order:

1. `## Scope`
2. `## Adversarial probe`
3. `## Candidate findings`
4. `## Clean rationale`
5. `## Static-analysis limits`

Use `None.` only where there is genuinely no entry. `Scope` and
`Adversarial probe` must never be empty.

`Scope` concisely records:

- files and symbols assigned;
- files and symbols actually inspected;
- callers, consumers, contracts, tests, and local precedents inspected;
- requirements and acceptance criteria inspected;
- anything sampled instead of exhaustively inspected; and
- any domain-specific check that materially affected the conclusion.

`Adversarial probe` records the falsification attempts required by
`adversarial-contract.md`. At least one material probe is required. A clean
assignment must include the most dangerous relevant changed invariant, the
strongest realistic counterexample attempted, and why it survived inspection.

## Candidate schema

Use this shape for every candidate:

```markdown
### <lens>/C001: <plain-English root cause>

Severity: critical | high | medium | low
Confidence: high | medium | low
Category: <category>
Locations:
- `path:line` — `symbol`
Requirements: <IDs or none>
Rules: <exact rules or none>

#### Maintainer summary
<In 2-4 sentences, explain current behavior, consequence, and correction
without relying on this title or another artifact.>

#### Problem
<Current flow, expected behavior, exact divergence, and source, caller,
contract, test, intent, rule, and precedent evidence.>

#### Failure scenario
<Concrete entry point, input/state sequence, observable result, affected
parties, blast radius, detectability, and recovery.>

#### Recommended correction
<Named owner and symbols, ordered behavioral changes, required semantics,
constraints, precedent, non-goals, and unsafe shapes.>

#### Verification
<Named test tier/location, setup, action, exact assertions, and precise static
closure inspection.>
```

Every candidate must cite target-snapshot evidence. Do not emit speculative,
preference-only, or working-tree-only candidates. Consolidate symptoms sharing
one root cause within the specialist report. Attempt to disprove the candidate
against its owner, callers, contracts, and tests before returning it.

Do not use references such as “the cited owner”, “the invariant expressed by
the title”, or “normal production workflow”. Do not label a location merely
`validated source location`. Essential facts belong in the candidate even when
the same facts also appear in supporting evidence.

`Clean rationale` records assigned coverage that cleared the lens, including
the source inspected and why it satisfies the applicable rule or invariant. A
review with no candidates is complete only when its clean rationale explains
the result.

`Static-analysis limits` records material assigned scope that source inspection
could not settle. Do not promote uncertainty into a candidate.

## Completion

Return the report path and candidate IDs through the agent-result channel.
Do not create plans, run project commands, launch agents, or modify production
source.
