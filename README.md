<p align="center">
  <a href="https://docs.bohmian.ai/wyrd/"><img src=".github/assets/wyrd-banner.jpg" alt="Wyrd: Declare. Observe. Verify." width="100%"></a>
</p>

<p align="center">
  <a href="https://github.com/bohmian-ai/wyrd/actions/workflows/lints-test.yml"><img src="https://github.com/bohmian-ai/wyrd/actions/workflows/lints-test.yml/badge.svg" alt="CI"></a>
  <a href="https://pypi.org/project/wyrd-ai/"><img src="https://img.shields.io/pypi/v/wyrd-ai?label=PyPI" alt="PyPI"></a>
  <a href="https://www.npmjs.com/package/wyrd-ai"><img src="https://img.shields.io/npm/v/wyrd-ai?label=npm" alt="npm"></a>
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/license-Apache%202.0-blue" alt="License: Apache 2.0"></a>
  <a href="https://discord.gg/K5bf23yPNb"><img src="https://img.shields.io/badge/chat-Discord-5865F2" alt="Discord"></a>
</p>

<p align="center">
  <a href="https://docs.bohmian.ai/wyrd/"><b>Documentation</b></a> ·
  <a href="https://docs.bohmian.ai/wyrd/get-started/quickstart/">Quickstart</a> ·
  <a href="https://discord.gg/K5bf23yPNb">Discord</a>
</p>

## The only real-time verification system built for your applications.

**You built the system. How do you know it's behaving correctly?**

Wyrd tells you while it runs. Before your application acts, it sends Wyrd what it is about to do. Wyrd judges that against the requirements you declare and returns a verdict for each one, with a reason. Your code decides what happens next, and Wyrd keeps the evidence beside the exact versions it judged.

Wyrd is open source. You build the verification your system needs from three steps: **Declare** what you ship and what it must do, **Observe** what it does, and **Verify** it.

## Example

An application shares synthetic customer reports with outside auditors. Before it sends one, it checks three requirements: the recipient is authorized, the company sharing policy permits the release, and the destination restricts access. A model judges the policy; the other two are assertions on records the application supplies.

<!-- example: examples/finance-release/finance_service.py region=requirements -->
```python
requirements=[
    Requirement(
        "recipient-authorized",
        "The recipient is authorized to receive the report.",
        Check.assert_("inputs.release.recipient.authorized", ComparisonOperator.Equals, True),
    ),
    Requirement(
        "follows-policy",
        "The company sharing policy permits this release.",
        follows_policy,
    ),
    Requirement(
        "destination-restricted",
        "The destination restricts access to the report.",
        Check.assert_("inputs.release.destination.restricted", ComparisonOperator.Equals, True),
    ),
],
```

The application verifies each release with one call:

<!-- example: examples/finance-release/finance_release.py -->
```python
from wyrd import WyrdState

from finance_service import Destination, Recipient, Release, Report, service

# Upload the policy, declare every Card, and get a ready state.
state = WyrdState.declare(service)
run = state.run()

recipient = Recipient(email="auditor@example.test", authorized=True)
destination = Destination(url="https://auditor.example.test/reports", restricted=True)
for text in (
    "Synthetic customer report: quarterly total $120.",
    "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.",
):
    release = Release(report=Report(text=text), recipient=recipient, destination=destination)
    result = run.observe.verify("release-check", release)
    print(f"{result.verdict}: {text}")

state.shutdown()
```

```text
passed: Synthetic customer report: quarterly total $120.
failed: Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.
```

The full example is in [`examples/finance-release`](https://github.com/bohmian-ai/wyrd/tree/main/examples/finance-release) for Python, Rust, and TypeScript. [Run your first verification](https://docs.bohmian.ai/wyrd/get-started/quickstart/) runs it against your own server and model.

## What you can build

The same primitives build other verification. Start from a use case:

<p align="center">
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#policy-verification"><img src="https://img.shields.io/badge/Policy%20verification-555555" alt="Use case: Policy verification"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#agent-evaluation"><img src="https://img.shields.io/badge/Agent%20evaluation-555555" alt="Use case: Agent evaluation"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#long-horizon-workflows"><img src="https://img.shields.io/badge/Long--horizon%20workflows-555555" alt="Use case: Long-horizon workflows"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#runtime-verification"><img src="https://img.shields.io/badge/Runtime%20verification-555555" alt="Use case: Runtime verification"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#observability"><img src="https://img.shields.io/badge/Observability-555555" alt="Use case: Observability"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#drift-monitoring"><img src="https://img.shields.io/badge/Drift%20monitoring-555555" alt="Use case: Drift monitoring"></a>
</p>

[What can I build?](https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/) walks through each one.

## How Wyrd fits your stack

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/wyrd-stack-dark.svg">
  <img src=".github/assets/wyrd-stack-light.svg" alt="Your app, CI, and your coding agent call one Wyrd server. CI and your app declare Services and Verifiers; your app verifies against the verification engine and sends telemetry, its own data, and evidence to Bifrost; your coding agent reads evidence over MCP. The server keeps declarations in Postgres and evidence in object storage." width="100%">
</picture>

1. **Declare.** Describe what you ship in YAML and bind Verifiers to it, from CI with `wyrd declare` or from your code. Each declaration has a version, so a changed prompt is a new version.
2. **Observe.** Your app sends Bifrost, Wyrd's data plane, everything it produces as it runs: telemetry, its own data, and the evidence it submits to verify. Bifrost keeps all of it queryable with SQL beside every verdict.
3. **Verify.** Your app sends a result to a Verifier and gets a verdict for each requirement, with a reason.

The Python, Rust, and TypeScript SDKs, the CLI, MCP, and HTTP all call one server, so a verdict reads the same from each. [Architecture](https://docs.bohmian.ai/wyrd/understand/architecture/) shows what a team deployment runs.

## Why Wyrd

Most AI tools hand you their evaluators, their UI, and their datastore. Your checks end up shaped by what the product supports, and your evidence lives where the product keeps it.

Wyrd gives you the primitives instead, and you build the verification layer your system needs: a code reviewer that reads the diff and your style guide, a payment check that cites your policy document, drift on a classical model, a check across a six-hour agent run.

| You get | So you don't have to |
| --- | --- |
| Verification primitives you compose into steps and checks | Fit your rules into one vendor's evaluator types |
| Checks as versioned YAML in your repo, run in CI, inline, in the background, per session, or on a schedule | Keep one set of evals for CI and another for production |
| One data plane for telemetry, model calls, your app's rows, and verdicts, as Iceberg tables in your object store, queried with SQL | Join evidence across four tools' stores and APIs |
| Registration and versioning for the whole service: agents, prompts, models, data, MCP servers | Guess which version produced a result |
| An LLM gateway and service identity, so every call and verdict names the account and release behind it | Bolt attribution on after the fact |
| Apache 2.0 and self-hosted, ingesting standard OpenTelemetry (OTLP) from any exporter | Re-instrument your app with a vendor SDK, or send your data to someone else's cloud |

## Install

You need PostgreSQL 16 and one of Python 3.11, Rust 1.95, or Node.js 22.18, on macOS (Apple silicon) or Linux.

```bash
# Python
uv add "wyrd-ai==0.4.1"

# Rust
cargo install wyrd-cli --version 0.4.1 --locked
cargo add wyrd-ai@0.4.1 tokio serde schemars --features tokio/full,serde/derive

# TypeScript
pnpm add wyrd-ai@0.4.1 zod
```

Start a local server with `wyrd init` (`uv run wyrd init` or `pnpm wyrd init`), or let your coding agent do it with `wyrd init guided`. [Set up Wyrd](https://docs.bohmian.ai/wyrd/get-started/setup/) has the details.

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

Wyrd verifies your system; your code runs it. Your app owns its code, models, and data, and decides what to do with a verdict. Wyrd is not an application runtime, model host, training framework, or general-purpose code executor.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before a substantial change. Ask questions on [Discord](https://discord.gg/K5bf23yPNb), and report vulnerabilities privately through [SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE.md).
