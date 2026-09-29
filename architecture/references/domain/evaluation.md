# Evaluation

Load for Eval-backed Verifiers, scenario execution, deterministic and
LLM-based checks, judge quality, scoring, or evidence used by policy and
release decisions.

## Evaluation is a declared workflow

Eval is a `Verifier` implementation (`implementation.kind: eval`), not a Card
kind. The Verifier is reusable and subject-less and declares a typed DAG of
checks; a Service component, Service, or standalone Agent binds it through
versioned `verified_by` with an `observations_ready` Trigger. Runtime
observation `card_ref` supplies subject identity. Each committed
`vala.eval.observations` record activates one run per matching active binding
of that subject, and the run records the common Verification Result plus Eval
item details. There is no Eval pull protocol; a future offline dataset route
must be Verifier-backed. Do not create a parallel evaluation ontology.

Each task has a stable ID, typed comparator, input selector, expected value,
dependencies, retry budget, and failure disposition. Validate the DAG for
cycles, depth, fan-out, inaccessible paths, and hidden network work before
execution. Aggregate results retain every task result and criticality; a total
score never hides a required failed assertion or missing evidence.

Prefer deterministic assertions for equality, ranges, types, subsets, schema,
trace facts, tool use, and workflow structure. Use an LLM judge only for a
criterion that cannot be represented honestly by a deterministic comparator.

## Reproducible evidence

Bind every result to:

- the exact subject Card version;
- evaluator Card/configuration and task graph;
- dataset/source identity, selected records, ordering, and time window;
- prompt, judge Card, provider/model version, decoding parameters, and seed
  where applicable;
- scenario, Run, Observation, request, trace, and attempt identities;
- retry, abstention, parse, provider, and terminal status;
- the raw or derived evidence permitted by the configured capture and
  sensitive-data policy.

A repeatable assertion uses deterministic ordering and seeds. A remote model
call is not guaranteed repeatable even with a seed; record the provider's
sampling inputs and classify the result as stochastic evidence. A changed
dataset, rubric, judge, model, prompt, or task graph is a new evaluation input
and is never silently compared as the same experiment.

Context capture is explicit and least-privilege. Full traces or payloads may be
needed for agent diagnosis, but they require sensitive access, bounded size,
redaction, and retention. Missing required context yields indeterminate or data
quality failure, never a pass.

## LLM judges

- Use one versioned Prompt Card per judge task with a closed output schema.
- Require `pass`, `fail`, and `indeterminate`/`abstain` outcomes; parsing or
  missing-evidence failure is not a negative judgment about the subject.
- Calibrate against representative human-labelled examples. Report agreement,
  disagreement, abstention, retry, parse-failure, and subgroup behavior.
- Test position, verbosity, reference, provider, and model-version sensitivity.
- Keep bounded retries visible. A retry that changes a judgment is reliability
  evidence, not noise to discard.
- Do not let a judge invoke production tools, mutate the subject, retrieve
  unrestricted context, or decide deployment solely from an opaque score.

Pairwise comparison is appropriate only with controlled ordering and a
decision that is genuinely relative. Reference-based grading requires a valid
reference. Otherwise use a rubric and disclose uncertainty.

## Decision use

Evaluation produces evidence. Policy, release, or human review consumes that
evidence under an explicit risk rule. Separate evaluator validity (measures the
intended property), reliability (stable enough for the decision), coverage
(represents the operating distribution), and subject performance. Offline
quality does not prove production availability, security, latency, or drift.

## Failure modes

Reject incompatible-score comparison, silent task skipping, safety-critical
failure hidden by an average, unbounded task fan-out, provider calls without
deadlines, retries omitted from results, a judge with production mutation
authority, and a green result with missing required traces. Exhausted,
cancelled, indeterminate, and failed checks remain distinct terminal states.

## Stable Wyrd anchors

- Eval contract: `architecture/wyrd-design.md` §Verifier (Eval implementation).
- Eval types: `crates/wyrd-spec/src/vala/eval/`.
- Engine: `crates/vala/vala-eval/`.

## Primary grounding

- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- [HELM](https://arxiv.org/abs/2211.09110)
- [OpenTelemetry traces](https://opentelemetry.io/docs/concepts/signals/traces/)
