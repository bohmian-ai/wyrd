---
title: Author a Verifier
description: Write a Verifier Card from its inputs and requirements, starting with a one-line assertion and working up to a model judge that cites a document.
pillar: wyrd
section: Build and verify
topic: Verify
order: 0
status: runnable
---

A Verifier is a YAML Card that says what evidence it takes and what must be true of it. This page builds the three Verifiers from the support-desk example, from cheapest to most expensive: an assertion, a Python check, then a model judge that reads a document. Start with the cheapest check that can catch the failure. Only reach for a model when nothing else can decide.

Every Verifier has the same skeleton:

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: my-check, version: 1.0.0, space: support}
spec:
  description: One sentence a reviewer reads in a report.
  inputs: {}        # the named evidence your app sends
  steps: []         # optional work before checking, such as extracting a file
  requirements: []  # what must be true; each has one check
```

Every field is listed in the [Verifier Card reference](/reference/verifier-card/).

## 1. Assert on a value

The cheapest Verifier is one assertion. `no-refund-promise` takes the agent's answer as JSON and fails if it contains a phrase:

<CodeFromFile file="examples/support-desk-app/service/no-refund-promise.yaml" lang="yaml" title="service/no-refund-promise.yaml" />

What each part does:

- `inputs.answer` names a slot. Your app fills it when it calls `verify("no-refund-promise", answer)`. `type: json` accepts any JSON value; add `schema` to constrain it.
- `requirements` is a list. Each requirement has an `id`, a `statement` a human reads in the result, and one `check`.
- `kind: assert` compares the value at `path` with `operator` and `expected`. No model runs, so the result is deterministic and returns in milliseconds.

`path` can reach inside a value, such as `inputs.ticket.order_id`. To check every item of a list, use `each: inputs.items[*].price` in place of `path`; the requirement passes only if every item does.

## 2. Check with Python

When one operator cannot express the rule, write it in Python. `ticket-complete` names every field the help desk needs that is missing:

<CodeFromFile file="examples/support-desk-app/service/ticket-complete.yaml" lang="yaml" title="service/ticket-complete.yaml" />

The function named by `entrypoint` receives the `inputs` you map to it and returns a verdict:

| Field | Value |
| --- | --- |
| `outcome` | `passed`, `failed`, `inconclusive`, or `not_applicable` |
| `reason` | One sentence that ends up in the result |
| `citations` | Where in the evidence the decision rests, as `{slot, locator}` |

The code runs in Wyrd's sandbox, not in your app. Keep it to the decision; anything with side effects belongs in your app.

A `passed` verdict must cite the evidence it read. Here `{"json_pointer": ""}` cites the whole ticket. A `passed` verdict with no valid citation is recorded as `inconclusive`, so a check cannot pass without showing its work.

## 3. Judge a document with a model

Some rules need judgment. The support agent writes a refund decision letter, and `follows-policy` asks a model whether the letter follows the refund policy. This takes three Cards: the Verifier, the judge Agent, and the Service that binds the Verifier to the support agent.

<Tabs>
<Tab label="follows-policy.yaml">

<CodeFromFile file="examples/support-desk-app/service/follows-policy.yaml" lang="yaml" title="service/follows-policy.yaml" />

</Tab>
<Tab label="policy-judge.yaml">

<CodeFromFile file="examples/support-desk-app/service/policy-judge.yaml" lang="yaml" title="service/policy-judge.yaml" />

</Tab>
<Tab label="support-desk.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-desk.yaml" lang="yaml" title="service/support-desk.yaml" />

</Tab>
</Tabs>

Read it top to bottom:

1. `inputs.letter` is a `media` slot. It accepts only Markdown files up to 1 MiB, so a PDF or a 50 MB upload is refused before any model runs.
2. The `read-letter` step extracts the file into `{format, text}`. A `{{letter}}` placeholder takes text, not a file, so the letter goes through `extract` first. Steps run before requirements, and a requirement reads a step's result as `steps.read-letter.output`.
3. The `agent` check sends the requirement's statement and the extracted letter to `policy-judge`. Those names fill `{{claim}}` and `{{letter}}` in the judge's prompt.
4. `policy-judge` has the policy in its system message and a closed `response_format`, so the model can only answer with an outcome, a reason, and citations at lines of the letter.

The policy lives in the judge, not in the request. Your app sends only the letter, and every verdict is tied to the exact judge version, and so the exact policy, that produced it. A new policy is a new `policy-judge` version.

A model call takes seconds, so the app [submits this Verifier](/build-and-verify/observe/record-a-run-and-observations/) in the background instead of waiting on it in the request.

## 4. Check it before you declare

`wyrd declare --check` loads every Card a Service references, follows the `./` paths, and validates each one without writing anything:

```bash
wyrd declare --check service/support-desk.yaml
```

```text
plan valid: 7 card(s)
```

The CLI then lists each Card it would declare. A broken Verifier fails here with the field that is wrong:

| You see | Cause | Fix |
| --- | --- | --- |
| `WYRD_REGISTRY_400_INVALID_CARD_SPEC` naming an unknown field | A misspelled key; Verifier specs reject fields they do not define | Fix the spelling against the [reference](/reference/verifier-card/) |
| `WYRD_SPEC_400_VALIDATION` with rule `unknown_reference` | A reference to an input, step, or requirement that does not exist, such as `steps.read-leter.output` | Match the slot or step `id` |
| `WYRD_SPEC_400_VALIDATION` with rule `duplicate_id` | A step and a requirement share an `id` | IDs are unique across the whole Verifier |
| `WYRD_SPEC_400_VALIDATION` with rule `media_as_text` | A `media` slot is passed straight to an Agent text placeholder | Add an `extract` step and pass `steps.<id>.output`, as in step 3 |
| `WYRD_SPEC_400_VALIDATION` with rule `extract_format` | An `extract` step reads a slot that accepts a format extraction cannot read, such as `png` | Accept only PDF, text, Markdown, CSV, JSON, HTML, DOCX, XLSX, or PPTX in that slot |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | `apiVersion`, `kind`, or `metadata` is missing or misspelled | Copy the skeleton above |

Then [bind the Verifier](/build-and-verify/declare/bind-a-verifier/) to a component and [call it from your app](/build-and-verify/verify/verify-a-request-in-real-time/).
