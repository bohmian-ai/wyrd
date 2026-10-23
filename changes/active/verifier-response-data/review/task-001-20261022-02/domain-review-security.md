# Security, tenancy, and output trust-boundary review

Result: **FAIL**. Proposed finding: **SEC-001**.

Subject: repository `/home/thorrester/Documents/GitHub/wyrd`, base
`4d0f7005888a899910008399d1b5e15e2a3207f9`, candidate
`9e56fc340cb5fd769d3b34f47433e2719339fdfb`; complete cumulative range reviewed
within this domain. HEAD still equals the candidate. The three pre-existing
workflow/skill modifications identified in `subject.md` remain untouched.

## Authority and source coverage

| Boundary | Authority | Source and assessment |
| --- | --- | --- |
| Declared formats and exact returned-output validation | Spec REQ-001/002/007; Wyrd Prompt/Verifier authority; security posture trust boundaries | `skald-spec/src/authoring.rs`, `prompt.rs::check_output_schema`; `wyrd-spec/src/verification/execution.rs::Verdict::response_format`; `card/verifier/validate.rs`; `vala-verify/src/blocks.rs::verdict_from`. Named formats close the envelope, validate the custom schema and require its object; code/Agent consumers validate exact outputs server-side. JSON Schema dependency has default features disabled (`Cargo.toml:120`), so this does not introduce default HTTP/file reference fetching. |
| Rejection, citations, diagnostics, and retained payload | Spec REQ-007/011; security privacy controls | Agent and code `CheckExecutor::execute`, `blocks/engine.rs::check`, `Judged::{inconclusive,report,cited}`, `verification/results.rs::{build,summary}`. Invalid output takes the existing inconclusive branch with no `data`; accepted data uses the existing outcome Variant. Citation filtering and Agent unsupported-pass downgrade remain. Raw diagnostic instance paths are the exception below. |
| HTTP/MCP read authority and tenant binding | Security posture permission model and Tenant and data isolation; spec REQ-011/012 | `components/verification/service.rs::get_execution` authorizes cards read, obtains caller-bound `TenantConn`, and reads the selected result using `ScheduledQueryCaller::authenticated`; `read_result` selects outcome through normal Bifrost admission. HTTP `components/verification/routes.rs::get_execution` and MCP `mcp/verification.rs::mcp_get_execution` share that owner. No new route, permission bypass, tenant selector, or storage writer was introduced. |
| Published Bifrost reads | `architecture/bifrost-design.md:1481–1484`; security posture tenant isolation | `tables/verification/results.rs` stores sensitive outcome unchanged; `oracle/mod.rs::authorize_payload_columns` scopes the additional gateway payload permission to `vala.gateway.calls`. Table query permission governs result outcome. New data receives exactly its containing outcome's controls. |
| Typed SDK reads and diagnostic privacy | Spec REQ-012; SDK AGENTS; `agent_docs/errors.md` | Rust ordinary verify/status/wait followed by `decode_data`; Python ordinary calls followed by `decode_data`; TS ordinary calls followed by `typedResult`. No typed operation executes a second verification or mutates durable results. Decoder messages are suppressed, but dynamic location keys can still carry payload bytes into diagnostics. |
| Evidence | Task scenario/evidence tables; `TESTING.md` | Inspected HTTP `custom_verdict_data_is_tenant_isolated`, MCP `custom_verdict_data_round_trips`, Agent `check_agent_validates_custom_data`, SDK readback/negative/typed journeys and recorded checks. Implementation records focused pure tests, journeys, and checks; those are not this reviewer's independent reruns. |

## SEC-001 — Payload-derived map keys escape into rejection and decoding diagnostics

- Classification: **INCORRECT**.
- Obligation: REQ-007 forbids echoing rejected data; REQ-011 forbids copying custom data into validation diagnostics; REQ-012 permits only a **safe** field path, without the payload.
- Primary changed location: `crates/vala/vala-verify/src/blocks.rs:201–205`, `verdict_from` formats `error.instance_path()` directly into its terminal reason.
- Sibling changed locations: `crates/wyrd-spec/src/verification/execution.rs:515–522` formats the Serde path; `sdks/wyrd-sdk-python/src/verify.rs:340–365` copies Pydantic-style `loc`; `sdks/wyrd-sdk-ts/wyrd/src/index.ts:283–292` copies parser issue `path`. These all assume that every path segment is a safe schema field, although supported dictionary shapes contain keys originating in the returned object.
- Reachable producer: a code check may declare `data: {type: object, additionalProperties: {type: integer}}` and return an otherwise valid Verdict with `data: {"customer-secret@example.test": "wrong-type"}`. The schema is valid and explicitly permits arbitrary map keys. The type error occurs at `/data/customer-secret@example.test`; this is a response key, not a schema-declared field. The diagnostic emits the secret verbatim even though the rejected object is removed. The same issue occurs inside nested dictionaries and accepted data later decoded to `HashMap<String, u32>`, a Pydantic dictionary of integers, or an ordinary dictionary parser that reports dynamic locations.
- Producer-to-consumer trace: `CodeExecutor::execute` (`hosted/sandbox.rs:377–379`) and `AgentExecutor::execute` (`blocks/agent.rs:480`) share `verdict_from`; the engine converts its terminal error to `Judged::inconclusive` (`blocks/engine.rs:630`), copies the reason into `RequirementResult` (`:918`), and `ResultPayloadBuilder::summary` serializes the outcome (`verification/results.rs:287–290`). Thus rejected data bytes survive in the direct HTTP/MCP result and retained/published result reason despite `data: None`. Typed conversion separately copies the same class of dynamic key into public error message/details. The concrete server path alone is sufficient to violate acceptance.
- Existing alternative/owner: keep the shared `verdict_from` as the single server rejection owner; it already has a schema-owned `schema_path()` that need not contain the response's arbitrary dictionary keys. Do not add guards to HTTP, MCP, persistence, or each check executor. SDK decoder boundaries own their own local error projection; keep the existing error code and suppress unsafe path segments there.
- Smallest testable correction: emit only schema-derived/static field locations or a safe containing/root location when a location depends on arbitrary payload keys. Preserve useful schema-declared field paths, requirement identity, inconclusive semantics, data omission, existing error codes, and ordinary/typed result behavior. Do not expose the underlying validator message or invent new permissions, decoder APIs, or storage machinery.
- Focused closure proof: use a valid dictionary schema and a secret marker **as the map key**, with a wrong-type value. Check both code and Agent rejection reasons and the serialized result contain no marker and no data. Exercise each SDK's existing typed conversion boundary with a dictionary mismatch and assert the catalogued error's message/details contain no marker while the ordinary stored result remains intact. Current privacy tests put the marker only in values (`blocks_agent.rs:633–650`), so they cannot catch this path-key leak. No broad journey suite is needed to demonstrate this narrowly defined gap.

## Payload-permission authority reconciliation

The implementation's query-read behavior is consistent with the narrow Bifrost
owner: `architecture/bifrost-design.md:1481–1484` explicitly states that sensitive
metadata is descriptive, table query permission governs every column, and only
gateway call request/response payloads additionally require gateway payload
read. The old `ResultsTable` comments claiming an elevated payload gate do not
override that owner. REQ-011 says data *inherits* its containing outcome's existing
protections, and the task repeatedly requires preserving existing controls.
There is therefore no source-backed authorization regression to fix by adding a
new result permission. Adding one would change approved existing behavior and
require a security decision; it is not a bounded implementation correction.
The spec's reference to inherited elevated protection should be reconciled with
the owning authority, without treating a new result-payload permission as an
implicit acceptance requirement. This is an authority clarification, not a
second proposed security finding.

## Updated-candidate re-audit

This is the full original-base-to-updated-candidate security audit, not review
of the last commit alone. The verdict/result/decoder/security owners in the
coverage table are byte-identical between the first and updated candidate,
confirmed by the commit-to-commit diff. I re-read the committed shared rejection
and all three typed error projections with `git show` and retained SEC-001 as an
unclosed proposed finding. There is no previous validated FIND ID or remediation
closure to claim. The approved security and Bifrost authority is unchanged.

The added `HttpAuth.auth_flow = sync_auth_flow` in
`sdks/wyrd-sdk-python/python/wyrd/client/_http_auth.py:54` reuses the existing
per-request token and header-scrubbing path, without caching a bearer or changing
which headers carry credentials. The synchronous method directly calls
`_authenticate`; it does not delegate back to `auth_flow`, so the assignment
introduces no recursion. The async method still fetches the token in
`asyncio.to_thread` and uses the same `_authenticate` owner. Both httpx and httpx2
clients receive the same existing boundary behavior. Existing mock-transport
unit tests cover synchronous httpx/httpx2 and asynchronous token replacement.

The new Python contextmanager annotations change Iterator/AsyncIterator to
Generator/AsyncGenerator without changing bodies, lifetimes, cleanup, login
redaction, tenant identity, or transport behavior. The typecheck assertion adds
`.get("data")` coverage. The lockfile upgrades development typechecker
`ty` from 0.0.39 to 0.0.91 with pinned registry artifacts and hashes; no runtime
credential library or auth dependency is added. No new domain finding arises
from these changes.

Independent frozen/read-only checks on this attempt:

| Command | Observation |
| --- | --- |
| `mise exec -- uv run --no-sync --offline --project sdks/wyrd-sdk-python python -B -m pytest -p no:cacheprovider -q sdks/wyrd-sdk-python/tests/unit/client/test_http_auth.py` | PASS, 3 tests in 0.08s. |
| Existing Pydantic, through `uv run --no-sync --offline`, validating a `dict[str, int]` field with a marker key and wrong-type value | Its ordinary error location is `scores.customer-secret@example.test`; the candidate's `error_path` copies this exact dynamic key into the Wyrd diagnostic. This demonstrates the sibling SDK part of SEC-001 without changing source or installing dependencies. |
| `git rev-parse HEAD` | `9e56fc340cb5fd769d3b34f47433e2719339fdfb`. |

## Verification limits

This was a static, independent domain audit. I did not change source or dependencies, run broad
tests, or independently rerun the database-backed journeys. No cross-tenant
access path or new authorization bypass was found. The material remaining risk
is rejected/decoded dictionary keys appearing in diagnostics and retained
requirement reasons.
