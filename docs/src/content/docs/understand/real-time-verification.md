---
title: Real-time verification
description: Real-time verification for AI applications - checking a system while it runs, in time to inform the current action, with deadlines, evidence and check time, uncertain results, and when a queued or recurring check fits better.
pillar: wyrd
section: Understand
order: 2
---

Real-time verification checks the relevant properties of a system while it executes, soon enough for the result to inform the action or response in progress; it is not hard real-time scheduling, where missing a fixed deadline counts as a system failure, and it implies no particular latency. It matters because the same check that prevents a bad action when it runs first can only explain that action when it runs afterward.

## The same check can arrive too late

An application is about to share a quarterly report with an outside auditor. Two requirements apply: the recipient is authorized to receive the report, and the report contains no bank account number.

When the checks run before sharing, a violation means the report never leaves. The application can hold it, remove the account number, or ask a person. When the identical checks run an hour after sharing, on the same evidence, they find the same violation, but the report is already with the auditor. The result can still open an incident, contact the recipient, and change the next release. It cannot undo the share.

Nothing about the evidence or the check differed between the two runs. Only the time the result arrived, relative to the action, changed what it could do. The [Quickstart](/get-started/quickstart/) implements this example as a finance API that verifies each report before releasing it.

## What can be checked during execution

The sharing decision follows the same sequence as any request or agent step that is verified before it acts:

1. **A proposed action.** The application has the report, the recipient, the purpose, and the destination.
2. **Evidence.** It gathers what the requirements need: the report text, the result of the authorization lookup for this recipient, and the version of the sharing policy that applies.
3. **Checks.** A pattern check scans the report for account numbers. A comparison reads the authorization result. A judgment compares the purpose with the policy.
4. **A result** for each requirement, and an overall result.
5. **The next action.** The application shares the report, holds it, or sends it for review.

A check before the action can only use evidence that exists before the action: the proposed content and the preconditions. It can show that the report contains no account number and that the authorization record lists this recipient. It cannot show the effects of the share. An authorization check before sending does not prove that the report was delivered, that only the authorized person opened it, or that the recipient then handled it as the policy requires. Those are claims about a completed action, and they need evidence produced after it, such as a delivery receipt or the destination's access log.

Agents make this split routine. Before an agent calls a tool, its proposed arguments can be checked against policy. After the call, its effects can be checked: whether the tool reported success, and whether the system's state now matches what the agent intended.

## A useful result must arrive in time

The application has a deadline: how long it can wait before it must share the report or answer the person who asked. A result that arrives after the deadline cannot inform that decision.

The time before a result has three parts:

- **Collecting and preparing evidence.** The report may already be in memory. The authorization result may need a lookup, and a PDF may need its text extracted before any check can read it.
- **Coordination.** Whatever runs the checks has to receive the evidence, find which checks apply, confirm the caller may run them, and schedule the work.
- **The checks themselves.** Scanning the text for an account number or comparing a field is local computation. Deciding whether a policy written in prose permits this purpose may need a language model, possibly several calls, each subject to the provider's load and the network.

The model-assisted judgment usually dominates, and its time varies from one call to the next. Waiting for it synchronously does not bound how long it takes. Only measurement on representative evidence shows whether a check fits the deadline, and the measurement holds only for the load, model, and provider measured.

When a result cannot arrive in time, the application needs a fallback chosen on purpose: refuse the action, hold it for review, or act now and verify later where the risk allows. The checking system may also refuse work it cannot take on, or stop a check that runs past a timeout. Neither is a pass. Choosing the fallback is the application's decision, and making it before production is part of the design.

## Missing evidence and uncertain judgments

Three outcomes in the sharing example are often confused:

| Situation | What it means | Possible response |
| --- | --- | --- |
| A violation | The evidence shows the requirement is not met | Hold the report |
| Evidence that cannot decide | The check ran, but the evidence it needs is missing, or the judge could not decide | Hold the report, fetch the missing record, or ask a person |
| A check that did not run | A timeout, a crash, or refused work leaves no judgment at all | The fallback chosen in advance, never a pass |

An authorization lookup that returns nothing belongs in the second or third row, not the first: it does not show that the recipient is unauthorized. A judge that cannot tell whether "external audit" covers this purpose belongs in the second. Systems name these outcomes differently. What matters is that the application can tell them apart, and that none of them silently becomes a pass.

A model-assisted judgment also changes who sees the evidence. Sending the report and the policy to an externally hosted model exposes their content to that provider, and relies on the provider's model for that requirement. A deterministic check avoids both. Where a judgment is needed, deciding which evidence the judge may see is part of designing the requirement.

## When a later check is appropriate

Not every check belongs before the action. Each timing informs a different decision:

| When the check runs | Decision it can inform | Fits |
| --- | --- | --- |
| Inline, before the action | Whether the action happens | Proposed content and preconditions that must be settled first |
| Queued, after the response | Follow-up: correct, notify, or review | Slow judgments, and evidence such as traces that arrives after the action |
| Recurring, on a schedule | Investigation, retraining, and the next release | Properties of many executions together, such as drift or a failure rate |

A check can move later when its result could not change the current action anyway, when its evidence does not exist yet, or when it is too slow for the deadline and the risk of acting first is acceptable. Only a recurring check sees a property of many executions at once, such as a shift in the input distribution. Live verification and verification during execution are other names for the inline case.

## What must be measured and retained

Two measurements decide whether a real-time check is workable. The first is end-to-end time, measured from the application on representative evidence, with the expensive and variable judges running on their real models. The second is each check's accuracy, measured against labeled cases as for any other check.

Results also need provenance: which version of the requirement, which evidence, and which versions of the system and the check. Without it, nobody can later tell which policy a past decision applied. When a requirement changes, the results of the old version stay attached to it.

A returned result and a durable record are different things. A checking system can return the result first and write its record afterward, which is faster, or write the record first, so that every result the application acted on is kept. Which a given system does, and what it can lose when it fails, depends on the implementation, and is a property to confirm rather than assume.

## These ideas in Wyrd

Wyrd is one implementation of these ideas. [How verification works in Wyrd](/understand/how-verification-works/) maps requirements, evidence, checks, results, and provenance onto Wyrd, and follows one inline verification of the report release, including where its time goes and what it retains. The [Quickstart](/get-started/quickstart/) runs that example, and [Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) covers deadlines, provider exposure, and cost. Wyrd does not choose an application's requirements or fallbacks.

## Sources

Runtime verification, as checking an observed execution rather than every execution, and the distinction between online and offline monitoring come from [Leucker and Schallhart (2009)](/understand/foundations-of-verification/#sources). Monitors that judge a finite trace as satisfying, violating, or inconclusive are described by [Bauer, Leucker and Schallhart (2011)](/understand/foundations-of-verification/#sources). Delayed outcomes in drift detection are covered by [Gama et al. (2014)](/understand/foundations-of-verification/#sources). The decision deadline, the three parts of elapsed time, and the inline, queued, and recurring comparison are this page's practical synthesis, not established research terminology.

## Next

[How verification works in Wyrd](/understand/how-verification-works/) applies these ideas to a running system. [Foundations of verification](/understand/foundations-of-verification/) covers the general discipline behind them.
