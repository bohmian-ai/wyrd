# Agent Harness

Wyrd should be easy for coding agents to inspect, use, validate, and repair.
That means contracts must be typed, errors must be structured, and generated
artifacts must be reproducible.

## Agent-Facing Surfaces

Agent-facing APIs include:

- MCP tools
- JSON schemas
- OpenAPI output
- CLI commands with machine-readable output
- structured errors
- docs generated from source contracts
- validation commands in `mise.toml`

Keep these surfaces deterministic and discoverable.

## Tool Contracts

MCP and CLI tools should have:

- stable names
- typed inputs
- typed outputs
- stable error codes
- concise descriptions
- examples where useful
- tests for schema and behavior

Do not make agents parse prose when a typed field can carry the same meaning.

## Validation

Validation should live where the durable contract lives. If multiple surfaces
need the same rule, put it in Rust and expose it outward through Python, HTTP,
MCP, CLI, and schemas.

Agent-friendly validation failures include:

- code
- field
- invalid value when safe to show
- expected format or range
- remediation

Public error catalogs should be generated from derive-backed Wyrd error
metadata. Do not maintain a hand-written error-code list beside the Rust enum.
The enum attributes are the source for JSON catalogs, Python typed exceptions,
docs, OpenAPI fragments, and MCP error descriptions.

## Generated Artifacts

Generated artifacts are part of the harness. Keep generation commands stable and
fail on drift. If a schema, stub, OpenAPI file, or MCP artifact is wrong, fix the
source or generator instead of editing the artifact.

## Prompts And Local Guidance

Repo-local skills and `AGENTS.md` are executable guidance for agents. Keep them
Wyrd-native, concise, and free of stale names. If a convention becomes important
enough to enforce, add a `mise` task or test instead of relying only on prose.
