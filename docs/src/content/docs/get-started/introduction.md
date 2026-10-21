---
title: What Wyrd does
description: Wyrd verifies what your AI system does against the requirements you declare, and records which versions produced each result.
pillar: wyrd
section: Get started
order: 1
---

Your agent, model, or service needs to behave correctly, both before you ship a change and while it runs. Wyrd verifies that. You declare your system and the requirements it must meet. Wyrd checks each result against them, tells you which requirements failed and why, and records which version of your system produced it.

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
  subject:  default/Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
failed: operator NotContains returned false
  subject:  default/Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/no-refund-promise@1.0.0#<uid>
```

In CI, `wyrd declare support-reply.yaml` declares the same files from the command line. A check can also ask a model to judge, such as whether a letter follows your refund policy.

## How it fits your system

1. **Declare.** In YAML, describe your Service and the parts it is built from, such as an Agent, a Prompt, and a Model. Bind Verifiers to those parts, and declare them from your code or the CLI. Every declaration has a version, so changing your prompt means a new version.
2. **Verify.** Send each result to Wyrd. Wait for the verdict, or let Wyrd verify in the background while your system keeps serving.
3. **Read the verdicts.** Each requirement passes or fails with a reason. Wyrd keeps every verdict with the exact versions involved, so you can later ask which results failed after a prompt change.

## Where Wyrd stops

Wyrd verifies your system; your code runs it. Wyrd can route your model calls through its gateway, but your code owns the agent and decides what to do with a verdict. Wyrd is not an application runtime, model host, training framework, or general-purpose code executor.

| Your application owns | The Wyrd server owns |
| --- | --- |
| Your code, models, and data | Your declarations and their versions |
| Running and deploying them | Verdicts and the evidence behind them |
| Deciding what to do with a verdict | Who may declare and verify, and the audit of those decisions |

The Python, Rust, and TypeScript SDKs, the CLI, MCP, and HTTP all call the same server, so a verdict reads the same from each.

## Next

[Set up Wyrd](/get-started/setup/) starts a local server. [Run your first verification](/get-started/quickstart/) then declares a Verifier and verifies one passing and one failing reply.
