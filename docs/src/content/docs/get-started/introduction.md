---
title: What Wyrd verifies
description: Wyrd checks that an AI system behaves the way its declaration says it should, before and after deployment.
pillar: wyrd
section: Get started
order: 1
---

Wyrd is open-source verification and assurance infrastructure for AI systems. You declare what an AI system should do, record what it actually does, and Wyrd returns a verdict on whether the two match. The system can be an individual LLM, a deployed agent service, or a traditional HTTP service, and the same declaration is checked before release and in production.

This page explains that loop and where Wyrd stops. To run it yourself, start with [Set up Wyrd](/get-started/setup/).

## The verification loop

Wyrd works in three steps, and each one leaves a durable record the next step builds on.

| Step | You provide | Wyrd records |
| --- | --- | --- |
| Declare | A Service declaration: its components and the Verifiers bound to check them | Versioned Cards in the registry |
| Observe | Your service's behavior: eval and drift observations, telemetry, and custom data | Rows in Bifrost, the Wyrd data plane |
| Verify | Nothing extra for triggered checks; the declaration says what is checked, when, and what happens on failure. Real-time checks are one call from your code | A verdict tied to the exact versions that ran, plus any failure actions the declaration names |

**Declare.** You declare a whole service in YAML, independent of the language it is written in. The declaration is a set of Cards. A Card is a versioned declaration of one part of your system, or of how that part is checked. See [Cards, versions, and relationships](/understand/cards-versions-and-relationships/).

**Observe.** Observe is how you record what the declared system actually does. Observations go straight into Bifrost, the Wyrd data plane. See [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/).

**Verify.** A Verifier judges observed behavior against a standard. Wyrd runs it for you when its Trigger fires, on a schedule or as observations arrive, and stores the verdict; a failed verdict runs the binding's Operators. Your code can also call a Verifier directly while a request is in flight and act on the answer. See [How verification works](/understand/how-verification-works/).

## Where Wyrd stops

Wyrd judges your AI system; it does not run it. Your model, agent, or HTTP service keeps running in your own runtime and calls Wyrd to record what it did. Wyrd is not an application runtime, training framework, workflow engine, or general-purpose code executor.

| Your runtime owns | The Wyrd server owns |
| --- | --- |
| Your application code | Cards and their versions |
| Running and deploying the service | Observations and the queries over them |
| The service's Wyrd API key, kept in your secret store | Verdicts, failure actions, policy decisions, and the audit log |

Your service calls Wyrd as itself. Registering a Service or Agent gives it a principal: an identity in your tenant with its own permissions. It may record observations only about the Cards in its own declaration, and every observation and verdict names the principal that produced it. See [Identity and authorization](/understand/identity-and-authorization/).

The SDKs, the CLI, MCP, and HTTP all call the same server, so a verdict reads the same from Python, from Rust, or from an agent over MCP.

The one exception to this split is the AI Gateway, an optional proxy for your model and provider calls that also manages provider credentials and usage. If you route calls through it, Wyrd sits in your runtime's call path and records every call itself, not only the observations your code sends.
