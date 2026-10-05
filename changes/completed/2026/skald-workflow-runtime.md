# Skald workflow runtime

- Change ID: `SPEC-skald-workflow-runtime`
- Completed: 2026-10-04
- Reviewed target: `395a3ad91a1dfa0706bc8c08c8dc3100dce3d78d`
- Delivery reference: not supplied

## Intent and value

Wyrd now has one declarative Workflow model and one reusable Skald execution
engine for explicit multi-Agent DAGs. Users can author a Workflow once, run it
locally from Rust, Python, TypeScript, or the CLI, register its exact Card graph,
load the pinned graph again, and submit a bounded accepted run to `wyrd-server`
through Rust, HTTP, or the CLI.

The result is portable, attributable execution without making Workflow a
principal, duplicating runtime behavior in clients, or turning Wyrd into a
general-purpose arbitrary-code workflow platform.

## Shipped behavior

- A normal `wyrd/v1` Workflow Card declares Agent steps, explicit dependencies,
  exact input bindings, route selection, retry/timeout policy, and named output
  bindings. Dependencies order work but never forward data implicitly.
- The Skald runtime validates one plan, runs ready steps with bounded
  concurrency, preserves namespaced results, applies deterministic retry and
  deadline precedence, drains owned work on cancellation, and always produces a
  complete terminal `WorkflowRun` snapshot for accepted runtime failures.
- `Workflow::from_path`/`Workflow.from_path`/`Workflow.fromPath` load authored
  bundles through the shared loader and Cards hydrator. Fully local bundles need
  no server. Registered references are resolved lazily and remain pinned to
  exact versions and UIDs.
- `cards.workflow().load` and its Python/TypeScript projections load registered
  Workflows through locked relationships. Composite registration validates the
  resolved graph before writing and preserves sibling-versus-external
  provenance.
- Local execution supports `Native`, `WyrdGateway`, and `ExtGateway`. Server
  execution supports the two gateway routes and refuses `Native` before work
  starts.
- The public gateway carries an authenticated, bounded, consumed fallback
  header. External gateway calls use the shared screened and pinned egress
  policy with execution-environment-owned bindings and selected secret
  resolution at run start.
- `wyrd-server` exposes process-local accepted Workflow runs with create, get,
  cancel, idempotent replay, bounded preparation, pinned graphs, bounded
  snapshots, global and per-tenant admission, retention, eviction, and
  shutdown drain.
- Accepted jobs capture token-free principal attribution and scopes once.
  Token expiry or later grant changes neither cancel nor widen the run; Cards,
  Bifrost, and gateway calls still authorize and audit at their owning live
  boundaries.
- Server Agents may use only `bifrost.query` and `cards.get`, both through their
  existing service owners. Local Agents may use only caller-supplied tools that
  their Card declares.
- The CLI ships `wyrd workflow run`, `status`, and `cancel`. Server submission
  prints the run ID before waiting; detach and interruption leave accepted work
  active.
- Provider requests use adjacent provider tags with one variant per wire
  schema. `Prompt.provider` is the optional native destination. Vertex uses a
  Gemini GenerateContent body with destination `vertex`; the local public
  gateway refuses Vertex while the in-process server gateway supports it.
- The obsolete Skald Observer plugin system and obsolete Workflow loader/graph
  machinery were removed in favor of semantic tracing and the shared owners.

## Lasting invariants and constraints

- `wyrd-spec` owns pure, synchronous, PyO3-free Workflow contracts, schemas,
  validation, and stable errors.
- Skald owns the one asynchronous Workflow algorithm. The server hosts it and
  adds resolution, tenancy, authorization, audit, resource bounds, and
  process-local lifecycle; language SDKs remain thin projections.
- Workflow Cards are declarative and are never principals. Invocation authority
  belongs to the caller and reused Cards transfer no credential or privilege.
- `wyrd-client` is the shared SDK-facing owner. Existing Cards hydration,
  loader parsing, Prompt binding, transport, and Skald execution are reused;
  there is no second graph, traversal, validator, renderer, transport, or
  executor.
- Registered graphs never float. Acceptance pins the exact active Workflow,
  Agent, and Prompt closure before execution.
- Route choice is immutable versioned behavior. Invocation input cannot replace
  it, and route selection never mutates the Prompt request contract.
- Gateway fallback, remaining deadline, cancellation, and correlation are
  immutable per-call values. Workflow correlation is trace-only.
- External egress uses one lower shared policy. Native provider transport and
  Vault/Operator security boundaries remain outside this change.
- Accepted runs are bounded, deployment-affine, and intentionally ephemeral.
  Restart loses them; no durable queue, cross-replica registry, recovery, or
  automatic provider replay is implied.
- Preparation is tracked through atomic publish or exact cleanup. Runtime state
  locks guard only short in-memory mutations and are not held across IO or
  drains.
- Complete snapshot accounting includes metadata and bounded errors and
  reserves enough space for payload-free terminalization of every step.
- Server jobs retain no bearer token or refresh secret. Fresh create/replay,
  get, and cancel requests authenticate independently.
- Rust, Python, and TypeScript support local authored and registered runs.
  Server-run lifecycle remains Rust, HTTP, and CLI only; MCP has no Workflow
  surface.

## Material decisions and rationale

- One Card and one engine serve local and server workflows so behavior cannot
  drift across execution locations.
- Steps invoke Agents only; Prompt execution and tools remain inside the Agent
  and its existing registries.
- Explicit bindings and outputs replace implicit handoff and last-step-wins
  behavior, keeping concurrency deterministic and wire contracts portable.
- Accepted work is process-local because a durable distributed scheduler was
  not needed for the requested workflow and would add a materially different
  product boundary.
- Server execution captures bounded authority instead of retaining or renewing
  the request token, allowing accepted work to finish without widening its
  original permissions.
- Gateway and external calls remain owned by the executing process. Translation
  stays gateway-owned; external gateway credentials stay environment-owned.
- Query tools bind the prepared run deadline once; an authored shorter
  `deadline_ms` wins. Cancellation settles through the existing query owner.
- A Running step reserves attempt 1, and interrupted work settles Cancelled.
- Oracle drain polling and idle refusal remain deleted. Analytical followers
  release on grant-stream close without an acknowledgement protocol.

## Approved revisions and deviations

- Revisions 10 and 11 fixed public seams and replaced the Observer system with
  semantic tracing. Revision 12 fixed shared loading, SDK, accepted-job, and
  reuse contracts. Revision 13 tagged `ProviderRequest`. Revision 14 reduced
  each wire schema to one variant, made `Prompt.provider` the optional native
  destination, and represented Vertex as GenerateContent with destination
  `vertex`.
- Human decision (follow-up): `wyrd workflow` commands take no `--server`
  option. The endpoint and credential come only from the ambient client
  configuration, as the SDKs and authored-file loader resolve them; other CLI
  commands keep their own `--server`.
- Rust and TypeScript SDK journeys register through Cards; the compiled CLI
  journey owns the independent `wyrd apply` proof. Python uses the installed
  CLI. TypeScript packages the Skald dependency cone. Development-only
  wiremock allowlist entries use the existing approved mechanism.
- The review follow-ups are closed: invalid execution choices are refused
  before the input file is read, Ctrl-C listener failure reports
  `WYRD_WORKFLOW_500_INTERNAL`, GenerateContent media refusals name the
  Prompt's effective provider, the unused request-only cache-key constructor
  is deleted, and the external-gateway success-response credential scan is
  removed in favor of standard secret handling and error/log redaction. The
  stale Revision 13 task authority and the `new Cards()` TypeScript example
  lived only in the removed active packet and closed with completion.

## Acceptance closure

| Area | Evidence | Result |
|---|---|---|
| Contract, DAG, explicit bindings, results, retries, deadlines, cancellation, and tracing | Contract/runtime unit and integration evidence recorded for TASK-001, plus target-bound abort-fence closure | PASS |
| Shared loading, exact graph registration, provenance, and Rust/Python/TypeScript local journeys | Cumulative TASK-002 PASS and later integrated SDK/CLI journeys | PASS |
| Shared remote client, public gateway context, native renewal, cancellation, and route-selected local configuration | TASK-003 evidence and target-bound Python contract closure | PASS |
| Server admission, authority, tools, idempotency, lifecycle, bounds, protocol matrix, and Oracle settlement | TASK-004 focused server/gateway/Oracle evidence; diagnostics/docs follow-ups closed | PASS |
| CLI, team reuse, all first-class SDK journeys, architecture, and aggregate repository verification | Green final aggregate at `b88102317`, three compiled CLI journeys, SDK journeys, docs check, and patch hygiene; CLI edge follow-ups closed | PASS |
| Revision 14 provider wire contract | Schema/round-trip tests, Python projection, local Vertex refusal, and server custom-Chat/Vertex journeys | PASS |
| Served Workflow OpenAPI routes | `pg_openapi_contract.rs` asserts the `/v1/workflow-runs*` operations in the served document | PASS |

## Current authority, owners, and tests

- [Wyrd design](../../../architecture/wyrd-design.md)
- [Wyrd doctrine](../../../architecture/wyrd-doctrine.mdx)
- [Security posture](../../../architecture/wyrd-security-posture.md)
- [Bifrost design](../../../architecture/bifrost-design.md)
- [Workflow contract](../../../crates/wyrd-spec/src/card/workflow.rs)
- [Skald Workflow owner](../../../crates/skald/skald-workflow/src/workflow.rs)
- [Shared client Workflow facade](../../../crates/shared/wyrd-client/src/workflow/mod.rs)
- [Server Workflow host](../../../crates/wyrd/wyrd-server/src/components/workflow/host.rs)
- [Server Workflow journeys](../../../crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs)
- [Rust SDK journey](../../../sdks/wyrd-sdk-rust/tests/workflow_loading.rs)
- [Python SDK journey](../../../sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py)
- [TypeScript SDK journey](../../../sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts)
- [Compiled CLI journey](../../../crates/wyrd/wyrd-cli/tests/workflow_journey.rs)
