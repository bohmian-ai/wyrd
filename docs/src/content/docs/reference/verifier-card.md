---
title: Verifier Card
description: Every field of a Verifier spec, its input slots, steps, checks, operators, references, citations, and outcomes.
pillar: wyrd
section: Reference
order: 1
draft: true
status: runnable
---

A Verifier takes evidence from your app and decides whether it meets your requirements. Every object on this page rejects keys it does not define, so a typo fails at `wyrd plan` instead of being ignored. For a walkthrough, see [Author a Verifier](/build-and-verify/verify/author-a-verifier/).

## How a Verifier works

A Verifier is built from five parts:

| Part | What it is | Example |
| --- | --- | --- |
| Input | A named piece of evidence your app sends | `letter`, a Markdown file |
| Step | Optional work that prepares evidence before anything is judged | Read the text out of the letter |
| Action | What one step does: `extract`, `agent`, or `code` | `extract` the letter |
| Requirement | One claim that must be true, in words a reviewer reads | "The letter follows the refund policy" |
| Check | How one requirement is decided: `assert`, `trace`, `agent`, `code`, or `drift` | Ask a judge Agent |

When you call `verify` or `submit`, the server:

1. Checks each input against its slot, such as a file's format and size.
2. Runs the steps. A step starts once the steps it reads from are done, and steps that don't depend on each other may run at the same time. A step's own requirements are checked right after it, before anything that depends on it starts.
3. Checks the top-level requirements, once every step has finished.
4. Combines every requirement's outcome into one verdict with `decide`.

Steps read and compute; they are not the place for side effects. To act when a Verifier fails, such as paging someone, attach an [Operator](/build-and-verify/verify/respond-to-a-failed-verification/) to the binding.

A Verifier with no steps is common: one input and one `assert` requirement covers many checks.

## Spec fields

A Verifier spec has six fields.

```yaml
apiVersion: wyrd/v1
kind: Verifier
metadata: {name: follows-policy, version: 1.0.0, space: support}
spec:
  description: A refund decision letter follows the refund policy.
  inputs: {letter: {type: media, formats: [markdown]}}
  steps: [{id: read-letter, action: {kind: extract, from: inputs.letter}}]
  requirements: [{id: follows-policy, statement: "...", check: {kind: agent, agent: ./policy-judge.yaml}}]
  decide: all
```

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `description` | string | none | One sentence shown with results |
| `inputs` | map of slot name to [input slot](#input-slots) | `{}` | The named evidence a caller sends |
| `steps` | list of [steps](#steps) | `[]` | Work that prepares evidence before requirements run |
| `requirements` | list of [requirements](#requirements) | `[]` | What must be true; each has one check |
| `decide` | `all` or `{pass_rate: {threshold}}` | `all` | How requirement outcomes become the verdict |
| `context_capture` | object | none | What run context to keep with the result |

Step and requirement `id`s share one namespace across the whole Verifier.

## Input slots

An input slot names one piece of evidence and the shape it must have. Your app fills it by name, as in `verify("follows-policy", letter=...)`, and steps and checks read it as `inputs.<slot>`. Each slot has a `type` and `required`, which defaults to `true`.

| `type` | Extra fields | Accepts |
| --- | --- | --- |
| `json` | `schema` (JSON Schema, optional) | Any JSON value, checked against `schema` when set |
| `media` | `formats` (required), `max_bytes` | A file in one of `formats`, no larger than `max_bytes` |
| `trace` | none | The run's trace spans |
| `samples` | `features` (required) | Feature values over a window, for drift checks |

Media formats: `png`, `jpeg`, `gif`, `webp`, `pdf`, `text`, `markdown`, `csv`, `json`, `html`, `docx`, `xlsx`, `pptx`, `doc`, `xls`, `ppt`, `tar_gz`, `zip`.

An `extract` step can read `pdf`, `text`, `markdown`, `csv`, `json`, `html`, `docx`, `xlsx`, and `pptx`. Its output shapes are in [Extraction output](/reference/extraction-output/).

## Steps

A step prepares evidence a requirement needs but the caller did not send in usable form: text pulled out of a PDF, a value computed in Python, or an Agent's summary. Its result is `steps.<id>.output`. Skip steps when your requirements can read the inputs directly.

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `id` | string | required | Name used in references such as `steps.<id>.output` |
| `action` | [action](#actions) | required | What the step does |
| `depends_on` | list of step ids | `[]` | Steps that must finish first |
| `when` | `{path, operator, expected}` | none | Run the step only if this condition holds |
| `requirements` | list of [requirements](#requirements) | `[]` | Requirements checked against this step's output |
| `timeout_seconds` | integer above 0 | none | Time limit for one attempt |
| `retries` | 0 to 5 | `0` | Attempts after the first |

### Actions

The action is what a step does. Each step has exactly one.

| `kind` | Use it to | Output |
| --- | --- | --- |
| [`extract`](#extract) | Turn an uploaded file into JSON that checks and Agents can read | The file's text and structure |
| [`agent`](#agent) | Ask an Agent to read evidence and return structured data | The Agent's response |
| [`code`](#code) | Compute a value in Python | The function's return value |

#### extract

`extract` reads one `media` input and returns its text and structure as JSON. Use it whenever a check or an Agent needs what is inside a file. An Agent's text placeholders and an `assert` path cannot read a raw file, so a PDF, DOCX, or spreadsheet always goes through `extract` first.

It takes one field, `from`, which names a `media` slot:

```yaml
spec:
  inputs:
    policy: {type: media, formats: [pdf]}
  steps:
    - id: policy-text
      action: {kind: extract, from: inputs.policy}
  requirements:
    - id: has-text
      statement: Every page of the policy has text
      check: {kind: assert, each: "steps.policy-text.output.pages[*].text", operator: not_equals, expected: ""}
    - id: names-window
      statement: The first page states the refund window
      check: {kind: assert, path: "steps.policy-text.output.pages[0].text", operator: contains, expected: "30 days"}
```

The app sends the file, and the server extracts it before any requirement runs:

```python
from wyrd import verify

run.observe.verify("policy-check", policy=verify.File("policy.pdf"))
```

For a two-page PDF, `steps.policy-text.output` is:

```json
{"format": "pdf", "pages": [{"number": 1, "text": "Refund policy\nRefunds are issued within 30 days."}, {"number": 2, "text": "Exceptions"}]}
```

The shape depends on the format: `pages` for PDF, `slides` for PPTX, `blocks` for DOCX, `sheets` for XLSX, `rows` for CSV, `value` for JSON, and `text` for text, Markdown, and HTML. Every shape is in [Extraction output](/reference/extraction-output/).

To hand the file to a judge, pass the whole output to an Agent placeholder, as in `inputs: {policy: steps.policy-text.output}`. Citations still point at the original file by slot and page or line, not at the step.

Extraction reads text only: there is no OCR, so a scanned PDF extracts as empty pages. The `has-text` requirement above catches that. `png`, `jpeg`, `gif`, `webp`, `doc`, `xls`, `ppt`, `tar_gz`, and `zip` cannot be extracted, and a slot that accepts any of them fails `wyrd plan` with rule `extract_format`.

#### agent

`agent` runs an Agent Card and returns its response, shaped by the Agent's Prompt `response_format`. Use it when a later requirement needs a model to pull something out of the evidence, such as the refund limit a policy states:

```yaml
steps:
  - id: refund-rules
    action:
      kind: agent
      agent: ./refund-reader.yaml
      inputs: {policy: steps.policy-text.output, answer: inputs.answer.text}
requirements:
  - id: limit-found
    statement: The reader found a positive refund limit
    check: {kind: assert, path: steps.refund-rules.output.max_refund, operator: greater_than, expected: 0}
```

`agent` is a path such as `./refund-reader.yaml` or a CardRef. `inputs` maps each Prompt placeholder to a reference. A placeholder takes text, so a `media` slot goes through `extract` first. `workspace` maps file paths under `/inputs/`, such as `/inputs/policy.pdf`, to references, for Agents that read files with the `fs` tools.

A step Agent can use these tools: `cards.list`, `cards.get`, `cards.get_by_ref`, `bifrost.list_tables`, `bifrost.describe_table`, `bifrost.query`, `fs.list`, `fs.read`, `fs.search`, and `code.run`.

#### code

`code` calls a Python function and returns what it returns, as JSON. Use it for logic one operator cannot express:

```yaml
steps:
  - id: answer-facts
    action:
      kind: code
      entrypoint: facts
      inputs: {answer: inputs.answer, rules: steps.refund-rules.output}
      source:
        inline: |
          def facts(answer, rules):
              return {"within_limit": answer["refund"] <= rules["max_refund"]}
requirements:
  - id: refund-within-limit
    statement: The refund offered is within the policy limit
    check: {kind: assert, path: steps.answer-facts.output.within_limit, operator: equals, expected: true}
```

`entrypoint` names the function; it receives `inputs` as keyword arguments. `source` is `{inline: "<python>"}` or `{artifact: {card, path?}}`, where `card` names an Artifact Card. A step returns data for later checks. To return a verdict directly, use a [`code` check](#checks) instead.

## Requirements

A requirement is one claim the evidence must support, plus the check that decides it. Each requirement ends `passed`, `failed`, `inconclusive`, or `not_applicable`, and those outcomes become the verdict. Put a requirement on a step when it judges that step's output, so later steps can branch on it; otherwise list it at the top level.

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | string | Name in results and in `steps.<id>.requirements.<rid>` |
| `statement` | string | The claim, as a reader sees it in results |
| `check` | [check](#checks) | How the claim is decided |

### Checks

A check is the method that decides one requirement. Use the cheapest one that can decide it: `assert` and `trace` compare values with no model, `code` runs your Python, and `agent` asks a model.

| `kind` | Fields | Decides by |
| --- | --- | --- |
| `assert` | `path` or `each`, `operator`, `expected` | Comparing a value; `each` with `[*]` requires every item to pass |
| `trace` | `on`, `span_selector`, `operator`, `expected` | Comparing values selected from trace spans |
| `agent` | `agent`, `inputs` | A tool-less Agent whose `response_format` is a closed [verdict](#verdicts-and-citations) |
| `code` | `runtime`, `source`, `entrypoint`, `inputs` | A Python function that returns a verdict |
| `drift` | `on`, `method`, `signal`, `condition`, `profile` | Comparing a window of samples with a baseline; see [Monitor drift](/build-and-verify/verify/monitor-drift/) |

`assert` and `trace` return in milliseconds. A step whose `when` condition is false is skipped, and its requirements are `not_applicable`.

## Comparison operators

`assert`, `trace`, and `when` compare a value with an operator. Write an operator as its name, such as `operator: not_contains`, or, when it takes parameters, as `{kind: <name>, ...}`.

| Group | Operators |
| --- | --- |
| Comparison | `equals`, `not_equals`, `greater_than`, `greater_than_or_equals`, `less_than`, `less_than_or_equals`, `is_positive`, `is_negative`, `is_zero` |
| String | `contains`, `not_contains`, `contains_ignore_case`, `starts_with`, `ends_with`, `is_email`, `is_url`, `is_uuid`, `is_ipv4`, `is_ipv6`, `is_json` |
| Collection | `in`, `not_in`, `is_subset`, `is_superset`, `is_disjoint`, `all_of`, `any_of`, `none_of`, `is_empty`, `is_non_empty`, `unique_values` |
| Type | `is_truthy`, `is_falsy`, `is_null`, `is_not_null`, `is_string`, `is_number`, `is_boolean`, `is_object` |

Operators with parameters:

| `kind` | Parameters |
| --- | --- |
| `in_range`, `not_in_range` | `min`, `max`, `inclusive` |
| `approximately_equals`, `within_abs_tolerance` | `tolerance` |
| `within_pct_tolerance` | `pct` |
| `within_std_dev` | `sigma`, `mean`, `std_dev` |
| `between_percentiles` | `lower_pct`, `upper_pct` |
| `divergence_less_than` | `metric` (`kl`, `js`, `wasserstein`), `threshold` |
| `cosine_similarity_at_least` | `threshold` |
| `matches_regex`, `not_matches_regex` | `pattern` |
| `has_min_length` | `min` |
| `has_max_length` | `max` |
| `length` | `expected` |
| `length_greater_than` | `min` |
| `length_less_than` | `max` |
| `is_type` | `expected` (`null`, `bool`, `number`, `string`, `array`, `object`) |

```yaml
check: {kind: assert, path: inputs.refund.amount, operator: {kind: in_range, min: 0, max: 100, inclusive: true}}
```

## References

Every `path`, `from`, `on`, and `inputs` value is a reference.

| Reference | Reads |
| --- | --- |
| `inputs.<slot>` | An input slot's value |
| `steps.<id>.output` | A step's output |
| `steps.<id>.status` | A step's final status |
| `steps.<id>.requirements.<rid>.outcome` | The outcome of a step's requirement |
| `steps.<id>.requirements.<rid>.reason` | The reason of a step's requirement |
| `requirement.id`, `requirement.statement` | The requirement being checked |

Inputs and step outputs take child selectors: `.field`, `[0]`, and `[*]`. `[*]` is allowed only in an `assert` check's `each`.

## Verdicts and citations

An `agent` or `code` check returns:

```json
{
  "outcome": "passed",
  "reason": "The 40 dollar refund is approved within 30 days.",
  "citations": [{"slot": "letter", "locator": {"lines": {"start": 3, "end": 4}}}]
}
```

`outcome` is `passed`, `failed`, `inconclusive`, or `not_applicable`. Each citation names an input `slot` and one `locator`:

| Locator | For |
| --- | --- |
| `{page: n}` | PDF pages, counted from 1 |
| `{slide: n}` | PPTX slides |
| `{lines: {start, end}}` | Text, Markdown, HTML, CSV |
| `{json_pointer: "/path"}` | JSON inputs and DOCX or XLSX extraction output |
| `{region: {page, x, y, width, height}}` | A rectangle on a page, in page coordinates |

The server checks every citation against the evidence. A `passed` verdict without a valid citation is recorded as `inconclusive`.

## Decide

With `decide: all`, the Verifier passes when every requirement that applies passes. With `decide: {pass_rate: {threshold: 0.8}}`, it passes when at least that share of applicable requirements pass. `threshold` is above 0 and at most 1.

## Rejections

| Code | Raised when |
| --- | --- |
| `WYRD_REGISTRY_400_INVALID_CARD_SPEC` | A field is unknown, missing, or has the wrong type |
| `WYRD_SPEC_400_VALIDATION` | A rule fails; `details` holds `rule` and `path` |

Common `rule` values:

| `rule` | Cause |
| --- | --- |
| `unknown_reference` | A reference names an input, step, or requirement that does not exist |
| `duplicate_id` | Two steps or requirements share an `id` |
| `graph` | `depends_on` forms a cycle |
| `media_as_text` | A `media` slot is bound to an Agent text placeholder |
| `extract_format` | An `extract` step reads a slot with a format it cannot extract |
| `operator` | An operator's parameters are invalid, such as a bad regex |
| `step_budget` | `timeout_seconds` is 0 or `retries` is above 5 |
| `pass_rate` | `threshold` is not above 0 and at most 1 |
