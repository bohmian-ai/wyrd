# Evaluation

Load for Eval Cards, agent or model quality measurement, judge design,
scenario execution, or evidence needed for a release or policy decision.

## Make evaluation a declared workflow

An `Eval` Card names one `subject_ref` and a typed workflow of checks. Keep
deterministic assertions, trace assertions, and agent-behavior assertions in
the same model; use an LLM judge only when a deterministic comparator cannot
express the criterion. Dataset-driven evaluation exercises reproducible
scenarios. Archived or online evaluation reads observations through a typed
source and keeps the same subject identity.

Each check should define its input path, expected value, comparator, retry
budget, and failure disposition. Dependencies form a bounded DAG; cycles,
unbounded fan-out, and hidden network calls are validation errors. Aggregate
scores must retain per-check evidence so a passing total cannot hide a critical
failure.

## Evidence and repeatability

- Version the subject Card, prompt/judge Card, dataset Card, and evaluator
  configuration. A score without those identities is not reproducible.
- Capture the Run and Observation context for every scenario. Store the raw
  response or trace payload only under the appropriate sensitive permission.
- Separate evaluator validity (does the check measure the intended property?)
  from evaluator reliability (does it produce stable results?). Calibrate
  judges against human-labelled examples and report agreement, abstentions, and
  retries rather than a single opaque score.
- Use deterministic seeds, explicit sampling, fixed time windows, and stable
  ordering. Treat a changed dataset or rubric as a new evaluation input.
- Map results to risk decisions: a score is evidence for a policy or release
  choice, not an automatic guarantee of safety or correctness.

## LLM judges

Prefer rubric prompts with a closed output schema, bounded retries, and a
separate judge Card. Test position, verbosity, and model-version bias. Require
an abstain or indeterminate result when evidence is missing. Never allow a
judge to execute production tools or mutate the subject as part of scoring.
Use pairwise or reference-based comparison only when its assumptions fit the
decision; otherwise report the limits explicitly.

## Failure modes and anti-patterns

Do not conflate offline scenario quality with production reliability, aggregate
away a safety-critical assertion, compare scores from incompatible rubrics, or
let a flaky remote provider decide a deployment gate without an evidence trail.
Retries can hide instability, so expose retry counts and classify exhausted
checks separately from genuine failures. A green score with missing traces is a
data-quality failure, not success.

## Stable Wyrd anchors

- Eval contract and subject direction: `architecture/wyrd-design.md` §Eval.
- Eval IDs, status, and operators: `crates/wyrd-spec/src/vala/eval/`.
- Evaluation engine and observation persistence: `crates/vala/vala-eval/` and
  `crates/vala/vala-sdk/`.
- Governance and audit surfaces: `crates/wyrd/wyrd-server/`.

## Primary grounding

- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- [HELM: Holistic Evaluation of Language Models](https://arxiv.org/abs/2211.09110)
- [lm-evaluation-harness](https://github.com/EleutherAI/lm-evaluation-harness)
- [OpenTelemetry traces](https://opentelemetry.io/docs/concepts/signals/#traces)
- Wyrd anchors: `architecture/wyrd-design.md` §Eval;
  `crates/wyrd-spec/src/vala/eval/`; `crates/vala/vala-eval/`.
