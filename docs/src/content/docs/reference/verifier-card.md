---
title: Verifier Card
description: Every field of a Verifier spec, its input slots, steps, checks, operators, references, citations, and outcomes.
pillar: wyrd
section: Reference
order: 1
draft: true
status: runnable
---

A Verifier spec has six fields. Every object on this page rejects keys it does not define, so a typo fails at `wyrd plan` instead of being ignored. For a walkthrough, see [Author a Verifier](/build-and-verify/verify/author-a-verifier/).

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

Each slot has a `type` and `required`, which defaults to `true`.

| `type` | Extra fields | Accepts |
| --- | --- | --- |
| `json` | `schema` (JSON Schema, optional) | Any JSON value, checked against `schema` when set |
| `media` | `formats` (required), `max_bytes` | A file in one of `formats`, no larger than `max_bytes` |
| `trace` | none | The run's trace spans |
| `samples` | `features` (required) | Feature values over a window, for drift checks |

Media formats: `png`, `jpeg`, `gif`, `webp`, `pdf`, `text`, `markdown`, `csv`, `json`, `html`, `docx`, `xlsx`, `pptx`, `doc`, `xls`, `ppt`, `tar_gz`, `zip`.

An `extract` step can read `pdf`, `text`, `markdown`, `csv`, `json`, `html`, `docx`, `xlsx`, and `pptx`. Its output shapes are in [Extraction output](/reference/extraction-output/).

## Steps

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

| `kind` | Fields | Output |
| --- | --- | --- |
| `extract` | `from`: a reference to a `media` slot | The extracted document |
| `agent` | `agent`, `inputs`, `workspace` | The Agent's response |
| `code` | `runtime` (`python`), `source`, `entrypoint`, `inputs` | The function's return value |

`agent` is a path such as `./policy-judge.yaml` or a CardRef. `inputs` maps each prompt placeholder to a reference. A placeholder takes text, so a `media` slot goes through `extract` first. `workspace` maps file paths under `/inputs/`, such as `/inputs/policy.pdf`, to references, for Agents that read files with the `fs` tools.

`source` is `{inline: "<python>"}` or `{artifact: {card, path?}}`, where `card` names an Artifact Card. `entrypoint` names the function to call; it receives `inputs` as keyword arguments.

A step Agent can use these tools: `cards.list`, `cards.get`, `cards.get_by_ref`, `bifrost.list_tables`, `bifrost.describe_table`, `bifrost.query`, `fs.list`, `fs.read`, `fs.search`, and `code.run`.

## Requirements

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | string | Name in results and in `steps.<id>.requirements.<rid>` |
| `statement` | string | The claim, as a reader sees it in results |
| `check` | [check](#checks) | How the claim is decided |

### Checks

| `kind` | Fields | Decides by |
| --- | --- | --- |
| `assert` | `path` or `each`, `operator`, `expected` | Comparing a value; `each` with `[*]` requires every item to pass |
| `trace` | `on`, `span_selector`, `operator`, `expected` | Comparing values selected from trace spans |
| `agent` | `agent`, `inputs` | A tool-less Agent whose `response_format` is a closed [verdict](#verdicts-and-citations) |
| `code` | `runtime`, `source`, `entrypoint`, `inputs` | A Python function that returns a verdict |
| `drift` | `on`, `method`, `signal`, `condition`, `profile` | Comparing a window of samples with a baseline; see [Monitor drift](/build-and-verify/verify/monitor-drift/) |

`assert` and `trace` need no model and return in milliseconds.

## Operators

Write an operator as its name, such as `operator: not_contains`, or, when it takes parameters, as `{kind: <name>, ...}`.

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
