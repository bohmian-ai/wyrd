# Evaluation

Load for Verifier requirements and checks, Agent preparation steps and
judges, sandboxed code checks, judge quality, scoring, or evidence used by
policy and release decisions.

## Evaluation is a declared Verifier

A `Verifier` declares named evidence `inputs`, optional preparation `steps`
(`extract`, `agent`, `code`), `requirements` that each produce one verdict
through an `assert`, `trace`, `agent`, `code`, or `drift` check, and a `decide`
rule. Evaluation is not a separate Card or Verifier kind. A Service component, Service,
or standalone Agent binds it through versioned `verified_by`; the caller runs
it with `verify` (inline result) or `submit` (queued execution), and a
`schedule` Trigger runs Window Verifiers. Every execution records one
`VerificationResult` with the requirement and step breakdown. Do not create a
parallel evaluation ontology.

Each step and requirement has a Verifier-unique ID, explicit references to
inputs and upstream outputs, retries, a timeout, and an optional `when`.
Validate the step graph for cycles, unknown references, position violations,
and known-schema fields at registration. Aggregation keeps every requirement
outcome; `decide` never lets an average hide a failed requirement under `all`,
and inconclusive requirements count against a `pass_rate`.

Prefer deterministic assertions for equality, ranges, types, subsets, schema,
trace facts, tool use, and workflow structure. Use an Agent check only for a
criterion that cannot be represented honestly by a deterministic comparator.

## Reproducible evidence

Bind every result to:

- the exact subject Card version;
- the exact Verifier Card, its steps, and requirement configuration;
- pinned input digests, extraction version, and, for drift, window and baseline;
- Agent and Prompt versions, effective route, provider/model version,
  decoding parameters, code hash and runtime, and tool-call digests;
- execution, Run, request, trace, and attempt identities;
- retry, abstention, parse, provider, and terminal status;
- the raw or derived evidence permitted by the configured capture and
  sensitive-data policy.

A repeatable assertion uses deterministic ordering and seeds. A remote model
call is not guaranteed repeatable even with a seed; record the provider's
sampling inputs and classify the result as stochastic evidence. A changed
input, rubric, judge, model, prompt, or step graph is a new evaluation input
and is never silently compared as the same experiment.

Context capture is explicit and least-privilege. Full traces or payloads may be
needed for agent diagnosis, but they require sensitive access, bounded size,
redaction, and retention. Missing required context yields indeterminate or data
quality failure, never a pass.

## Agent judges

- A check-position Agent uses only the tools it declares, each under the
  executing principal's ordinary grants, and may mount selected files of an
  exact Artifact version in a read-only workspace; live span data it reads
  stays outside the pinned snapshot. It returns the closed Verdict schema
  `{outcome: passed | failed | inconclusive, reason, citations}`; parsing or
  missing-evidence failure is `inconclusive`, never a negative judgment.
- A pass needs at least one citation the server validates against pinned
  original evidence; preparation steps pass their citations forward through
  explicit inputs.
- Calibrate against representative human-labelled examples. Report agreement,
  disagreement, abstention, retry, parse-failure, and subgroup behavior.
- Test position, verbosity, reference, provider, and model-version sensitivity.
- Keep bounded retries visible. A retry that changes a judgment is reliability
  evidence, not noise to discard.
- Do not let a judge invoke an undeclared or ungranted tool, mutate the
  subject, retrieve unrestricted context, or decide deployment solely from an
  opaque score.

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

Reject incompatible-score comparison, silent requirement skipping, safety-critical
failure hidden by an average, unbounded step fan-out, provider calls without
deadlines, retries omitted from results, a judge with production mutation
authority, and a green result with missing required traces. Exhausted,
cancelled, indeterminate, and failed checks remain distinct terminal states.

## Stable Wyrd anchors

- Verifier contract: `architecture/wyrd-design.md` §Verifier.
- Authoring types: `crates/wyrd-spec/src/card/verifier/`.
- Result types: `crates/wyrd-spec/src/verification/execution.rs`.

## Primary grounding

- [NIST AI RMF 1.0](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.100-1.pdf)
- [HELM](https://arxiv.org/abs/2211.09110)
- [OpenTelemetry traces](https://opentelemetry.io/docs/concepts/signals/traces/)
