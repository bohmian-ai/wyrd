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

# You built the system. How do you know it's behaving correctly?

Verifying application behavior shouldn't mean building your own infrastructure from scratch.

Every system has its own requirements, workflows, and definitions of correct behavior. But turning those expectations into repeatable checks often means stitching together evaluation frameworks, observability tools, custom scripts, and data pipelines. Your team ends up maintaining the plumbing as well as the requirements that matter to your application.

**Wyrd provides open-source building blocks for creating your own verification systems.** Compose the checks, evidence, and workflows your application needs, from local development through production.

## Declare. Observe. Verify.

**Declare requirements. Observe real behavior. Verify the evidence.**

1. **Declare.** Describe your Services, components, and Verifiers in versioned YAML. Bind reusable checks to the exact versions they verify.
2. **Observe.** Record what happens during a Run: application data, telemetry, model calls, and the evidence your checks need.
3. **Verify.** Evaluate that evidence against your requirements. Get a verdict for each requirement, with a reason, tied to the exact subject and Verifier versions.

Your application decides when to verify and what to do with the result. Wyrd provides the verification engine, versioning, and evidence storage so you can focus on defining correct behavior.

## What can you build?

- **Development verification workflows.** Check application and agent behavior before release, then reuse the same Verifiers on live traffic.
- **Agent verification.** Combine checks on individual actions with trace checks and grouped-session verification for multi-step workflows.
- **Custom evaluation platforms.** Compose deterministic assertions, Python checks, model judgments, and domain-specific requirements.
- **Continuous production verification.** Check requests inline, submit checks in the background, or monitor drift on a schedule. Investigate failures using the recorded execution evidence.
- **Team verification standards.** Define, version, and reuse verification requirements across Services and applications.

Build the verification system you need. Start with a workflow:

<p align="center">
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#policy-verification"><img src="https://img.shields.io/badge/Policy%20verification-555555" alt="Use case: Policy verification"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#agent-evaluation"><img src="https://img.shields.io/badge/Agent%20evaluation-555555" alt="Use case: Agent evaluation"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#long-horizon-workflows"><img src="https://img.shields.io/badge/Long--horizon%20workflows-555555" alt="Use case: Long-horizon workflows"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#runtime-verification"><img src="https://img.shields.io/badge/Runtime%20verification-555555" alt="Use case: Runtime verification"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#observability"><img src="https://img.shields.io/badge/Observability-555555" alt="Use case: Observability"></a>
  <a href="https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/#drift-monitoring"><img src="https://img.shields.io/badge/Drift%20monitoring-555555" alt="Use case: Drift monitoring"></a>
</p>

[What can I build?](https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/) explains these compositions, and [Recipes](https://docs.bohmian.ai/wyrd/build-and-verify/verify/verifier-recipes/) provides Verifier examples.

## Example: verify a support answer against your refund policy

The existing [support-desk example](examples/support-desk/) shows how the building blocks fit together in a working application, with Python, Rust, and TypeScript implementations:

1. **Declare the requirement.** Bind a `policy-check` Verifier to the support Agent. An Agent step reads the refund-policy PDF, a deterministic assertion rejects the phrase "guaranteed refund", and a model judge checks the answer against the policy findings, citing the policy page.
2. **Observe the request.** The application records a ticket, a request span, and its gateway model call under the same Run.
3. **Verify the answer.** Use `verify` for an immediate verdict or `submit` for a background check. The example's answer "You have a guaranteed refund." fails the deterministic requirement; each requirement has its own outcome and reason.
4. **Investigate with evidence.** Query Bifrost to join the ticket, span, model call, and verdict for that Run. The application can use the result to decide whether to accept, hold, or escalate an answer.

The application owns the support workflow and its policy. Wyrd executes the checks and retains the evidence. [Read and run the example](examples/support-desk/README.md) after configuring its server and `gpt-4o` deployment, or start with the smaller, model-free quickstart below.

## Quickstart

<a id="install"></a>

### Install and start a local server

You need PostgreSQL 16 and one of Python 3.11, Rust 1.95, or Node.js 22.18, on macOS (Apple silicon) or Linux.

```bash
# Python
uv add "wyrd-ai==0.4.1"

# Rust
cargo install wyrd-cli --version 0.4.1 --locked
cargo add wyrd-ai@0.4.1 tokio --features tokio/full

# TypeScript
pnpm add wyrd-ai@0.4.1
```

Start a local server with `wyrd init` (`uv run wyrd init` or `pnpm wyrd init`), or let your coding agent do it with `wyrd init guided`. [Set up Wyrd](https://docs.bohmian.ai/wyrd/get-started/setup/) has the details.

<a id="example"></a>

### Declare and verify your first requirement

Declare one requirement: a support answer never promises a guaranteed refund.

<!-- example: examples/quickstart/no-refund-promise.yaml -->
```yaml
# The Verifier: a support answer never promises a guaranteed refund.
apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: no-refund-promise
  version: 1.0.0
spec:
  inputs:
    answer: { type: json }
  requirements:
    - id: no-refund-promise
      statement: The answer never promises a guaranteed refund
      check:
        kind: assert
        path: inputs.answer
        operator: not_contains
        expected: guaranteed refund
```

Bind it to the Service it checks:

<!-- example: examples/quickstart/support-reply.yaml -->
```yaml
# The subject: your support agent's Service, verified by no-refund-promise.
apiVersion: wyrd/v1
kind: Service
metadata:
  name: support-reply
  version: 1.0.0
spec:
  verified_by:
    - verifier: ./no-refund-promise.yaml
```

Then verify two answers:

<details open>
<summary><b>Python</b></summary>

<!-- example: examples/quickstart/quickstart.py -->
```python
from wyrd import WyrdState

# Declare the Service and the Verifier it references, and get a ready state.
state = WyrdState.declare("support-reply.yaml")
run = state.run()

for answer in ("Your refund request is approved.", "You have a guaranteed refund."):
    result = run.observe.verify("no-refund-promise", answer=answer)
    print(f"{result.verdict}: {result.requirements[0]['reason']}")
    print(f"  subject:  {result.subject}")
    print(f"  verifier: {result.verifier}")
```

</details>

<details>
<summary><b>Rust</b></summary>

<!-- example: examples/quickstart/quickstart.rs -->
```rust
//! The docsite Quickstart in Rust: declare a Service and the Verifier it
//! references, then judge a passing and a failing answer.

use wyrd_ai::WyrdError;
use wyrd_ai::serde_json::json;
use wyrd_ai::state::WyrdState;

/// Declares `support-reply.yaml` from the working directory, verifies two
/// answers against `no-refund-promise`, and prints each verdict with its reason
/// and the exact subject and Verifier identities.
///
/// # Errors
///
/// Returns the [`WyrdError`] of a refused declaration or verification.
#[tokio::main]
async fn main() -> Result<(), WyrdError> {
    // Declare the Service and the Verifier it references, and get a ready state.
    let state = WyrdState::declare("support-reply.yaml").await?;
    let run = state.run();

    for answer in [
        "Your refund request is approved.",
        "You have a guaranteed refund.",
    ] {
        let result = run
            .observe()
            .verify("no-refund-promise", json!(answer))
            .await?;
        let reason = &result.outcome.requirements[0].reason;
        println!("{}: {reason}", result.outcome.verdict);
        println!("  subject:  {}", result.subject);
        println!("  verifier: {}", result.verifier);
    }
    Ok(())
}
```

</details>

<details>
<summary><b>TypeScript</b></summary>

<!-- example: examples/quickstart/quickstart.ts -->
```typescript
import { type CardRef, WyrdState } from "wyrd-ai";

// Declare the Service and the Verifier it references, and get a ready state.
const state = await WyrdState.declare("support-reply.yaml");
const run = state.run();

const id = (ref: CardRef) => `${ref.kind}/${ref.name}@${ref.version}#${ref.uid}`;
for (const answer of ["Your refund request is approved.", "You have a guaranteed refund."]) {
  const result = await run.observe.verify("no-refund-promise", { answer });
  console.log(`${result.outcome.verdict}: ${result.outcome.requirements[0].reason}`);
  console.log(`  subject:  ${id(result.subject)}`);
  console.log(`  verifier: ${id(result.verifier)}`);
}
```

</details>

Every language prints one passing and one failing verdict, each naming the exact versions it checked:

```text
passed: the comparison held
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
failed: operator NotContains returned false
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
```

[Run your first verification](https://docs.bohmian.ai/wyrd/get-started/quickstart/) walks through it. This assertion is the simplest check. A Verifier can also run Python, judge with an Agent that reads your uploaded policy files and cites them, judge a whole session, or watch for drift on a schedule; see [What can I build?](https://docs.bohmian.ai/wyrd/get-started/what-can-i-build/) and [Recipes](https://docs.bohmian.ai/wyrd/build-and-verify/verify/verifier-recipes/).

## Architecture and technical details

### How Wyrd fits your stack

<picture>
  <source media="(prefers-color-scheme: dark)" srcset=".github/assets/wyrd-stack-dark.svg">
  <img src=".github/assets/wyrd-stack-light.svg" alt="Your app, CI, and your coding agent call one Wyrd server. CI and your app declare Services and Verifiers; your app verifies against the verification engine and sends telemetry, its own data, and evidence to Bifrost; your coding agent reads evidence over MCP. The server keeps declarations in Postgres and evidence in object storage." width="100%">
</picture>

The Python, Rust, and TypeScript SDKs, CLI, MCP, and HTTP all call one server. Declare from CI with `wyrd declare` or from your code, record executions, and retrieve verdicts through the same API contracts.

- **Verification engine.** Executes the steps and checks you compose into Verifiers, with results tied to versioned declarations.
- **Bifrost OLAP warehouse.** Built on DataFusion and Apache Iceberg, Bifrost stores and queries your application's own tables alongside telemetry, model calls, and verification evidence. It provides read-only SQL over those tables in your object store, with a Postgres catalog. See [Bifrost](https://docs.bohmian.ai/wyrd/understand/bifrost/).
- **LLM gateway.** Provides access to configured model-provider deployments for application calls and model-judged checks.
- **Identity and authorization.** Service accounts, roles, and tenant boundaries control access and attribute activity to its caller.

Postgres holds declarations, identities, and coordination state; object storage holds evidence, artifacts, and analytical tables. [Architecture](https://docs.bohmian.ai/wyrd/understand/architecture/) explains a team deployment.

<a id="why-wyrd"></a>

### Building blocks you can reuse

| You get | So you don't have to |
| --- | --- |
| Verification primitives you compose into steps and checks | Fit your rules into one vendor's evaluator types |
| Checks as versioned YAML in your repo, run in CI, inline, in the background, per session, or on a schedule | Keep one set of evals for CI and another for production |
| One data plane for telemetry, model calls, your app's rows, and verdicts, as Iceberg tables in your object store, queried with SQL | Join evidence across four tools' stores and APIs |
| Registration and versioning for the whole service: agents, prompts, models, data, MCP servers | Guess which version produced a result |
| An LLM gateway and service identity, so every call and verdict names the account and release behind it | Bolt attribution on after the fact |
| Apache 2.0 and self-hosted, ingesting standard OpenTelemetry (OTLP) from any exporter | Re-instrument your app with a vendor SDK, or send your data to someone else's cloud |

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
