---
name: wyrd-spec
description: First stage of the Wyrd build pipeline. Turns a raw idea into a durable spec.md — the decision and contract authority — before any planning or code. Use when the user says /spec, starts a new Wyrd feature or significant change, has an ambiguous idea, or needs success criteria and architectural decisions settled before a plan exists. Routes to wyrd-architecture-reviewer skill (gate 1). Do not write code or a commit plan here.
---

# Wyrd Spec

Stage 1 of the pipeline: **idea → spec → plan → tasks → implementation → test → review**.

Produce `spec.md`: the *what and why*. It is the decision and contract authority
the rest of the pipeline executes against. It does **not** contain code, a commit
DAG, or file-level detail — those belong to `plan` and `tasks`.

The spec exists so `wyrd-architecture-reviewer` can catch doctrine and architecture
gotchas at the cheapest possible point — before a commit is planned, let alone
written.

## When To Use

- `/spec`, a new feature, a significant change, a vague or half-formed idea.
- Any work where the requirements, contracts, or cross-cutting decisions are not
  yet settled.

Skip for a one-line fix with no new contract or decision — go straight to the
relevant implementation skill.

## Source Of Truth

Ground in the Wyrd repo before asking anything. Do not ask what the repo answers.

1. `AGENTS.md`
2. `architecture/wyrd-design.md` — active design authority.
3. `docs/src/content/docs/concepts/core-doctrine.mdx` — doctrine vocabulary and
   the Card/Spec/Run/Observation ontology.
4. The nearest existing architecture file, spec, or implementation for the
   surface being specced.

Use CodeGraph to read existing contracts and seams (`codegraph_explore`), not a
grep/read loop.

## Process

1. **Ground.** Read the sources above and the relevant existing code. Separate
   discoverable facts (what the repo already decides) from genuine product intent
   (what only the user can decide).
2. **Interview.** Ask high-impact questions **one at a time** (reuse the
   `wyrd-plan-interviewer` discipline). Cover goal, users/agents, success
   criteria, scope, constraints, and the tradeoffs behind each cross-cutting
   decision. State assumptions explicitly when proceeding with a default; never
   silently guess a decision.
3. **Decide.** Drive every cross-cutting question to an accepted decision with a
   one-paragraph rationale. These decisions are the load-bearing output — the
   plan, tasks, and arch review all hang off them.
4. **Write** `spec.md` to `.dev/plan/<feature>/spec.md`. Written spec should be human
    legible and understandable. The spec is mean to be reviewed by a human and fed to
    an agent for planning. Write accordingly (concise, clear, consistent)
5. **Gate 1.** Hand `spec.md` to `wyrd-architecture-reviewer` **at full-sweep
   depth** — explicitly tell it "full sweep: continue past blockers, report every
   materially-separate finding," and do **not** hand it a pre-narrowed focus list
   (a short focus list silently scopes it to blocker-only). The spec is where
   decisions get *locked*; a missed finding here propagates through plan → tasks →
   code, and it is cheapest to fix as prose. Blocker-only is a false economy at a
   pipeline gate — reserve it for a fast pre-spec "is this even doctrine-legal?"
   gut-check, never as the Gate-1 pass. Resolve findings (loop with
   `wyrd-plan-interviewer` if a decision reopens) before moving to `wyrd-plan`.

## References

Load from the shared doctrine library (`.claude/references/`, indexed in
`.claude/references/README.md`) when relevant:

- `doctrine/positioning-and-vocabulary.md` — doctrine vocabulary, Card envelope,
  forbidden surface drift (frames every contract).
- `doctrine/architecture-constraints.md` — product/service/crate boundaries the
  spec must respect.
- `review/agent-first-review.md` — whether a small, literal agent can reason about
  the contracts being specced.
- `review/review-rubric.md` — the dimensions gate 1 judges; write the spec to pass
  them by construction.

## Spec Shape

```markdown
# Spec: <Feature>

## Objective
What is being built, who/what it serves (human and agent surfaces), and why it
matters now.

## Success Criteria
- Specific, testable outcomes. Observable behavior, not implementation.
- End to end journeys that will be tested - for user and agent experience

## Scope
- In:
- Out:

## Contracts
Behavior-level interfaces that change: HTTP routes, MCP tools, CLI verbs, Python
surfaces, Card/Spec kinds, schemas, config, and the stable Wyrd error codes
involved. Describe the contract, not the code.

## Cross-Cutting Decisions
For each decision that spans more than one future commit:
- **Decision:** the accepted choice, in one sentence.
- **Why:** the tradeoff and the reason this option won.
- **Consequences:** what it forces downstream (boundaries, schema, isolation).
Label anything that reopens a previously locked decision and cite the file.

## Repo Context
Existing patterns, crates, seams, and constraints discovered from the repo
(named as symbols, not line numbers).

## Boundaries
- Always:
- Ask first:
- Never:

## Open Questions
Only unresolved items that block or materially change the plan.
```

## Standards

- The spec is the decision authority. Pin decisions and contracts exhaustively;
  leave implementation rendering to the executor (it hydrates seams from
  CodeGraph at build time — see `wyrd-tasks`).
- Convert vague requests into observable, testable success criteria.
- Honor the `wyrd-rust-python` non-negotiables and `architecture/wyrd-design.md`
  (Card/Spec/Run/Observation ontology, shared envelope, `wyrd-spec` is
  PyO3/SQL-free, client-tier dependency limits, audit/tenant isolation). A spec
  that violates a lock must label it and route to review, not bury it.
- No code, no commit DAG, no file-by-file plan. If you are sequencing commits,
  you have left the spec stage.

## Verification

Before handing off:

- Objective, success criteria, scope, contracts, cross-cutting decisions, repo
  context, boundaries, and open questions are all present.
- Every cross-cutting question is an accepted decision with a rationale, or a
  labeled open question that blocks planning.
- `wyrd-architecture-reviewer` (gate 1) has run on `spec.md` and its findings are
  resolved.

## Hand-Off

`spec.md` (approved at gate 1) → **`wyrd-plan`**: decompose into the commit DAG
(`plan.md`) in the existing `00-overview` format, then gate 2.
