---
name: wyrd-plan-review
description: Review a Wyrd plan or task packet for source-grounded decision completeness, executability, real dependencies, claim coverage, and credible evidence. Use before implementation when readiness matters; do not enforce a particular document layout or execution controller.
---

# Wyrd Plan Review

Decide whether implementers can execute the proposed work without inventing a
material product, contract, ownership, persistence, security, tenancy,
migration, rollout, or acceptance decision. Review plan meaning, not formatting
or controller protocol.

Remain read-only. Accept a plan path, plan directory, task packet, or supplied
text. An immutable source revision or artifact digest is useful provenance when
supplied; its absence does not make an otherwise readable plan unreviewable.
Review any Wyrd plan regardless of its handoff string or packet schema.

## Establish authority

Read the named artifacts, user intent, `AGENTS.md`,
`architecture/agent-rules.md`, and the relevant portions of Wyrd design and
doctrine. Inspect only the owners, implementations, consumers, tests,
manifests, and `mise` tasks needed to adjudicate a credible concern. Follow
CodeGraph instructions. Use an immutable revision when the caller supplies one;
otherwise state the repository state reviewed.

## Review readiness

Assess these semantic axes:

1. **Source grounding** — material claims and precedents match repository
   authority.
2. **Decision completeness** — material behavior, failure, ownership, security,
   tenancy, persistence, migration, and compatibility choices are settled or
   explicitly escalated.
3. **Outcome and task coverage** — every requested outcome and cross-boundary
   closure has a cohesive owner; scope and non-goals are usable.
4. **Dependencies** — edges identify real integrated prerequisites; no missing
   producer, unsafe shared seam, or false ordering blocks execution.
5. **Claims** — required obligations are atomic enough to adjudicate,
   behavior-focused, collectively sufficient, and traceable to outcomes.
6. **Evidence** — each required claim names credible direct proof and accepted
   evidence classes; proof can detect failure and respects repository test and
   journey rules.
7. **Integration** — cross-task claims, generated projections, and journeys
   have an owner and occur after their actual prerequisites.

Claim and evidence classes may follow the Wyrd Change trust vocabulary, but
plan readiness is not product verification or authorization. Do not collapse
deterministic evidence, model evaluation, and human attestation into one score.
Do not accept a mutable analytical query or external status projection as a
substitute for evidence when the planned behavior requires durable finalized
evidence.

Task layout, headings, YAML, claim IDs, write forecasts, exact command slots,
digests, skill names, and scheduling strategy are preferences unless their
absence creates real ambiguity, unsafe overlap, or unprovable acceptance.
Write sets are forecasts, not allowlists. Do not demand private helper names,
exhaustive file inventories, or a different task split merely because another
shape is possible.

Concurrency is advisory. Report an objectively false dependency, unsafe fanout,
or materially unowned integration seam; do not reject a ready plan based on
speculative wall-clock economics.

## Finding threshold

Report only findings that would force material rediscovery, produce likely
behavioral drift, make implementation unsafe, or prevent credible acceptance.
Consolidate related symptoms by root cause.

A blocking finding needs:

- an exact plan/task location;
- concrete repository or authority evidence;
- the implementation or proof consequence; and
- the smallest semantic correction required.

Do not block on wording, formatting, naming, optional metadata, recoverable
command drift, private implementation choices, or harmless preference
differences. Omit nonblocking advice unless the caller asks for it or it has
unusually high execution value.

## Verdict and output

Use one verdict:

- `READY` — no blocking readiness gap; reversible mechanics remain with the
  implementer;
- `REVISE` — bounded semantic repairs are needed without changing direction;
- `MATERIAL_DECISION_REQUIRED` — new product, public/durable contract,
  ownership, security, tenancy, migration, rollout, or acceptance authority is
  required; or
- `REVIEW_BLOCKED` — the named artifact or essential authority cannot be
  inspected after reasonable discovery.

Lead with the verdict. Then report readiness by the seven axes and list only
blocking findings:

```text
Finding PR-001 — <Critical|Major>: <title>
Location: <plan/task section>
Evidence: <source/authority fact>
Consequence: <why execution would stall, drift, or be unprovable>
Required plan change: <smallest semantic correction>
```

For `READY`, say explicitly: `No blocking readiness findings.` Structured YAML
is optional when the caller or an automation contract requests it; empty
procedural fields are never required.
