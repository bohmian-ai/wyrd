---
title: Establish sufficient evidence
description: Give each requirement evidence from its source, with citations, exact versions, and complete inputs, and tell missing evidence apart from a failure.
pillar: wyrd
section: Build and verify
topic: Observe
order: 3
---

Give each requirement the evidence that settles it, and make sure a gap in the evidence comes back as a gap rather than a verdict. The examples use the finance release from the [Quickstart](/get-started/quickstart/).

Each practice names the failure it prevents and where it stops helping. Start from requirements written as in [Write verifiable requirements](/build-and-verify/declare/write-verifiable-requirements/).

## 1. Take evidence from its source

**Prevents:** a verdict on what the system says it did, instead of what it did.

Pass the record the claim is about, not a description of it:

| Requirement | Avoid | Pass instead |
| --- | --- | --- |
| `recipient-authorized` | The assistant's reply: "I confirmed the auditor is authorized" | The authorization record your application looked up |
| `follows-policy` | A summary of the policy in the prompt | The policy file, mounted at an exact version |
| `destination-restricted` | The destination's name | The destination's access settings |

In the finance example the recipient and destination records come from the application, and the policy comes from the Artifact the Verifier version names.

**Limits:** a source record can itself be wrong or stale. Verification checks the evidence against the requirement; it does not audit the system that produced the record.

## 2. Require citations from judges

**Prevents:** a model judgment nobody can audit.

A model judge returns citations: the input slot and the location in it that its decision rests on, such as a page of the policy. Wyrd checks each citation against the pinned evidence, and a judge that passes without a valid citation makes the requirement `inconclusive`. Citations name the original input, never a scratch file the judge wrote while working. [Verdicts and citations](/reference/verifier-card/#verdicts-and-citations) has the citation format.

Read a sample of citations when you test a judge. A citation to the right page with the wrong reasoning is still a wrong judgment.

**Limits:** a valid citation shows where the judge looked, not that it read the passage correctly.

## 3. Pin exact versions

**Prevents:** a verdict that cannot be traced to the policy, Verifier, or subject it judged.

Every result names the exact Verifier and subject versions. A file a Verifier mounts, such as the sharing policy, is part of the Verifier version: uploading a newer policy does not change what an existing version reads, and a caller cannot swap the file at run time. To judge with the new policy, declare a new Verifier version, as the Quickstart's [policy update](/get-started/quickstart/#5-update-the-policy) does.

**Limits:** Wyrd pins what it holds. Records your application supplies per request are pinned as inputs, but their origin is your application's to record.

## 4. Treat missing evidence as inconclusive

**Prevents:** a missing record that passes by default, or fails by default.

Supply every slot a requirement needs. When one is missing, the requirements that need it come back `inconclusive` and the others are judged. In the finance journey, a release with no destination record is `inconclusive` on `destination-restricted`, and the application holds it.

Partial evidence is treated the same way. A `trace` check is `inconclusive`, never judged on part of a trace, when the trace has more than 10,000 spans or cannot be read in full. [Trace evidence](/understand/how-verification-works/#trace-evidence) lists every case.

**Limits:** `inconclusive` tells you a judgment was not possible, not why the evidence is missing. Investigate a rising rate of inconclusive results as a capture problem.

## 5. Do not read absence as proof

**Prevents:** concluding an action was skipped because its record is not there.

An agent's trace can show the steps it took. A missing span is weaker evidence than it looks:

- Spans can land after the call that verifies them. `verify` judges a trace with no visible span `inconclusive`; a queued execution tries again before it does.
- Exported spans can be dropped when the exporter's queue is full, and sampling can skip whole traces.
- The step may have run in code that is not instrumented.

Conclude that a step was skipped only when the capture path records every executed step, and require a positive record of the step rather than the absence of a contrary one.

**Limits:** even complete traces show what ran, not that it ran correctly. Pair a trace check with checks on the step's output.

## Next

[Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) covers what your application does with each outcome.
