# TASK-010 Gateway aliases

Spec: `changes/active/example-journey-fixes/spec.md` r6, REQ-009 aliases and
the alias case of AC-006. Depends on TASK-008.

## Outcome

A tenant defines `luna -> openai/gpt-6-luna`; a caller sends model `luna`;
authorization, pricing, fallback, and capture apply to `openai/gpt-6-luna`
and the call row records the requested alias and the resolved target.

## Scope

- Alias set as a gateway administration resource following the existing
  credential/deployment/policy pattern: `wyrd-spec` contract, SQL storage
  (edit defining migration in place), service methods with
  `gateway:write`/`gateway:read`/`gateway:delete` audit, HTTP routes,
  `wyrd gateway alias put|get|list|delete`, Rust/TS/Python client methods,
  MCP if gateway admin is exposed there.
- Validation: alias name has no `/`; target must be `<deployment>/<model>`;
  a deployment may not be named like an existing alias and vice versa.
- Snapshot carries aliases; resolution happens before planning.
- `/v1/models` lists aliases.

## Evidence

- Journeys in Rust, TS and Python: put alias, call through it, assert
  resolved deployment and model; alias to a `granted` deployment refuses an
  `all`-only principal; alias collision with a deployment name is refused.
- Lanes as TASK-008.
