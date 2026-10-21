---
title: What Wyrd verifies
description: Wyrd judges what an AI system did against criteria you declared, and ties every judgment to the exact versions involved.
pillar: wyrd
section: Get started
order: 1
---

Wyrd is open-source verification and assurance infrastructure for AI systems. You declare what a part of your system must do. Wyrd judges evidence of what it did against that declaration and returns a verdict. Every verdict names the exact versions of the system and of the check that produced it.

Verification is the judgment: did this behavior meet the declared criteria? Assurance is what accumulates from it: durable evidence that lets you show later which version was checked, against which criteria, with what result.

## From declaration to judgment

| You declare | You supply | Wyrd returns |
| --- | --- | --- |
| A Verifier: the criteria, such as "the refund is at most 100 dollars" | Evidence of one behavior, as JSON | `passed` or `failed`, with a reason for each criterion |
| A subject, such as a Service, that the Verifier is bound to | | The exact subject and Verifier identities behind the verdict |

Declarations are Cards: small versioned YAML documents. A Card's name and version always mean the same content; changing the content needs a new version. That is what makes a verdict attributable: it points at one fixed declaration, not whatever the file says today.

A `failed` verdict is a successful answer. The check ran and the behavior did not meet the criteria. A request Wyrd cannot carry out, such as one without permission, is refused with an error instead, and produces no verdict.

## Where Wyrd stops

Wyrd judges your system; it does not run it. Your model, agent, or service runs in your own runtime, and your code sends Wyrd the evidence to judge. Wyrd is not an application runtime, model host, training framework, or general-purpose code executor.

| Your application owns | The Wyrd server owns |
| --- | --- |
| Your code, models, and data | Cards and their versions |
| Running and deploying them | Verdicts and the evidence behind them |
| Deciding what to do with a verdict | Who may declare and verify, and the audit of those decisions |

The Python, Rust, and TypeScript SDKs, the CLI, MCP, and HTTP all call the same server, so a verdict reads the same from each.

## Next

[Set up Wyrd](/get-started/setup/) starts a local server. [Run your first verification](/get-started/quickstart/) then declares a Verifier and judges one passing and one failing reply.
