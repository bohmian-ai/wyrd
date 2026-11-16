---
title: What Wyrd does
description: The only real-time verification system built for your applications. Your application asks Wyrd whether an action meets your requirements, and acts on the verdict.
pillar: wyrd
section: Get started
order: 1
---

**You built the system. How do you know it's behaving correctly?**

Every system has its own requirements, workflows, and definitions of correct behavior. Turning them into repeatable checks usually means stitching together evaluation frameworks, observability tools, custom scripts, and data pipelines.

Wyrd gives you open-source building blocks for your own verification systems. **Declare** requirements. **Observe** real behavior. **Verify** live, during execution, or continuously over time.

Live verification puts the verdict before the decision. Your application asks Wyrd whether what it is about to do meets your requirements, gets an evidence-backed verdict for each one, and decides what happens next. The same verdicts are available from queued and continuous checks.

## An example

An application shares synthetic customer reports with outside auditors. Before it sends one, it checks three requirements in one Verifier. A model reads the company sharing policy PDF and judges the report and its purpose; the other two are assertions on records the application supplies:

<CodeFromFile file="examples/finance-release/finance_service.py" lang="python" title="finance_service.py" />

The application verifies each release with one call and releases it only on `passed`:

<CodeFromFile file="examples/finance-release/finance_release.py" lang="python" title="finance_release.py" />

<CodeFromFile file="examples/finance-release/output.txt" lang="text" title="output" />

The judge's wording varies. The policy forbids bank account identifiers and permits sharing only for external audits. [Run your first verification](/get-started/quickstart/) runs the full program in Python, Rust, or TypeScript. A Verifier can also run Python, judge a whole session, or compare samples with a baseline on a schedule; [What can I build?](/get-started/what-can-i-build/) matches each to a problem.

## How Wyrd fits your stack

<WyrdDiagram name="wyrd-in-your-stack" />

1. **Declare.** Describe what you ship in YAML (a Service and its Agents, Models, and Data) and bind Verifiers to any of them. Declare them from CI with `wyrd declare` or from your code. Each declaration has a version, so a changed prompt is a new version.
2. **Observe.** Your app sends Bifrost, Wyrd's data plane, everything it produces as it runs: telemetry, its own data, and the evidence it submits to verify. Bifrost keeps all of it queryable with SQL beside every verdict.
3. **Verify.** Your app sends a result to a Verifier and gets a verdict for each requirement, with a reason. Wyrd keeps every verdict with the exact versions it judged.

Your coding agent reads the same declarations, verdicts, and evidence over MCP. The Python, Rust, and TypeScript SDKs, the CLI, MCP, and HTTP all call one server, so a verdict reads the same from each. [Architecture](/understand/architecture/) shows what a team deployment runs.

## Already have tools?

Keep them. Add Wyrd where they stop.

| You use | Keep it for | Add Wyrd for |
| --- | --- | --- |
| MLflow, W&B | Training, experiments, model registry | Runtime monitoring and verification of the model you deployed, at its exact version |
| Langfuse, Phoenix, LangSmith | Trace exploration, prompt iteration | Your own verification layer: checks you compose, judges with tools and your files, verdicts your app acts on |
| promptfoo, DeepEval, Ragas | Test suites and red-teaming in CI | Running the same checks on live traffic, and keeping every result |
| Evidently, Fiddler | Drift reports and dashboards | Drift judged next to traces, model calls, and app data, in one SQL store |
| LiteLLM, Portkey | Model routing, inline guardrails | Tying every call to an account, a release, and a verdict |
| Datadog, Grafana, Honeycomb | Infrastructure monitoring and alerts | Pass/fail verdicts on the same OpenTelemetry data |
| Credo AI, watsonx.governance | AI inventory, policy programs | Proof from production for each control |
| Guardrails AI, NeMo Guardrails | Predefined input/output filters | Your own requirements, judged with your evidence and files, returned to your code to decide |

## Where Wyrd stops

Wyrd verifies your system; your code runs it. Your app owns its code, models, and data, runs and deploys them, and decides what to do with a verdict. Wyrd is not an application runtime, model host, training framework, or general-purpose code executor. Wyrd returns verdicts; it never blocks, retries, or reroutes your application. Your code decides.

## Next

[Set up Wyrd](/get-started/setup/) starts a local server. [Run your first verification](/get-started/quickstart/) then runs this example against it. [Foundations of verification](/understand/foundations-of-verification/) explains requirements, evidence, and judgments without Wyrd, [Real-time verification](/understand/real-time-verification/) explains checking a system while it runs, and [How verification works in Wyrd](/understand/how-verification-works/) shows how Wyrd places a verdict before your application acts.
