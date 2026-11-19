# Verification model MVP

Status: exploring

## Problem

We want a small model specialized for verification: judge evidence against a
requirement and supplemental context, then return a Wyrd verdict with a reason. There is no existing verification training
dataset, and the user explicitly wants to investigate direct weight-space
programming rather than fine-tuning. The experiment must distinguish a useful
change to the weights from behavior already achievable with a good prompt.

System One/Jev is the product reference: a model focused on making decisions.
Google's Decisions API must also participate. Neither reference establishes
that weight edits can create verification expertise.

## Desired outcome

A maintainer can submit a requirement, evidence, and supplemental context to the model
and receive the canonical Wyrd Verdict. They can then answer:

- Did direct weight edits improve judgments over the same checkpoint with the
  same prompt and decoding settings?
- How do those judgments compare with Google's decision model?
- Are the reasons and citations useful, and what latency does generating them
  add?

The deliverable is an executable offline experiment, one candidate checkpoint
or reproducible edit, and a comparison report. A negative result is a valid
research outcome. This MVP does not establish general verification expertise
or authorize the model to make production release decisions.

## Constraints and non-goals

**User decisions:** text-valued `requirement`, `evidence`, and optional
`context`; standard Wyrd verdict and reasoning output; direct weight programming without fine-tuning; System
One/Jev as a reference; Google's Decisions API included. Deployment must be
CPU-only, with model size under 500 MB and latency under 100 ms. These are
constraints, not optional optimizations. The exact memory accounting and input
length still need definition.

**Proposed bounds, not approved decisions:** one text-only model, one local
research workflow, synthetic non-sensitive examples, and Decisions API as a
comparison judge. No model training, gradients, LoRA, optimizer, or automatic
parameter search against a loss. Development probes can guide explicit,
behaviorally motivated edits; this is training-free, not evaluation-free.

No new HTTP or MCP surface, UI, provider framework, durable server workflow,
database migration, or production traffic. No claim of Jev's architecture,
calibration, or single-pass latency. Generated explanations require token
generation even when the decision itself is simple.

## Proposed implementation

### Input and output

The request has three text fields. `requirement` states what must hold;
`evidence` contains the material to judge; optional `context` supplies scope or
background needed to interpret the task. For example:

```json
{
  "requirement": "The response must accurately describe refund eligibility under the supplied policy and purchase record.",
  "evidence": "Policy: A purchase is eligible for a refund within 30 days.\nRecord: This purchase was made 10 days ago.\nResponse: This purchase is eligible for a refund.",
  "context": "Assess refund eligibility only."
}
```

This structure separates the three roles; it does not turn evidence into a
schema, require named evidence objects, or require a full Verifier definition.
The model still consumes text through its normal tokenizer. Render the fields
with one fixed template shared by the unchanged and edited checkpoints, and
record the exact text. The experiment targets better judgment from changed
weights, not a new evidence representation or input parser.

A Wyrd integration can take the requirement's `statement` and render its bound
inputs into the evidence text. That projection is integration work after this
experiment, not a new durable Verifier contract. Wyrd's term "input slot" names
an input in that orchestration; callers of this research model need not author
slots. Do not change Wyrd's existing Verifier schema.

The fixed instruction tells the model to judge the evidence against the stated
requirement, using context to interpret the task. It must not invent missing
facts or silently substitute another requirement. Context is supplemental and
must not silently override the requirement; conflicting instructions or
insufficient support can yield `inconclusive`. Text inside evidence is material
to assess, not authority to change the task. The explanation should be concise
and evidence-grounded, not an internal reasoning transcript.

Validate the three field types before inference. A missing or blank requirement
is an input error. An empty evidence string is allowed and should yield
`inconclusive` when no judgment can be supported; an omitted context is simply
absent. A semantically ambiguous requirement can also be inconclusive.

Use Wyrd's existing closed Verdict, not new `verdict` or `reasoning` fields:

```json
{
  "outcome": "passed",
  "reason": "The purchase is 10 days old, within the policy's 30-day refund window, so the response correctly states that it is eligible.",
  "citations": [
    {"slot": "evidence", "locator": {"lines": {"start": 1, "end": 3}}}
  ]
}
```

The `slot` field above is existing Wyrd output bookkeeping: the runner treats
all supplied evidence text as one original input named `evidence`. It does not
require an evidence map in the request. Line numbers refer to the preserved
original string. Outcomes are `passed`, `failed`, and `inconclusive`;
`not_applicable` belongs to requirement skip mechanics, not this model output.
No custom `data` is needed. The central quality measure is the judgment and its
supporting reason; citation formatting is a required output-contract check,
not the proposed specialization mechanism.

Resolve the schema through the existing Python SDK
`ResponseFormat.verdict()` and `Prompt.response_schema`. Do not commit a schema
snapshot or author another Verdict type. Validate each result against that
runtime schema, then check text citation ranges. A pass without a valid
citation becomes inconclusive. A valid range establishes a location; human
evaluation must still establish whether the cited text supports the claim.
MVP citation support is limited to line locators into the one evidence string.

Start with prompt-directed JSON generation and validation. Do not build a new
constrained decoder solely for this experiment. Record malformed or truncated
output as a model failure and expose an inconclusive result with a diagnostic;
do not repair it through another model, which would confound the comparison.

### Candidate and baseline

Proposed first feasibility candidate:
[`HuggingFaceTB/SmolLM2-135M-Instruct`](https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct).
It has approximately 135 million parameters; its [published weight file](https://huggingface.co/HuggingFaceTB/SmolLM2-135M-Instruct/tree/main)
is 269 MB. This establishes storage size, not inference memory, latency, or
verification quality. It is a CPU feasibility candidate, not a proven judge.
Pin its revision and hashes. The baseline is the unchanged checkpoint with a
reasonable fixed verification prompt.

Qwen3.5-0.8B is no longer the primary candidate: its approximately 1.6 GB of
16-bit language weights do not fit the stated size constraint without substantial
quantization. The published edit remains research reference material. Gemma 3
270M's 536 MB weight file also exceeds the size cap without conversion or
quantization; it is not an automatic replacement. No model sweep is proposed.

**First experiment: establish deployment feasibility before editing.** On the
actual target CPU, measure the smallest complete request path: tokenize all
three fields, process the input, produce the complete Verdict including a short
reason and citations, and validate the result. Warm the already-loaded model;
record cold load separately. Report complete-response latency, including p95
and the largest observed value, never just time to first token or verdict
selection. Do not claim the hard limit from an average. Record hardware,
thread count, concurrency, input tokens, and output tokens.

The 100 ms requirement cannot be evaluated without an input-length bound and
specific CPU. Generated explanations are a material cost: 40 output tokens in
100 ms already require 400 tokens/second, before input processing and
validation. That arithmetic is not a measured throughput claim. No existing
source establishes that this candidate meets the complete requirement. A
bounded reason is still required; do not silently replace it with a template
or move it outside the latency budget. If the measured workflow misses the
limits, report the failure and discuss alternatives before an editing run.

Measure deployed artifact bytes and peak process resident memory separately;
clarify whether the 500 MB cap applies to model storage, runtime memory, or
both before claiming feasibility. A Python controller is appropriate for
research and floating-point edits, but its dependency overhead cannot be
ignored when measuring a deployed process. Choose an existing CPU runtime
that fits the agreed accounting; do not build a new inference engine.

Quantization is now an option to establish feasibility rather than a late
optimization. If required, fix one deployment representation for both the
unchanged and edited models. Edit floating-point source tensors offline, then
convert each checkpoint identically. Evaluate the actually deployed artifact:
a weight edit that disappears through quantization is not a deployed gain.
Record conversion settings and hashes. Quantization alone does not establish
the latency target and is not the verification specialization mechanism.

After feasibility and a basic judgment baseline, freeze the prompt, template,
tokenizer, decoding parameters, input limits, and output limits. Use no hidden
reasoning expansion. Compare unchanged and edited checkpoints under the same
CPU deployment protocol. Keep size and full-response latency as advancement
conditions alongside judgment quality; a more accurate candidate that exceeds
either bound does not meet this MVP. Offline editing may use more memory than
deployment; its compute and spend still require a budget decision.

### Direct editing workflow

Use an existing coding-agent session as the editor rather than constructing an
autonomous agent platform. Give it the verification behavior description,
development examples, model configuration, tensor access, and a bounded probe
operation that can return outputs and selected activations. The editor must
explain the targeted behavioral defect and why its edit might address it.

Small research helpers load and inspect tensors, run probes, save a candidate,
and reconstruct it for evaluation. Reuse the authors' edit reconstruction and
hash-checking approach where appropriate; the complete original LLM editing
harness is not published. Do not copy the separate Go experiment's OS-specific
sandbox into Wyrd.

Save candidate tensors separately from the baseline. Check tensor names,
shapes, dtype, finite values, and base hashes before application. Preserve the
architecture and tokenizer. Record changed tensors, changed-value counts,
the patch hash, the editor's explanation, and every charged probe. An edit can
be reverted by selecting the unchanged checkpoint; it never mutates the only
copy. Avoid pickle-based model artifacts.

Proposed initial budget: one editing run, at most 20 development probe batches
of at most 20 requests, and a hard compute/API spend cap chosen before running.
Model inspection uses the same recorded budget when it performs forward
passes. Freeze one final candidate using development evidence only. A budget
exhaustion or failed run remains visible; it does not trigger an unbounded
retry or another model automatically.

### Decisions API comparison

Send the same rendered requirement, evidence, and optional context to Google's
[`gemini-decision-1` Decisions API](https://aistudio.google.com/docs/decisions-api)
with one `choice` question whose options describe `passed`, `failed`, and
`inconclusive`. Its instructions use the same criterion and evidence policy
as the local model. This is a comparison of approaches, not identical model
interfaces.

Keep the returned choice, probabilities, confidence, model identifier when
available, latency, and usage as a separate comparison record. It is not a
complete Wyrd Verdict: the API generates no explanation or citations. Do not
invent a reason, attribute a local model's reason to Google, or treat Google's
choice as ground truth. Compare outcome quality across all three candidates;
compare reason and citation quality only between the two local checkpoints.

Use the documented REST operation with an existing HTTP library for the
offline experiment. The repository has Jev support, but a bounded search found
no Decisions API owner; Google's endpoint is not interchangeable with Jev's
`system_one` wire contract. Avoid upgrading the whole SDK's Google dependency
or adding a production adapter just to make this comparison.

Authentication, safety refusal, quota, timeout, and transport failures are
recorded as unavailable comparisons, not substantive failed judgments. Report
their counts and common-case coverage; do not silently omit difficult cases.
Keep provider confidence separate from measured calibration. Make no new
confidence threshold a release rule in this MVP.

### Evaluation without a training dataset

Propose 40 development cases and 120 withheld cases. Construct explicit records
and controlled variations; no historical tenant data is needed. Cover supported
and violated expectations, insufficient context, contradictions, paraphrases,
negation, and instructions embedded in the evidence. Include semantic cases
so success cannot come entirely from matching numbers or fixed words.

Each case contains the three text fields, expected outcome, expected supporting
locations where applicable, and a short adjudication explanation. Cases get
human review before model outputs are visible; ambiguous cases are resolved or
explicitly labelled inconclusive. Google or a frontier model can propose cases
but cannot be their sole ground-truth authority. Keep labels and explanations
out of model input.

Split by scenario family before paraphrasing, so variants cannot cross from
development into the hidden set. The editor receives no hidden requests,
labels, outputs, or aggregate scores. The evaluator keeps them outside the
editor's workspace and evaluates only after the patch is frozen. Do not claim
that a directory name alone enforces this separation. Once results influence
further editing, that set is development data and a new hidden set is needed.

Report a full confusion matrix, false-pass rate on non-passing cases, false
failure rate on passing cases, abstention on decidable cases, correct handling
of insufficient evidence, malformed-output and invalid-citation rates, and
human-assessed support for reasons. Preserve raw outcomes and post-validation
outcomes so downgrading invalid passes cannot disguise model defects.

Also record warm p50/p95 latency, generated tokens, load time separately,
memory where measurable, provider usage and known costs, and total editing
cost. Google timing includes network/service time; local generation includes
the explanation. Disclose that difference instead of claiming equal serving
economics. Run the same frozen local protocol on both checkpoints; record
seeds and runtime details without claiming perfect determinism.

At this sample size, counts and paired differences are exploratory evidence,
not proof of production safety. The proposed advance criterion is improved
correct decisions without more false passes, a collapse into abstention, or
worse citation/reason support. Freeze quantitative tolerances before the hidden
run; unresolved tolerances prevent a positive advancement claim, not reporting
a completed experiment. Include uncertainty and per-family regressions.

### Implementation footprint and reuse

Proposed private location: `research/verification-model/`, containing one small
Python runner, reviewed development fixtures, and instructions. Use one
run-directory manifest plus JSONL results; keep downloaded weights, patches,
and raw run artifacts in `target/verification-model/`. The withheld corpus is
evaluator-owned until the editing run ends. Add a narrow opt-in `mise` task
when implementation is approved; no such task exists yet.

This directory is proposed because the existing `benchmarks/README.md` lane
measures released server capacity, not model quality or tensor edits. The SDK
is a source of contracts and helpers, not the owner of a training/research
workflow. No changes to its public exports or production dependencies are
proposed.

Inspected reuse points and affected consumers:

| Existing owner | Reuse and limit |
|---|---|
| `crates/wyrd-spec/src/verification/execution.rs::Verdict`, `Citation`, `Locator` | Canonical output and runtime schema; no parallel contract. |
| `sdks/wyrd-sdk-python/src/prompt.rs::verdict` and Python `Prompt.response_schema` | Obtain the canonical schema without adding a schema exporter. |
| `crates/vala/vala-verify/src/blocks.rs::verdict_from` and `blocks/agent.rs` citation handling | Existing production validation owner; leave it unchanged. The research runner's limited text checks are not full server validation. |
| `sdks/wyrd-sdk-python/pyproject.toml` and `uv.lock` | Torch, Transformers, safetensors, HTTP and JSON Schema packages are already represented; verify compatible locked versions at implementation time. A research dependency must be explicit rather than relying silently on a transitive dependency. |
| `crates/skald/skald-spec/src/wire/jev.rs` and `skald-providers/src/clients/jev.rs` | Existing Jev contracts, not a Google Decisions transport. No changes proposed. |
| `examples/finance-release/policy-judge.yaml` | Existing Agent Prompt with `response_format: verdict`; useful eventual integration pattern. |

Required new machinery is limited to tensor editing/probing and comparison:
existing Wyrd owners execute and validate judgments, but do not implement this
weight-editing experiment. No new generic provider interface, model registry,
job scheduler, artifact database, or MCP server is justified by this outcome.

The implementation's focused checks should prove that malformed output and
invalid citations cannot pass, mismatched/non-finite tensor patches are
refused, API failures are distinguished from choices, and hidden cases never
enter an editing probe. A reconstruction check must reproduce the candidate's
tensor hashes. A real model/API comparison is opt-in, credential-dependent
research evidence, not a mocked quality test. Discover the owning format and
lint commands through `mise` when implementing.

## Evidence and rationale

Repository facts were inspected on 2026-11-17:

- [Prompt and Agent authority](../../../../architecture/wyrd-design.md#prompt)
  owns named `verdict` response formats and execution routes. Native
  `system_one` bodies reject the named Verdict format.
- [Verifier authority](../../../../architecture/wyrd-design.md#verifier)
  owns outcomes, missing evidence, and original-evidence citations.
- [Research source](https://rikiyat.github.io/blog/weight-space-programming/)
  reports targeted loop repair in Qwen and no established reasoning improvement
  in the Gemma follow-up. It does not demonstrate a verification specialist.
- [Released checkpoint](https://huggingface.co/RikiyaT/Qwen3.5-0.8B-weight-space-programming)
  supplies a downloadable model; the model card is not verification evidence.
- [Experiment 8 artifacts](https://github.com/RikiyaT/lm_understand-logs/tree/main/exp8_math)
  include edit diffs and reconstruction code, but exclude the complete editing
  harness. Their practice feedback also demonstrates that training-free does
  not mean feedback-free.
- [Jev](https://docs.typesafe.ai/introduction) returns typed decisions, not
  generated explanations. Its published interface is inspiration, not a
  reproducible architecture for our model.
- [Google Decisions API](https://aistudio.google.com/docs/decisions-api) supplies
  hosted typed inference without generated text. Account access, quota, current
  pricing, and runtime behavior have not been tested.

Shared memory search found relevant standing repository preferences but no
additional prior verification-model experiment. No models or APIs were run.

## Decisions and alternatives

- **Established by the user:** text-valued requirement and evidence with optional
  supplemental context, Wyrd output with reasoning, direct weight edits,
  Decisions API participation, and CPU-only deployment below 500 MB and 100 ms. The user has not approved
  model choice, experiment size, compute, or Google being comparison-only.
- **Recommended:** standalone research first. It isolates whether edits help
  and avoids production gateway/provider changes before there is evidence.
- **Deferred alternative:** a full Google-backed Wyrd judgment would need a
  separately attributed explanation/citation step and evaluation of the combined
  result. That adds model calls and can produce a reason that rationalizes an
  incorrect fixed choice; use only if the user wants that backend in the MVP.
- **Revised model recommendation:** start with a 135M feasibility probe because
  memory and latency are hard constraints. A larger model is not an automatic
  fallback. If this probe fails, report whether the failure is quality, memory,
  or full-response latency and return the trade-off to the user before redesign.
- Fine-tuning and teacher-label distillation do not meet the user's selected
  approach. Pure prompt tuning remains the necessary control, not the proposed
  specialization mechanism.

## Standards conflicts

No authority exception is requested or approved. The three text fields are the
research model invocation, not a replacement public Verifier schema. Exact boundaries:

- The phrase "verdict and reasoning" maps to the existing `{outcome, reason,
  citations}` schema; dropping citations or inventing a new wire envelope
  would conflict with `architecture/wyrd-design.md` §Verifier.
- Google's choice is comparison metadata, not an Agent check result. Passing
  it directly as a full Verdict would violate that same contract.
- Offline records are research artifacts, not persisted Wyrd verification
  evidence. An eventual product integration must use an Agent check through
  `wyrd_gateway`, retain server citation validation, and use existing Bifrost
  recording. This proposal adds no server hot-path waits, so it makes no
  server benchmark or hot-path PASS claim.
- Root `AGENTS.md` prohibits OS-specific containment. The upstream Go sandbox
  is not adopted. CPU execution remains portable across supported platforms.

## Open questions

Material before spec approval or a paid experiment:

1. Is Google comparison-only, or must the MVP also offer a full Google-backed
   Verdict? Comparison-only is the current recommendation; user response is
   pending.
2. Specify the target CPU, maximum input length, concurrency, whether the
   500 MB limit covers artifact size, resident memory, or both, and the latency
   acceptance statistic. The proposed 100 ms measurement includes the complete
   reason and citations. Accept a 135M feasibility probe, then choose an editor
   model and hard offline experiment budget. No latency result is assumed.
3. Approve exploratory quality/regression tolerances and the human adjudication
   owner before looking at hidden results. The 40/120 split and probe budget
   are proposed starting sizes, not approved acceptance requirements.

Private implementation choices: exact helper layout, artifact filenames,
compatible package pins, and text-generation plumbing. No need to resolve them
to assess this proposal.

## Handoff

The next step is to settle the material scope decisions and, when the user says
design is complete, finalize these notes. A later specification can authorize
the bounded experiment; neither this design nor the research references grants
implementation, paid API execution, or production deployment approval. Recheck
current contracts and dependencies before implementation. No spec or tasks have
been created, and no code has been staged or committed.
