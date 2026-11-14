---
title: What Wyrd does
description: The only real-time verification system built for your applications. Your application asks Wyrd whether an action meets your requirements, and acts on the verdict.
pillar: wyrd
section: Get started
order: 1
---

**You built the system. How do you know it's behaving correctly?**

Wyrd is the only real-time verification system built for your applications. Before your application acts, it sends Wyrd what it is about to do. Wyrd judges that against the requirements you declare and returns a verdict for each one, with a reason. Your code decides what happens next, and Wyrd keeps the evidence beside the exact versions it judged.

## An example

An application shares synthetic customer reports with outside auditors. Before it sends one, it checks three requirements in one Verifier. A model reads the company sharing policy and judges the report; the other two are assertions on records the application supplies:

<CodeFromFile file="examples/finance-release/finance_service.py" region="requirements" lang="python" title="finance_service.py" />

The application verifies each release with one call:

<CodeFromFile file="examples/finance-release/finance_release.py" lang="python" title="finance_release.py" />

```text
passed: Synthetic customer report: quarterly total $120.
failed: Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.
```

The second report names a bank account, which the policy forbids. [Run your first verification](/get-started/quickstart/) runs the full program in Python, Rust, or TypeScript. A Verifier can also run Python, judge a whole session, or compare samples with a baseline on a schedule; [What can I build?](/get-started/what-can-i-build/) matches each to a problem.

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

## Where Wyrd stops

Wyrd verifies your system; your code runs it. Your app owns its code, models, and data, runs and deploys them, and decides what to do with a verdict. Wyrd is not an application runtime, model host, training framework, or general-purpose code executor.

## Next

[Set up Wyrd](/get-started/setup/) starts a local server. [Run your first verification](/get-started/quickstart/) then runs this example against it. [How verification works](/understand/how-verification-works/) explains requirements, checks, and verdicts.
