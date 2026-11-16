---
title: Technical questions
description: Real-time verification technical questions - who enforces a verdict, verify versus submit and schedules, missing evidence, timeouts, model providers and sensitive data, versions, languages, and self-hosting.
pillar: wyrd
section: Understand
order: 9
---

These are direct answers to the questions engineers ask before adopting Wyrd. Each links to the page that owns the detail.

## Does Wyrd block a response that fails?

No. Wyrd returns a verdict; your application decides what to do with it. In the [finance API](/understand/how-verification-works/#one-verification-end-to-end), the handler releases a report only when `result.passed` is true and holds it otherwise. If the handler ignored the verdict, nothing would stop the release.

Enforcement lives in your code because the right response depends on the action: hold a report, rewrite a reply, ask a person, or log and continue. Executions a binding's Trigger creates can also notify through an [Operator](/understand/how-verification-works/#operators), which reports a failure but does not undo anything.

## When should I use verify, submit, or a schedule?

Use `verify` when the application must decide before it responds or acts. Use `submit` when the decision can wait, the check is too slow for the request, or the evidence lands after the call. Use a schedule to judge a time window of recorded samples, as with drift. [verify and submit](/understand/how-verification-works/#verify-and-submit) compares the three. `verify` and `submit` run the same Verifier; schedules judge drift Verifiers, which refuse `verify`.

## What happens when evidence is missing?

The requirements that need the missing evidence come back `inconclusive`, and the others are judged normally. With the default decision rule, any inconclusive requirement makes the verdict inconclusive unless another requirement failed. `inconclusive` is never a pass and never a claim that the subject failed.

A trace that has not landed yet is a special case: `verify` judges the `trace` check inconclusive at once, and a queued execution tries 3 times before it does. A missing trace on its own does not prove an action was skipped. [Trace evidence](/understand/how-verification-works/#trace-evidence) has the rules.

## What happens on a timeout or an error?

A `verify` call that runs past `WYRD_VERIFICATION_TIMEOUT_MS` (60 seconds by default) returns `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` and no verdict. The SDK does not retry it, so your code chooses the fallback. Invalid input is refused with a typed error before anything runs. A queued execution that times out or errors has status `timed_out` or `errored` and no verdict.

Treat every outcome except `passed` as not passed. [Runtime limits](/reference/verification-runtime-limits/#if-a-limit-is-reached) lists each limit and its result, and [Error codes](/api/errors/) lists every code.

## How fast is a real-time check?

It depends on the checks. Assertions and trace checks run without a model; a model judge adds at least one model call and usually dominates. Wyrd publishes no general latency figure, because the total depends on your Verifier, evidence, model, and provider. Measure the Verifier you deploy. [Where the time goes](/understand/how-verification-works/#where-the-time-goes) separates engine, evidence, and check time.

## What does a model provider see?

An `agent` check sends its prompt, with the evidence the prompt includes, to the model provider through your server's [LLM gateway](/connect/set-up-the-llm-gateway/). Anything the Agent reads with a tool, such as a policy file through `fs.read`, is sent too. The provider key stays on the server, and each tool call needs its own [grants](/reference/agent-tools-and-authorization/).

Assertion, trace, and code checks send nothing to a provider. Keep sensitive evidence out of model checks when a deterministic check can decide the requirement, and use synthetic data while you build.

## Can I trust a model judge?

Not without testing it. A model judge can be biased, inconsistent, or wrong, and agreement with another model is not independent evidence. Wyrd requires a judge that passes to cite the evidence it used, and makes the requirement `inconclusive` when it does not; it does not measure a judge's accuracy for you. Run the judge on labeled cases before relying on it, as in [Choose checks and test judges](/build-and-verify/declare/write-verifiable-requirements/#3-choose-checks-and-test-judges). [Foundations of verification](/understand/foundations-of-verification/#when-there-is-no-single-expected-answer) summarizes what research shows about model judges.

## What happens to old results when a requirement changes?

They keep their versions. Every result names the exact Verifier and subject versions it judged, and a changed Verifier, or a changed file it mounts, becomes a new version. The [Quickstart](/get-started/quickstart/#5-update-the-policy) shows a policy update creating `release-check@0.1.1` while results from `0.1.0` still name the original policy. [Cards and versions](/understand/cards-versions-and-relationships/) explains versioning.

## Where is the evidence kept?

Each result's summary goes to `bifrost.verification.results`, its requirement outcomes and steps to `bifrost.verification.execution_outputs`, and the pinned inputs to `bifrost.verification.execution_inputs`, all queryable with SQL. The server stores them after it answers, so a row appears shortly after the call returns. An abrupt server stop before that write loses the record, though the verdict already returned stands; the loss is counted. [What Wyrd retains](/understand/how-verification-works/#what-wyrd-retains-and-how-requirements-change) explains the boundary.

## Which languages are supported?

Python, Rust, and TypeScript have first-class SDKs with the same calls. Any language can use the [HTTP API](/api/openapi/), and agents can use the [MCP tools](/reference/mcp-tools/) or the `wyrd` CLI.

## Can I run it locally or on my own infrastructure?

Yes. [Set up Wyrd](/get-started/setup/) runs a local server against your PostgreSQL 16 on macOS (Apple silicon) or Linux. [Deploy for a team](/operate/deploy-wyrd-for-a-team/) runs it on your own infrastructure with PostgreSQL and an object store.

## Next

Start with the [Quickstart](/get-started/quickstart/), or read [Foundations of verification](/understand/foundations-of-verification/) and [Real-time verification](/understand/real-time-verification/) for the ideas behind it.
