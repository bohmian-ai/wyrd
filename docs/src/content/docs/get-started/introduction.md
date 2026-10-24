---
title: What Wyrd does
description: Wyrd verifies what your AI system does against the requirements you declare, and records which versions produced each result.
pillar: wyrd
section: Get started
order: 1
---

You ship a support agent, a Q&A bot, or a model behind an API. Wyrd checks what it says against the rules you set, such as "never promise a refund" or "answer from the policy document", before you release a change and while it serves real traffic. When a rule fails, Wyrd tells you which rule, why, and which version of your system broke it.

## How Wyrd fits your stack

<WyrdDiagram name="wyrd-in-your-stack" />

1. **Declare.** Describe what you ship in YAML (a Service and its Agents, Models, and Data) and bind Verifiers to any of them. Declare them from CI with `wyrd declare` or from your code. Each declaration has a version, so a changed prompt is a new version.
2. **Observe.** Your app sends Bifrost, Wyrd's data plane, everything it produces as it runs: telemetry, its own data, and the evidence it submits to verify. Bifrost keeps all of it queryable with SQL beside every verdict.
3. **Verify.** Your app sends a result to a Verifier and gets a verdict for each requirement, with a reason. Wyrd keeps every verdict with the exact versions it judged.

Your coding agent reads the same declarations, verdicts, and evidence over MCP. The Python, Rust, and TypeScript SDKs, the CLI, MCP, and HTTP all call one server, so a verdict reads the same from each. [Architecture](/understand/architecture/) shows what a team deployment runs.

## An example

A support agent must never promise a guaranteed refund. The Service describes the agent and binds a Verifier, which states that requirement as an assertion:

<Tabs>
<Tab label="support-reply.yaml">

<CodeFromFile file="examples/quickstart/support-reply.yaml" lang="yaml" title="support-reply.yaml" />

</Tab>
<Tab label="no-refund-promise.yaml">

<CodeFromFile file="examples/quickstart/no-refund-promise.yaml" lang="yaml" title="no-refund-promise.yaml" />

</Tab>
</Tabs>

This program declares both and verifies two answers:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/quickstart/quickstart.py" lang="python" title="quickstart.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/quickstart/quickstart.rs" lang="rust" title="src/main.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/quickstart/quickstart.ts" lang="typescript" title="quickstart.ts" />

</LangTab>
</LangTabs>

```text
passed: the comparison held
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
failed: operator NotContains returned false
  subject:  Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
```

In CI, `wyrd declare support-reply.yaml` declares the same files from the command line. A check can also ask a model to judge, such as whether a letter follows your refund policy.

## Where Wyrd stops

Wyrd verifies your system; your code runs it. Your app owns its code, models, and data, runs and deploys them, and decides what to do with a verdict. Wyrd is not an application runtime, model host, training framework, or general-purpose code executor.

## Next

[Set up Wyrd](/get-started/setup/) starts a local server. [Run your first verification](/get-started/quickstart/) then declares a Verifier and verifies one passing and one failing reply.
