# Remove the public SDK Agent runtime

- Change: `agent-runtime-composition`
- Specification: `SPEC-agent-runtime-composition`, approved revision 3
- Completed: 2026-10-16
- Reviewed target: `9be085c6c` (TASK-001 with its R1 remediation, task
  re-review PASS), approved by the human owner in place of a final change
  review
- Delivery reference: not supplied

Wyrd is verification infrastructure, not an agent framework. This change
removes the executable Agent and Run invocation from the Rust, Python, and
TypeScript SDKs. Applications own their inference and call Wyrd's gateway or
their framework of choice. Agent Cards stay declarative and registrable, and
the internal Skald runtime still executes Agents inside server-owned Verifier
workflows.

## Shipped behavior

- Python no longer exports `wyrd.agent` or its runtime companions: the
  executable `Agent`, run results, local `@tool` registration, lifecycle
  callbacks, sessions, the `agent`/`tool`/`providers` native submodules, and
  the mock `ProviderResponse.text` constructor. The Skald crates' `python`
  features that only registered them are gone.
- `Run.invoke`/`invoke_on` are deleted from `wyrd-client` and every SDK
  projection, together with the shared client's gateway caller. No renamed
  execution convenience replaces them.
- `AgentCard`/`AgentSpec`, registration, hydration, `state.agent(...)`,
  Prompt authoring and `Prompt.render`, verification bindings, and the shared
  error codes are unchanged.
- The examples and journeys that used Run invocation now render the Agent's
  Prompt themselves and call the gateway's native ingress. They attach the
  existing `wyrd-run-id`/`wyrd-card-uid` headers, so captured calls keep their
  caller, Run, Card, and deployment. Gateway refusals reach the application
  as gateway responses with their stable codes. The support-desk examples
  bound each call at ten minutes and insert questions literally, and the
  reference app returns only catalog titles or fixed text for refusals and
  local failures.
- The opt-in live smoke lane still proves authored portable and native
  Prompts lower to requests current providers accept, now by posting the
  rendered native body straight to each provider.

## Invariants and constraints

- Durable gateway authorization, capture, accounting, and audit stay on the
  server. Provider credentials stay server-owned, and subject headers are
  captured, never forwarded upstream.
- Server-controlled execution still refuses native routes. Skald stays
  independent of Wyrd client and server mechanics, and no execution moved
  into a language binding.
- No Card kind, persisted Agent contract, wire schema, or permission semantics
  changed.

## Decisions and deviations

- Public SDK Agent execution was removed rather than expanded to three
  languages. This replaced the composition objective of revisions 1–2, which
  never ran.
- D1: Run invocation was deleted rather than kept as a correlated Prompt
  convenience. Applications own inference and may supply correlation headers.
- An Agent-tagged call sends the existing `wyrd-prompt-provider` header so the
  gateway still refuses an incompatible adapter before dispatch.
- TypeScript has no Prompt render API, so its callers bind the Prompt's
  question placeholder themselves.
- Retired with the API, because the API was their only subject: the
  Agent-runtime unit suites, the live smoke's local-tool-loop case, and the
  Run-invocation-specific refusals.

## Acceptance closure

| Criterion | Evidence |
|---|---|
| AC-007 runtime surfaces absent | Import and attribute probes, `py:typecheck`, `py:lints`, `check:py-wheel-no-testing`, `ts:pack:check`, `ts:napi:check` |
| AC-008 declarative and internal Agent preserved | Card hydration and Verifier preservation journeys in all three SDKs, `test:skald`, `test:shared` |
| AC-009 Run invocation absent, workflows migrated | Support-desk, gateway-inference, and local-development journeys in all three SDKs, asserting captured Run/Card/caller/deployment and viewer or grant denial with no upstream IO |
| AC-010 integrations intact | External-framework and stock-client gateway journeys unchanged, `check:deps` |
| AC-011 boundary documented | `docs:check`, `check:examples`, format, lint, and type lanes |

## References

- [Wyrd design](../../../architecture/wyrd-design.md) and
  [doctrine](../../../architecture/wyrd-doctrine.mdx): user code owns
  inference; Wyrd owns verification.
- [Call Wyrd from a stock client](../../../docs/src/content/docs/connect/call-wyrd-from-a-stock-client.md)
- [Runs, observations, and evidence](../../../docs/src/content/docs/understand/runs-observations-and-evidence.md)
- [Support desk examples](../../../examples/support-desk) and
  [reference app](../../../examples/support-desk-app/rust/lib.rs)
- [Rust gateway-inference journey](../../../sdks/wyrd-sdk-rust/tests/integration/gateway_inference.rs),
  [Python](../../../sdks/wyrd-sdk-python/tests/integration/test_gateway_inference.py),
  [TypeScript](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/gateway-inference.test.ts)
- [Live Prompt smoke](../../../sdks/wyrd-sdk-python/tests/smoke/test_prompt_live_providers.py)
