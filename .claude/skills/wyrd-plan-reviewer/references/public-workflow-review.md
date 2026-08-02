# Public Workflow Review

Load this reference only when the artifact changes how a developer or agent
discovers, configures, invokes, composes, debugs, or recovers from Wyrd.

Trace the primary changed journey:

| Persona | Goal | Entry point | Success path | Likely failure and recovery |
|---|---|---|---|---|

Review the smallest successful use, common repeat use, one likely failure, and
continuation from durable state when relevant. Record unchanged workflows in
one sentence; do not reconstruct them.

## Critical

Use only when the workflow is likely to mutate the wrong durable state, cross a
tenant or permission boundary, expose a secret, or present a destructive action
as read-only.

## Major

Use only when a primary workflow:

- requires private service or crate knowledge to use correctly
- exposes inconsistent durable nouns, fields, lifecycle, or error semantics
  across first-class surfaces
- requires manual reconstruction of a contract Wyrd already owns
- has hidden ordering or side effects likely to produce incorrect state
- fails without a reasonably discoverable recovery path
- omits the first-class SDK or agent-facing path the capability claims to ship

Do not report taste-based API preferences, optional convenience helpers,
localized boilerplate, example polish, or minor naming friction. A wire
contract alone is not automatically poor SDK design, and an ergonomic helper
must not duplicate server-owned durable behavior.
