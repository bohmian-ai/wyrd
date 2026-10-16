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

An `extract` step can read `pdf`, `text`, `markdown`, `csv`, `json`, `html`, `docx`, `xlsx`, and `pptx`. See [extract](#extract) for what each extractor returns.

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

`extract` runs one of Wyrd's built-in extractors on a `media` input and returns the file's text and structure as JSON. Checks and Agent placeholders read JSON and text, not raw files, so any PDF, Office document, or data file you want to judge goes through `extract` first.

It takes one field, `from`, naming a `media` slot. The extractor is picked by the file's format; you don't choose one.

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

```python
from wyrd import verify

run.observe.verify("policy-check", policy=verify.File("policy.pdf"))
```

##### Built-in extractors

Every output has a `format` field naming the extractor that produced it, and every other field is always present: an empty list is `[]`, empty text is `""`, and `null` appears only where the type allows it. There is no `page_count` or `slide_count`; use the list's length.

| Format | Reads | Output |
| --- | --- | --- |
| `pdf` | The text layer of each page | `pages` |
| `docx` | Paragraphs with their style, and tables, in document order | `blocks` |
| `pptx` | Each slide's title, body paragraphs, tables, and speaker notes | `slides` |
| `xlsx` | Every non-empty cell of every sheet: value, formula, and error | `sheets` |
| `csv` | Every row, including the header, as strings | `rows` |
| `json` | The parsed document | `value` |
| `text`, `markdown`, `html` | The source text, unchanged | `text` |

**PDF**

```typescript
{ format: "pdf"; pages: { number: number; text: string }[] }
```

```json
{"format": "pdf", "pages": [{"number": 1, "text": "Refund policy\nRefunds are issued within 30 days."}, {"number": 2, "text": ""}]}
```

`number` is one-based. A page with no text layer, such as a scan, has `text: ""`. An encrypted PDF fails the step.

**DOCX**

```typescript
{ format: "docx"; blocks: ({ kind: "paragraph"; text: string; style: string | null } | { kind: "table"; rows: string[][] })[] }
```

```json
{"format": "docx", "blocks": [{"kind": "paragraph", "text": "Refund policy", "style": "Heading1"}, {"kind": "table", "rows": [["Window", "Days"], ["Standard", "30"]]}]}
```

`style` is Word's stored style ID, such as `Heading1`, or `null` for an unstyled paragraph. Text inside content controls is included.

**PPTX**

```typescript
{ format: "pptx"; slides: { number: number; title: string | null; paragraphs: string[]; tables: { rows: string[][] }[]; notes: string }[] }
```

```json
{"format": "pptx", "slides": [{"number": 1, "title": "Q3 refunds", "paragraphs": ["Refund rate fell to 2%"], "tables": [{"rows": [["Region", "Rate"], ["US", "2%"]]}], "notes": ""}]}
```

Slides are in presentation order and `number` is one-based. `title` is `null` when the slide has no title placeholder; `notes` is `""` when it has no speaker notes.

**XLSX**

```typescript
{ format: "xlsx"; sheets: { name: string; cells: { address: string; value: string | number | boolean | null; formula: string | null; error: string | null }[] }[] }
```

```json
{"format": "xlsx", "sheets": [{"name": "Refunds", "cells": [{"address": "A1", "value": "Days", "formula": null, "error": null}, {"address": "B2", "value": 30, "formula": "=15*2", "error": null}]}]}
```

Cells are in row, then column, order, and empty cells are left out, so look a cell up by `address`, not by its position in `cells`. `value` is the stored value or the result Excel last saved for a formula; Wyrd never recalculates. Dates stay Excel serial numbers. An error cell has `value: null` and the error, such as `#DIV/0!`, in `error`.

**CSV**

```typescript
{ format: "csv"; rows: string[][] }
```

```json
{"format": "csv", "rows": [["region", "rate"], ["US", "0.02"]]}
```

The first row is kept as data and no types are inferred, so `"0.02"` is a string. Rows may have different lengths.

**JSON**

```typescript
{ format: "json"; value: any }
```

```json
{"format": "json", "value": {"refund_days": 30, "regions": ["US", "EU"]}}
```

**Text, Markdown, HTML**

```typescript
{ format: "text" | "markdown" | "html"; text: string }
```

```json
{"format": "markdown", "text": "# Refund policy\n\nRefunds are issued within 30 days.\n"}
```

The file must be UTF-8. Markdown and HTML are not parsed; you get the source.

In every table, from DOCX or PPTX, a merged cell's text is in its top-left position and the positions it covers are `""`, so every row of a table has the same length.

##### Use the output

Read fields with a path, as in the example: `steps.<id>.output.pages[0].text`. Indexes are zero-based, even though PDF `number` and PPTX `number` start at 1. `[*]` works only in an `assert` check's `each`.

To give the whole file to an Agent, pass the output to a placeholder: `inputs: {policy: steps.policy-text.output}`. A citation still points at the original file by slot, using `page` for PDF, `slide` for PPTX, `lines` for text, Markdown, HTML, and CSV, and `json_pointer` into this output for DOCX, XLSX, and JSON. [Extraction output](/reference/extraction-output/) has a citation example for each.

##### What extract does not do

- Read images. There is no OCR and no rendering. `png`, `jpeg`, `gif`, and `webp` can't be extracted; give them to an Agent as a `media` input instead.
- Read legacy Office files. `doc`, `xls`, and `ppt` are separate formats; convert them to `docx`, `xlsx`, or `pptx`.
- Open archives. `zip` and `tar_gz` go to a step Agent's `workspace` instead.

A slot that accepts a format `extract` can't read fails `wyrd plan` with rule `extract_format`. At run time the step fails, and the requirements that need it are `inconclusive`, when the file doesn't parse as its format, a PDF is encrypted, a text file isn't UTF-8, or the file exceeds `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` or `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB` ([limits](/reference/verification-runtime-limits/)). Each result records the extractor version, `wyrd-extract/1`, so a verdict shows which extractor read the file.

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
