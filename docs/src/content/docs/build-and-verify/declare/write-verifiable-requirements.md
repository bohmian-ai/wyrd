---
title: Write verifiable requirements
description: Turn a vague goal such as "the agent should be reliable" into scoped requirements with named evidence, a suitable check, tested model judges, and versioned changes.
pillar: wyrd
section: Build and verify
topic: Declare
order: 0
---

Turn a goal like "the agent should be reliable" into requirements a check can decide, then keep them trustworthy as they change. The examples use the `release-check` Verifier from the [Quickstart](/get-started/quickstart/).

Each practice names the failure it prevents and where it stops helping. [Foundations of verification](/understand/foundations-of-verification/) explains the reasoning behind them.

## 1. Scope each requirement

**Prevents:** a requirement no check can decide, so the judge decides what it means.

Write one statement per failure you care about, naming the context it applies to:

| Avoid | Write instead |
| --- | --- |
| The agent should be reliable. | The recipient is authorized to receive the report. |
| The report should be safe to share. | The destination restricts access to the report. |
| The agent should follow company policy. | The company sharing policy permits this release. |

Each statement on the right fails in one way, and a person reading a failed result knows what broke. `release-check` declares these three, each with its own `id` and verdict:

<CodeFromFile file="examples/finance-release/release-check.yaml" lang="yaml" title="release-check.yaml" range="14-31" />

In the Quickstart the policy judge also catches a named bank account, because the policy forbids it; a separate `code` check would make that its own requirement, with its own verdict.

**Limits:** a scoped requirement checks only what it states. Failures you did not write down pass unnoticed, so review the list when incidents or new uses appear.

## 2. Name the evidence

**Prevents:** judging what the system claims instead of what it did.

For each requirement, name the evidence that settles it and where it comes from:

| Requirement | Evidence | Source |
| --- | --- | --- |
| `recipient-authorized` | The recipient's authorization record | The application's own lookup, not the model's summary |
| `follows-policy` | The report, the purpose, the recipient, and the policy text | The request, and the policy file the Verifier version mounts |
| `destination-restricted` | The destination's access settings | The application's destination record |

Each piece of evidence becomes an input slot. If the evidence is not captured today, capture it before you write the check. [Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/) covers provenance, citations, and missing evidence.

**Limits:** evidence from your own systems is only as accurate as those systems. A wrong authorization record produces a confident wrong verdict.

## 3. Choose checks and test judges

**Prevents:** a slow, costly, or unreliable check where a precise one would do, and a model judge nobody has tested.

Use the cheapest check that decides the requirement:

| The rule is | Check | Example |
| --- | --- | --- |
| A field comparison | `assert` | `recipient.authorized` equals `true` |
| About what the code did | `trace` | The authorization service was called before the send |
| Deterministic, but no operator expresses it | `code` | The report text matches no account-number pattern |
| A judgment about meaning or policy | `agent` | The policy permits this purpose |

`assert`, `trace`, and `code` checks give the same verdict for the same pinned evidence. An `agent` check can vary between runs, costs a model call, and sends its evidence to a provider. [Recipes](/build-and-verify/verify/verifier-recipes/) has a working Verifier for each kind.

Before relying on a model judge, run it on labeled cases:

- Cases the policy clearly permits, which must pass.
- Cases it clearly forbids, which must fail.
- Ambiguous or incomplete cases, which should come back `inconclusive` rather than guess.

The Quickstart's four releases cover the first two kinds: one permitted, one naming a bank account, two with purposes the policy does not allow. Add an ambiguous case before relying on the judge. Rerun the set after any change to the judge's prompt, model, or policy, and keep the results.

**Limits:** a judge that passes your cases can still fail on cases unlike them. Wyrd requires a passing judge to cite its evidence, but it does not measure the judge's accuracy. Measure agreement with labeled outcomes from the domain it judges before relying on it.

## 4. Keep cases for pass, fail, and inconclusive

**Prevents:** a change to a check that silently stops catching what it used to.

For every requirement, keep at least one case of each outcome and run them whenever the Verifier changes:

| Case | Expected | Finance example |
| --- | --- | --- |
| Meets the requirement | `passed` | The external-audit release |
| Breaks it | `failed` on that requirement | Set `authorized` to `false`: `recipient-authorized` fails, whatever the judge decides |
| Lacks the evidence | `inconclusive` | Omit the destination record: `destination-restricted` is `inconclusive` |

A failure case that does not fail its intended requirement is a defect in the case or the check.

**Limits:** cases test the Verifier you have, not the requirements you should have. They catch regressions, not omissions.

## 5. Change requirements through versions

**Prevents:** a past verdict that silently changes meaning when the requirement does.

Change a requirement, a check, or a file the Verifier mounts by declaring it again. Wyrd registers a new Verifier version and leaves the old one as it was. In the Quickstart's policy update, the revised policy creates `release-check@0.1.1`; results from `0.1.0` still name the original policy.

Rerun your cases against the new version and compare. Expected changes should be the only changes: in the policy update the regulator request moves from `failed` to `passed`, and the other three releases keep their verdicts.

**Limits:** versions record what changed, not whether the change is right. A domain owner still approves the new expectation.

## If it fails

| You see | Fix |
| --- | --- |
| A requirement is `inconclusive` in every case | The slot its check reads is not supplied. Match your input names to the Verifier's `inputs` |
| `WYRD_SPEC_400_VALIDATION` with rule `unknown_reference` | A check reads a slot or step that is not declared. Match the `id` it names |
| A labeled case gets the wrong verdict from the judge | Fix the judge's prompt or the case, rerun the whole set, and keep both results |

## Next

[Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/) for each requirement, then [Write a Verifier](/build-and-verify/verify/author-a-verifier/) to declare and call it.
