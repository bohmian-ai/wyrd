---
title: Act on verdicts in production
description: Handle passed, failed, inconclusive, errors, and timeouts explicitly, decide what model providers may see, and measure latency and cost for the Verifier you deploy.
pillar: wyrd
section: Build and verify
topic: Verify
order: 6
---

Decide what your application does with every outcome of a verification before it reaches production. The examples use the finance API from [How verification works in Wyrd](/understand/how-verification-works/#one-verification-end-to-end).

Each practice names the failure it prevents and where it stops helping.

## 1. Release only what passed

**Prevents:** an inconclusive verdict, an error, or a timeout treated as permission.

The finance handler releases a report only when the verdict is `passed`:

<CodeFromFile file="examples/finance-release/api.py" lang="python" title="api.py" region="handler" />

Testing for failure instead, with `if not result.failed`, releases every inconclusive report: the ones whose evidence was missing or whose judge could not decide. Decide each outcome on purpose. With the default `decide: all`:

| Outcome | Means | Finance API |
| --- | --- | --- |
| `passed` | Every applicable requirement passed | Release |
| `failed` | At least one requirement was judged broken | Hold, and show the failing requirement's reason |
| `inconclusive` | No judgment was possible for some requirement | Hold for a person |
| An error, such as invalid input or `WYRD_VERIFICATION_429_STAGING_FULL` | Nothing was judged | Hold; retry a staging refusal after `retry_after_seconds` |
| `WYRD_SERVER_503_SERVICE_UNAVAILABLE` | No Python worker or workspace was free | Hold, or retry |
| `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` | No verdict within the timeout | Hold, or `submit` and decide later |

In Python, `verify` raises on an error or timeout, so the handler above returns an error to its caller and releases nothing. Catch the error when the caller needs a deliberate response instead.

**Limits:** holding every non-pass is safe for a report, but can be wrong for an action that is costly to delay. The right fallback depends on the action; Wyrd reports the outcome and leaves the choice to you.

## 2. Fit checks to the request's deadline

**Prevents:** a request that waits on a judgment it cannot afford.

A `verify` call is bounded by `WYRD_VERIFICATION_TIMEOUT_MS`, 60 seconds by default, and the SDK does not retry it. Keep checks that must decide before the response on the request path, and move the rest:

- Assertions and code checks add no model call. A code check can wait for a free Python worker, and is refused with `WYRD_SERVER_503_SERVICE_UNAVAILABLE` when none is available.
- A model judge adds at least one model call. Keep it on the request path only when its measured time fits, as the finance policy judge must because the release cannot happen without it.
- Move checks that can be decided after the response into a separate Verifier, and `submit` it after you respond. `submit` returns once the inputs are staged.

Measure the Verifier you deploy: time the `verify` call in your application on representative evidence, and read each step's `duration_ms` in the execution's details. [Where the time goes](/understand/how-verification-works/#where-the-time-goes) explains what the total includes.

**Limits:** a measurement holds for the evidence, model, provider, and load you measured. Remeasure after changing any of them.

## 3. Decide what a provider may see

**Prevents:** sensitive evidence sent to a model provider without anyone choosing to send it.

An `agent` check sends its prompt and the evidence the prompt includes to the provider, through your server's [LLM gateway](/connect/set-up-the-llm-gateway/). Files the Agent reads with a tool are sent too. In the finance example, the purpose, the report, the recipient record, and the policy text all reach OpenAI.

- Pass a model check only the fields it needs. `follows-policy` takes the purpose, report, and recipient, not the destination.
- Decide a requirement with an `assert` or `code` check when one can; those send nothing to a provider.
- Use synthetic data until the provider's data handling meets your requirements.
- Grant each tool the Agent calls explicitly; declaring a tool grants nothing. [Agent tools](/reference/agent-tools-and-authorization/) lists the grants.

**Limits:** Wyrd controls what the server sends, not what the provider does with it. Provider retention and training terms are between you and the provider.

## 4. Account for model cost

**Prevents:** a verification bill that grows with traffic unnoticed.

Each `agent` check costs at least one model call per verification, and more when it calls tools: the finance judge allows up to three model calls (`max_iterations: 3`). Assertions and code checks cost no model call. Turn on metadata recording, as in [LLM gateway step 3](/connect/set-up-the-llm-gateway/#3-turn-on-recording), to see each call's model and token usage without the prompt or reply. The policy applies to every gateway call in the tenant.

**Limits:** token counts measure what you spent, not whether the judge was worth it. Weigh cost against what a missed failure costs.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` on the request path | Move the slow check into a Verifier you `submit` after responding |
| `WYRD_VERIFICATION_429_STAGING_FULL` | Retry after `retry_after_seconds`; hold the action meanwhile |
| Inconclusive reports were released | Test for `passed`, not for the absence of `failed` |

## Next

[Technical questions](/understand/technical-questions/) answers common adoption questions, and the [Verifier Card reference](/reference/verifier-card/) lists every field.
