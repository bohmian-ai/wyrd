---
id: SPEC-sdk-verification-control
revision: 2
status: approved
---

# Verification control through the SDKs

## Objective and user value

A developer following a check from binding to run to verdict should do it in
the language they already write, without dropping to raw HTTP. Today the server
exposes the verification control plane over HTTP and MCP only:

| Operation | HTTP | MCP | Rust | Python | TypeScript |
| --- | --- | --- | --- | --- | --- |
| Read a binding | `GET /v1/verification/bindings/{binding_id}` | `verification.get_binding` | absent | absent | absent |
| Start a manual Drift run | `POST /v1/verification/runs` | `verification.start_run` | absent | absent | absent |
| Read a run | `GET /v1/verification/runs/{run_id}` | `verification.get_run` | absent | absent | absent |
| Read a result | SQL on `vala.verification.results` | `bifrost.query` | `Bifrost::sql` | `Bifrost.sql` | `Bifrost.sql` |

So [Run a verification and inspect its evidence](../../../docs/src/content/docs/build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md)
switches from SDK tabs to JSON tool arguments halfway through. This change
projects the three existing control-plane operations into one SDK handle in
Rust, Python, and TypeScript, and rewrites that guide so every step shows the
three SDKs, with the MCP tool and SQL alongside.

This is a client projection of server contracts that already exist. No server
route, permission, table, or wire type changes.

## Requirements

### The handle

- **REQ-001** Each first-class SDK exposes a `Verification` handle that follows
  [SPEC-sdk-client-instantiation](../sdk-client-instantiation/spec.md): created
  with `connect` (Rust `connect()` / `connect_with(&client)`), optional client
  with ambient resolution, one `WyrdClient` held for its lifetime, and in Python
  a lazy constructor. It lives in Python `wyrd.verify` and Rust
  `wyrd_sdk::verify`, and is exported from the TypeScript package root (D1).
- **REQ-002** The handle has exactly three operations, each a projection of the
  existing server operation:

  | Operation | Calls | Returns |
  | --- | --- | --- |
  | `get_binding(binding_id)` | `GET /v1/verification/bindings/{binding_id}` | `VerificationBindingStatus` |
  | `start_run(target, input, idempotency_key=None)` | `POST /v1/verification/runs` | `StartVerificationRunResponse` (`run_id`) |
  | `get_run(run_id)` | `GET /v1/verification/runs/{run_id}` | `VerificationRunStatus` |

  Names follow each language's idiom (`getBinding`, `startRun`, `getRun` in
  TypeScript). Inputs and outputs are the `wyrd-spec` wire types, projected as
  typed values, not untyped maps, in every language.
- **REQ-003** `start_run` sends the optional idempotency key as the
  `Idempotency-Key` header. A retry with the same key and request returns the
  same `run_id`, as HTTP does today.
- **REQ-004** Every refusal surfaces as the server's existing stable error code
  in each language's normal Wyrd error type: invalid ID or window (`400`),
  missing permission (`403`), unknown binding or run in the caller's tenant
  (`404`), baseline not ready or idempotency conflict (`409`), registry or
  verifier unavailable (`503`). A `failed` verdict is never an error.
- **REQ-005** Identifiers are typed where the SDK already types them:
  `BindingId` and `VerificationRunId` in Rust; strings in Python and TypeScript
  validated by the server.

### Documentation

- **REQ-006** [Run a verification and inspect its evidence](../../../docs/src/content/docs/build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md)
  shows every step in Python, Rust, and TypeScript tabs, using the `support-desk`
  example, then names the MCP tool and its arguments for the same step.
- **REQ-007** The result step keeps SQL as the read path in all three SDKs and
  shows the same query for MCP `bifrost.query`. The guide states that binding
  readiness and run status come from the control plane, not SQL, because they
  live in the registry rather than Bifrost.
- **REQ-008** The MCP tools reference and the generated SDK references list the
  new handle and its operations.

## Invariants

- **INV-001** Authorization, audit, tenancy, idempotency, and error mapping stay
  in the server's `VerificationControl`. The SDKs add no client-side permission
  checks, retries that change semantics, or cached status.
- **INV-002** The SDKs share one Rust implementation in `wyrd-client`; Python
  and TypeScript wrap it and do not reimplement transport or request encoding.
- **INV-003** The wire contract is unchanged: same routes, bodies, headers,
  status codes, and error codes. No compatibility path is added.
- **INV-004** The handle never reads or writes Bifrost. Results stay a SQL read
  through `Bifrost`.

## Scope and non-goals

In scope: the `Verification` handle in Rust, Python, and TypeScript; its
journeys; the guide rewrite and reference updates.

Out of scope:

- **New server operations**, such as listing runs or listing bindings by Card.
  Binding IDs come from the owner Card's `status.verification.binding_ids`
  through `Cards`; run IDs come from `start_run` or a binding's `last_run_id`.
- **Direct execution** (`POST /v1/verification/execute`). Applications already
  reach it through `run.observe.verify`, which carries run attribution.
- **A polling helper** that waits for a run to finish. Callers read `get_run`
  in their own loop; the guide shows one.
- **Result-reading convenience methods** over `vala.verification.results`.

## Expensive-to-reverse decisions

- A new public handle, `Verification`, in `wyrd.verify` / `wyrd_sdk::verify`
  and the TypeScript root.
- Operation names that mirror the MCP tool names (`get_binding`, `start_run`,
  `get_run`), so an agent moving between MCP and SDK sees the same verbs.

## Acceptance criteria

- **AC-001** A user journey per language against `WyrdTestServer`: register the
  `support-desk` Service with a drift binding, read its binding ID through
  `Cards`, read the binding, start a run for a window, read the run until it is
  final, and read the result by `result_id` through `Bifrost.sql`.
- **AC-002** Negative journeys per language: an unknown binding ID returns the
  `404` code; a principal without `verifier:run` gets the `403` code from
  `start_run`; an inverted window gets the invalid-window code; a repeated
  idempotency key with the same request returns the same `run_id`, and with a
  different request returns the `409` code.
- **AC-003** A public-surface test per SDK finds `Verification` with
  `connect` and the three operations, and no other public operation.
- **AC-004** `mise run docs:check` and `mise run codegen:check` pass with the
  rewritten guide, regenerated Python stubs, and TypeScript declarations.

## Decided

- **D1. Handle name and module.** The handle is `Verification`, in Python
  `wyrd.verify` and Rust `wyrd_sdk::verify`, exported from the TypeScript
  package root. The module is named for the user's action; the handle and its
  verbs match the MCP `verification.*` tools.

## Open material decisions

None.

## Revision history

| Revision | Status | Change |
| --- | --- | --- |
| 1 | draft | Initial draft: one `Verification` handle projecting get binding, start run, and get run in three SDKs; guide rewrite with MCP and SQL. |
| 2 | approved | D1 decided: `Verification` in Python `wyrd.verify` and Rust `wyrd_sdk::verify`. |

## Authority

- [AGENTS.md](../../../AGENTS.md) §2 (server owns durable behavior; first-class
  SDKs over `wyrd-client`) and §11 (user-journey tests per surface).
- [SPEC-sdk-client-instantiation](../sdk-client-instantiation/spec.md)
  (instantiation, client ownership, Python import scoping).
- `crates/wyrd/wyrd-server/src/components/verification/routes.rs` and
  `crates/wyrd/wyrd-server/src/mcp/verification.rs` (the existing operations).
- `crates/wyrd-spec/src/verification.rs` (wire types).
