---
title: Verifier Card
description: Fields, input slots, steps, checks, operators, references, citations, and refusals of a Verifier spec.
pillar: wyrd
section: Reference
order: 1
---

A Verifier spec declares the evidence a caller sends, the steps that prepare it, and the requirements that judge it. Every object refuses keys it does not define.

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

## Spec fields

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `description` | string | none | One sentence shown with results |
| `inputs` | map of slot name to [input slot](#input-slots) | `{}` | The named evidence a caller sends |
| `steps` | list of [steps](#steps) | `[]` | Work that prepares evidence before requirements run |
| `requirements` | list of [requirements](#requirements) | `[]` | What must be true; each has one check |
| `decide` | [`all` or `{pass_rate: {threshold}}`](#decide) | `all` | How requirement outcomes become the verdict |
| `context_capture` | `full`, `hash`, or `redact` | `full` | How a check's `actual` value is stored: as extracted, as its SHA-256 hex digest, or omitted |

Step and requirement `id`s share one namespace.

## Input slots

A caller fills a slot by name; steps and checks read it as `inputs.<slot>`. `json`, `media`, and `trace` slots also take `required`, default `true`.

| `type` | Extra fields | Accepts |
| --- | --- | --- |
| `json` | `schema` (JSON Schema, optional) | Any JSON value, checked against `schema` when set |
| `media` | `formats` (required), `max_bytes` | A file in one of `formats`, no larger than `max_bytes` |
| `trace` | none | The run's trace spans |
| `samples` | `features` (required) | Feature values over a window, for drift checks |

A group Verifier slot is a source string instead of an object: `verifiers.<name>.inputs.<slot>` or `verifiers.<name>.requirements.<rid>.outcome`. It holds one value per source execution, in order. `samples` and source slots are always required.

Media formats: `png`, `jpeg`, `gif`, `webp`, `pdf`, `text`, `markdown`, `csv`, `json`, `html`, `docx`, `xlsx`, `pptx`, `doc`, `xls`, `ppt`, `tar_gz`, `zip`.

## Steps

A step's result is `steps.<id>.output`.

| Field | Type | Default | Meaning |
| --- | --- | --- | --- |
| `id` | string | required | Name used in references |
| `action` | [action](#actions) | required | What the step does |
| `depends_on` | list of step ids | `[]` | Steps that must finish first |
| `when` | `{path, operator, expected}` | none | Run only if this holds; otherwise the step is skipped and its requirements are `not_applicable` |
| `requirements` | list of [requirements](#requirements) | `[]` | Checked against this step's output, right after it |
| `timeout_seconds` | integer above 0 | none | Time limit for one attempt |
| `retries` | 0 to 5 | `0` | Attempts after the first |

### Actions

When to use each kind is in [Steps and checks](/understand/how-verification-works/#steps-and-checks).

| `kind` | Fields | Output |
| --- | --- | --- |
| `extract` | `from`: a `media` slot | The file's text and structure, in the shape [Extraction output](/reference/extraction-output/) gives for its format |
| `agent` | `agent`, `inputs`, `workspace` | The Agent's response, shaped by its Prompt `response_format` |
| `code` | `runtime` (`python`, default), `entrypoint`, `source`, `inputs` | The function's return value, as JSON |

- `extract` reads `pdf`, `text`, `markdown`, `csv`, `json`, `html`, `docx`, `xlsx`, and `pptx`, choosing the extractor by format.
- `agent` is a path such as `./reader.yaml` or a CardRef. `inputs` maps each Prompt placeholder to a reference; a placeholder takes text, so a `media` slot goes through `extract` first. `workspace` maps paths under `/inputs/` to a reference such as `inputs.policy`, or to `{artifact: {card: {kind: Artifact, name, version}, path}}`, which mounts one file, a directory, or (without `path`) every file of an uploaded Artifact version, unchanged and pinned with the Verifier. Tools are in [Agent tools and authorization](/reference/agent-tools-and-authorization/).
- `code` calls the function `entrypoint` with `inputs` as keyword arguments. `source` is `{inline: "<python>"}` or `{artifact: {card, path?}}`, where `card` names an Artifact Card.

## Requirements

A requirement ends `passed`, `failed`, `inconclusive`, or `not_applicable`.

| Field | Type | Meaning |
| --- | --- | --- |
| `id` | string | Name in results and in `steps.<id>.requirements.<rid>` |
| `statement` | string | The claim, as a reader sees it in results |
| `check` | [check](#checks) | How the claim is decided |

### Checks

When to use each kind is in [Steps and checks](/understand/how-verification-works/#steps-and-checks).

| `kind` | Fields | Decides by |
| --- | --- | --- |
| `assert` | `path` or `each`, `operator`, `expected` | Comparing a value; `each` with `[*]` requires every item to pass |
| `trace` | `on`, `span_selector`, `operator`, `expected` | Comparing values selected from trace spans |
| `agent` | `agent`, `inputs`, `workspace` | An Agent, with its declared tools and optional workspace as in a step, whose Prompt `response_format` is `verdict` or a closed [verdict](#verdicts-and-citations) schema |
| `code` | `runtime`, `source`, `entrypoint`, `inputs`, optional `response_format` | A Python function that returns a verdict |
| `drift` | `on`, `method`, `signal`, `condition`, `profile` | Comparing a window of samples with a baseline |

A `trace` check reads every retained span with the trace ID from `bifrost.traces.spans`, as the executing account, ordered by start time then span ID, up to 10,000 spans. A larger trace, a refused or failed query, a timed-out read, an undecodable span, or an input with no trace makes it `inconclusive`. A trace with no visible span is `inconclusive` under `verify`; in a queued execution, each attempt before the last ends with error code `evidence_unavailable` and status `retrying` (3 attempts, 30 s then 60 s apart), and the last is `inconclusive`. [Trace evidence](/understand/how-verification-works/#trace-evidence) explains the behavior.

## Comparison operators

`assert`, `trace`, and `when` take an operator: its name, such as `operator: not_contains`, or `{kind: <name>, ...}` when it takes parameters.

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

Every `path`, `each`, `from`, `on`, and `inputs` value is a reference.

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

An `agent` or `code` check returns a verdict:

```json
{
  "outcome": "passed",
  "reason": "The 40 dollar refund is approved within 30 days.",
  "citations": [{"slot": "letter", "locator": {"lines": {"start": 3, "end": 4}}}]
}
```

`outcome` is `passed`, `failed`, or `inconclusive`. Each citation names an input `slot` and one `locator`:

| Locator | For |
| --- | --- |
| `{page: n}` | PDF pages, counted from 1 |
| `{slide: n}` | PPTX slides |
| `{lines: {start, end}}` | Text, Markdown, HTML, CSV |
| `{json_pointer: "/path"}` | JSON inputs and DOCX or XLSX extraction output |
| `{region: {page, x, y, width, height}}` | A rectangle on a page, in page coordinates |

A citation names the original slot, or the `attachment_<n>` slot a fixed workspace file reports in `sources`, never an extract step or `/work` file. With `response_format: verdict`, an Agent check's model may cite only the bare slots its inputs and workspace reach, such as `answer`, never `inputs.answer`. An invalid citation is dropped, and a `passed` verdict left without one is recorded as `inconclusive`. For JSON pointers into extracted files, see [Extraction output](/reference/extraction-output/#citations).

### Custom data

`response_format: verdict` (or `{kind: verdict}`) asks for exactly the verdict above. `{kind: verdict, data: <JSON Schema>}` also requires a `data` object matching that object schema, written in JSON Schema 2020-12 (omit `$schema` or declare 2020-12) and without a root `$id`. An Agent declares it on its Prompt; a `code` check declares it on the check, beside `entrypoint`, and returns `data` in its verdict. Without a `data` schema, a verdict must not carry `data`.

The server validates the whole verdict against the declared schema. Data must fit both limits, inclusive:

| Limit | Bound |
| --- | --- |
| Size | 65,536 bytes of compact UTF-8 JSON |
| Depth | 16 containers; the root object is one, each nested object or array adds one |

An Agent answer that fails its schema is returned to the model with a correction, and each correction spends one of the Agent's `max_iterations`. An answer still invalid on the last iteration makes the requirement `inconclusive` with `WYRD_AGENT_422_STRUCTURED_INVALID` in its reason. A `code` check's verdict is never corrected. A verdict that fails its schema or a limit makes the requirement `inconclusive` with no `data`; the reason names the location or limit, never the content. Accepted data is the requirement result's `data`, on step and Verifier requirements alike, and is kept with the rest of the result.

## Decide

| Value | Verdict |
| --- | --- |
| `all` | `failed` if any requirement fails; otherwise `inconclusive` if any is inconclusive; otherwise `passed` |
| `{pass_rate: {threshold}}` | `passed` when the passed share of applicable requirements, inconclusive ones included in the count, reaches `threshold`; `threshold` is above 0 and at most 1 |

`not_applicable` requirements are excluded. With no applicable requirement, the verdict is `inconclusive`.

## Rejections

| Code | Raised when |
| --- | --- |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | `wyrd declare` refused the Card tree; each problem is one entry in `details.diagnostics` with its own code |
| `WYRD_REGISTRY_400_INVALID_CARD_SPEC` | The server refused a registered spec whose field is unknown, missing, or has the wrong type |
| `WYRD_SPEC_400_VALIDATION` | A rule fails; its `rule` and `path` are in `details`, or in a `wyrd declare` diagnostic's `context` |

Common `rule` values in `WYRD_SPEC_400_VALIDATION`:

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
| `check_response_format` | A `code` check's `response_format` names an unknown format or an invalid `data` schema |
| `check_agent_schema` | A check Agent's Prompt cannot produce a verdict, or declares `data` without requiring an object |
