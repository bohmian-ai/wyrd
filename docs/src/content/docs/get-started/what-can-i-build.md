---
title: What can I build?
description: Example verification workflows built from Wyrd's primitives - policy documents, agent evaluation, long-running workflows, runtime checks, observability, and drift.
pillar: wyrd
section: Get started
order: 4
---

Wyrd gives you primitives, and you compose them into whatever verification your system needs. The workflows below are examples, not a complete list. Each names a problem, the composition that solves it, and the page that shows how. [Run your first verification](/get-started/quickstart/) builds the smallest one: a single assertion.

Every workflow uses the same parts. Cards declare exact versions of what you ship: Services, Agents, Prompts, Models, Data, and uploaded Artifacts. A Verifier composes deterministic checks (`assert`, `trace`, `code`) with model judgments (`agent`) and scheduled `drift`. Each verdict cites its evidence and is stored in Bifrost with the exact versions it judged. Your application runs the workflow and decides what a verdict means; Wyrd does not orchestrate or enforce it.

## Policy verification

**Problem.** Payments, refunds, or other decisions must follow a written policy document.

**Workflow.** Upload the policy once as a versioned Artifact. A Verifier mounts the exact files an Agent check reads, and the judge cites the line it relied on. Put exact rules, such as an amount limit or a required field, in `assert` or `code` checks beside it, so they never depend on a model. Send the facts the decision rests on as evidence, then act on the verdict in your application: approve, hold, or escalate.

[Uploaded files](/build-and-verify/verify/verifier-recipes/#uploaded-files) walks through the upload, the Service, the call, and the cited result. [Steps and checks](/understand/how-verification-works/#steps-and-checks) explains which kind decides which rule.

## Agent evaluation

**Problem.** An agent's answers must be correct and within policy, and you need to know before a new prompt or model ships.

**Workflow.** Bind a Verifier to the Agent or its Service. Combine assertions with a [model judge](/build-and-verify/verify/verifier-recipes/#model-judge) that returns a cited verdict and, if you declare one, a score or label. Run the same Verifier from tests before release and on live traffic after. Each verdict names the exact Service and Verifier versions, so a regression points at the change that caused it.

[Write a Verifier](/build-and-verify/verify/author-a-verifier/) builds one step by step.

## Long-horizon workflows

**Problem.** A coding, review, or support workflow runs for minutes or hours across many steps.

**Workflow.** Verify each step as it finishes, with one `group_id` for the whole workflow. A [grouped session](/build-and-verify/verify/verifier-recipes/#grouped-session) Verifier then judges the accumulated evidence and outcomes once the group goes quiet. A [trace](/build-and-verify/verify/verifier-recipes/#trace) check confirms the steps ran in the order the workflow requires. Your code or agent framework runs the workflow; Wyrd records and judges it.

## Runtime verification

**Problem.** A request must be checked before your application acts on it, or checked in the background without delaying it.

**Workflow.** `verify` returns the verdict to the caller, so your code can act on it before it responds. `submit` queues the check and returns an execution that any process can wait on. [verify and submit](/understand/how-verification-works/#verify-and-submit) explains both. An `agent` check adds a model call to the wait; an `assert` check needs none.

## Observability

**Problem.** You need to see what the system did, not only whether a check passed.

**Workflow.** Wrap each request in a Run, export OpenTelemetry spans, record your own rows, and send model calls through the [LLM gateway](/connect/set-up-the-llm-gateway/). Bifrost keeps traces, logs, gateway calls, rows, and verdicts queryable with SQL, joined on the Run. [Runs and observations](/build-and-verify/observe/record-a-run-and-observations/) records one Run and reads it back; [Bifrost](/understand/bifrost/) explains the data plane.

## Drift monitoring

**Problem.** Inputs, outputs, or model behavior shift slowly, and no single request fails.

**Workflow.** Declare a baseline as a Data Card, record samples from your code, and run a [drift](/build-and-verify/verify/verifier-recipes/#drift) Verifier on a schedule. An Operator on that binding can notify Slack or PagerDuty when a window fails; [React to a failure](/build-and-verify/declare/bind-a-verifier/#3-react-to-a-failure) adds one.

## Next

[Recipes](/build-and-verify/verify/verifier-recipes/) has a copyable Verifier for each check. [Use with your coding agent](/get-started/use-with-your-coding-agent/) gives your agent these pages and your server's evidence.
