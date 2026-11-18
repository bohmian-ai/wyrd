---
title: Real-time verification
browserTitle: Real-Time Verification for AI Applications | Wyrd
description: Real-time verification for AI applications - checking a system while it runs, in time to inform the current action, with deadlines, evidence and check time, uncertain results, and when a queued or recurring check fits better.
pillar: wyrd
section: Understand
order: 2
---

## What real-time verification is

Real-time verification is checking a system while it runs, early enough that the result can still change what the system does next. A check that runs before an application sends a message can stop a bad message. The same check run tomorrow can only report it.

Here, "real-time" means the result arrives before the system decides what to do next. It doesn't mean instant, and it doesn't promise any particular speed.

## Why timing matters

Most verification happens at one of two times. Tests run before release, and log reviews happen afterward. Both are useful, but neither checks a real input at the moment the system acts on it. Tests miss problems because they can't cover every real input. Log reviews find a problem only after it has done its damage.

AI systems make both weaknesses worse. A model can answer the same request differently each time, so passing a test once says little about how it will answer a real request later. An agent can take actions that can't be undone, such as sending an email or making a payment, so finding the problem in a log afterward is too late. Checking each real action before it happens catches the bad one before it does harm.

## The same check can arrive too late

An application is about to share a quarterly report with an outside auditor. Two requirements apply: the auditor must be authorized to receive the report, and the report must not contain a bank account number.

Suppose the application checks both requirements before sharing. If either check fails, the report is not sent. The application can hold the report, remove the account number, or ask a person to decide.

Now suppose the same checks run on the same report an hour after it was shared. They find the same problem, but by then the auditor already has the report. The team that runs the application can still investigate, contact the auditor, and change how the next report is handled. Nobody can unsend the report.

The report and the checks were the same both times. The only difference was timing. Before sharing, the result could stop the report. After sharing, it could only report the problem. Wyrd's [Quickstart](/get-started/quickstart/) builds this example: a finance application that checks each report before releasing it.

## What a check before the action can show

Checking the report before it is shared follows the same steps as checking any request or agent action before it happens:

1. **The application starts with the proposed action.** It has the report, the recipient, the reason for sharing, and the channel the report will be sent through.
2. **It gathers the evidence the requirements need.** That is the report text, a record of whether this recipient is authorized, and the company's sharing policy. The policy adds a third requirement: the reason for sharing must be one the policy allows.
3. **It runs one check per requirement.** A text search looks for account numbers. A simple comparison confirms that the authorization record approves this recipient. An AI model reads the policy and decides whether it allows this reason for sharing.
4. **Each check reports a result.** The overall result passes only if every check passes.
5. **The application decides what to do next.** It shares the report, holds it, or sends it to a person to review.

A check before the action can only use what exists before the action. It can show that the report has no account number and that the auditor is authorized. It can't show what happened after sending. For example, it can't show that the report arrived, that only the auditor opened the report, or that the auditor followed the policy. Those need evidence that exists only afterward, such as a delivery receipt or an access log.

Agent actions have the same split. Before an agent uses a tool, the request it is about to make can be checked against policy. Afterward, the outcome can be checked: whether the tool call succeeded and whether it made the change the agent intended.

## A result is only useful if it arrives in time

The application has a deadline: the longest it can wait before it must share the report or reply to whoever requested it. A result that arrives after the deadline can't affect whether the report is shared.

The time spent waiting for a result has three parts:

- **Gathering evidence.** The report may already be loaded. The authorization record may have to be looked up. If the report is a PDF, its text may have to be extracted before a check can read it.
- **Setting up the checks.** The checks often run in a separate service. Before that service runs any check, it receives the evidence, works out which checks apply, and confirms that the application may use them.
- **Running the checks.** A text search or a comparison takes almost no time. Asking an AI model whether the policy allows the sharing can take several calls to an outside service. Each call's time depends on how busy that service is and on the network.

The AI model usually takes the longest, and its time varies from one call to the next. The application can set a time limit and stop waiting, but then it has no result. A time limit doesn't make the model answer sooner. The only way to know whether the checks fit the deadline is to time them on realistic data. Those timings hold only for the traffic level, AI model, and provider used when they were measured.

Before launch, the application's team chooses what happens when the result won't arrive in time. The application can refuse the action, hold it for a person, or go ahead and check afterward if the risk is acceptable. The same plan covers two other cases: the checking service rejects the request because it is overloaded, or it stops a check that runs too long. A rejected or stopped check never counts as a pass.

## When the answer is "can't tell"

A check can end in three ways that are often confused. Using the report example:

| Result | What it means | Possible response |
| --- | --- | --- |
| A violation | The evidence shows the requirement is not met | Hold the report |
| Can't tell | The check ran, but evidence is missing, or the AI model couldn't decide | Hold the report, find the missing record, or ask a person |
| No result | The check timed out, crashed, or was rejected | Follow the plan chosen before launch: refuse, hold, or check later. Never treat it as a pass |

Suppose the authorization lookup comes back empty. The result is "can't tell". If the lookup times out or fails, the result is "no result". Neither is a violation, because neither shows that the auditor is unauthorized.

Now suppose the policy allows sharing for audits, and the AI model can't decide whether this reason for sharing counts as an audit. The result is again "can't tell".

Different systems use different names for these three results. What matters is that the application can tell them apart, and that none of them is ever treated as a pass.

## What an AI model sees

Using an AI model run by another company means sending it the report and the policy. That company sees both. The result also depends on a model that the company controls and can change. A rule-based check, such as a text search, sends nothing to another company and doesn't depend on anyone else's model. When a check needs an AI model, deciding what the model is allowed to see is part of designing the check.

## Not every check has to happen before the action

A check can run before the action, right after it, or on a schedule. Each timing helps with a different decision:

| When the check runs | What it can change | Good for |
| --- | --- | --- |
| Before the action | Whether the action happens | Things that must be settled first, such as who may receive the report |
| Right after | Follow-up: correct, notify, or review | Slow checks, and evidence that exists only afterward |
| On a schedule | Investigations, retraining, and the next release | Patterns across many requests, such as a rising failure rate, or real requests that differ more and more from the test data |

A check can run later instead of before the action in three cases:

- its result couldn't change the action anyway;
- its evidence doesn't exist until afterward;
- it is too slow, and acting first is an acceptable risk.

Real-time verification is the first row of the table: checking before the action.

## What has to be measured

Two measurements decide whether real-time checking works for an application:

- **How long the whole check takes**, from the moment the application asks to the moment it gets the result. It has to be measured on realistic data, with the AI model set up exactly as it will be in production.
- **How often each check is right.** This is measured by running the check on examples whose correct result is already known.

## What has to be kept

Requirements, systems, and checks change over time, so each one has versions. Each result needs a record of the versions that produced it and the evidence it used. Without that record, nobody can later tell which policy a past decision followed. Because of the record, old results stay tied to the old version of a requirement after it changes.

Getting a result back is not the same as having it saved. Some checking systems send the result first and save the record afterward, which is faster. If such a system crashes after sending the result but before saving it, the application has acted on a result that has no record. Other systems save first, so every result the application acted on is kept. It is worth confirming which order a system uses instead of assuming.

## These ideas in Wyrd

Wyrd is one implementation of these ideas. The page [How verification works in Wyrd](/understand/how-verification-works/) follows the report example through Wyrd, including where the time goes and what Wyrd keeps. The [Quickstart](/get-started/quickstart/) runs the example, and [Verify during a request](/build-and-verify/verify/verify-a-request-in-real-time/) checks each report inside a web API before releasing it. [Check on a schedule](/build-and-verify/verify/monitor-drift/) runs the recurring row of the table above. Wyrd calls a check's result a verdict. The guide [Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) covers deadlines, what an AI provider sees, and cost. Wyrd does not choose an application's requirements, or its plan for when a check can't finish.

## Sources

[Leucker and Schallhart (2009)](/understand/foundations-of-verification/#sources) introduce runtime verification: checking real runs of a system, as they happen or afterward. [Bauer, Leucker and Schallhart (2011)](/understand/foundations-of-verification/#sources) describe checks that can't give a result until more of the run has happened. [Gama et al. (2014)](/understand/foundations-of-verification/#sources) describe the delay before the true outcome of a decision is known, which is why some checks can run only later. The research doesn't use this page's terms for the deadline, the three parts of the wait, or the three check timings. This page introduces them as a practical summary.

## Next

[How verification works in Wyrd](/understand/how-verification-works/) applies these ideas to a running system. [Foundations of verification](/understand/foundations-of-verification/) covers the general ideas behind them.
