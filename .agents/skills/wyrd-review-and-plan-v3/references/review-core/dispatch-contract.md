# Harness Review Dispatch Contract

Use the active harness's native delegation mechanism to execute immutable,
read-only specialist assignments. This contract defines identity, isolation,
and evidence requirements; it does not select a provider, model, effort level,
agent role, process runner, or concurrency target.

## Immutable assignment

Each specialist receives:

- review and assignment IDs;
- domain and required capability;
- unique reviewer ID;
- repository root and resolved target SHA;
- digest-bound specialist prompt;
- immutable scoped context containing the review tuple, impact slice,
  authorities, requirements, task criteria, and report contract;
- report destination; and
- prohibitions on writes, project commands, planning, verdicts, remediation,
  user communication, and nested delegation.

Every assignment binds the same target SHA. Prompt and context bytes must be
verified before dispatch. A result cannot satisfy a different assignment by
changing its identity fields.

## Dispatch and independence

Dispatch each baseline and triggered assignment in a distinct review context
with a distinct reviewer ID. The active harness may execute assignments
concurrently or in capacity-bounded waves. Capacity limits never justify
combining or omitting required domains.

The dispatch mechanism must preserve read-only source access, the immutable
assignment context, and schema-constrained output. If the harness cannot supply
a required capability or distinct review context, record the coverage gap and
return `REVIEW_BLOCKED`.

Harness-specific execution metadata may be recorded for diagnostics outside
the authoritative review contract, but it never changes coverage, acceptance,
or independence requirements.

## Result publication

Each specialist returns the shape in
`../schemas/specialist-result.json`. Before publishing the Markdown report,
validate assignment, domain, reviewer, target, prompt digest, candidate IDs,
report content, and static limits. Publish the accepted Markdown and one compact
`<assignment>.attestation.json` atomically beneath `evidence/specialists/`.
The attestation contains `schema_version: 1`, assignment ID, domain, reviewer
ID, target SHA, prompt SHA-256, report path, report SHA-256, and completed
status. It is derived from the validated specialist result and contains no
report body or harness-specific execution metadata.

The root verifies the complete roster and performs all candidate validation,
deduplication, and consolidation. Specialists never assign final finding IDs or
decide the terminal verdict.

## Independent adjudication

When the independent-validation floor in `evidence-contract.md` triggers,
dispatch one additional reviewer that produced no specialist candidate report.
Give it the immutable adjudication packet and
`validation/independent-validation.md`, without a desired verdict. Its concrete
execution environment is chosen by the active harness.
