# TASK-001 Jev as a typed provider

Spec: `../spec.md` REQ-001, INV-001, INV-002, INV-004, AC-001..AC-003.

Rework the rejected Jev implementation (commit "Add the TypeSafe Jev gateway
provider" on this branch) so Jev is modeled exactly like the existing typed
providers: `ProviderRequest`/`ProviderResponse` variants with typed wire
structs in `skald-spec` (request: model, state, questions keyed by name with
typed noul/choice/score question variants, instructions, criteria; response:
model, typed answers per question type, usage), the provider client, the
gateway adapter and native `/v1/systemone` route, Prompt Card support for a
native Jev request dialect, generated schemas, and Rust/Python/TypeScript SDK
types. Remove the chat-completions JSON-in-message shim and every raw
`Value` body standing in for Jev.

A Verifier `llm_judge` with a Jev Prompt must work, and a dependent assertion
must threshold a typed answer field. Prompt variables containing quotes and
newlines must render a valid request (prove it end to end; the prior
implementation's quote refusal must become a success).

Evidence: unit tests for wire round-trips and error mapping; gateway journeys
in Rust, Python, and TypeScript (native route, judge Verifier pass and fail,
quote in variables, invalid question type, upstream 400/429/5xx); updated
fixtures; codegen check.
