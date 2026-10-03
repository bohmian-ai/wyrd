# Repository standards review — TASK-003 r4

## Review Findings

### Critical

None.

### Important

#### STD-R4-001 — the latest shared run-path change lacks its required language-boundary proof

- Classification: **VIOLATION / MISSING EVIDENCE**
- Governing authority: `AGENTS.md` §§11–12 (required targeted proof and Python-visible coverage); `architecture/agent-rules.md` (every user-facing capability requires its owning journey); `architecture/references/languages/testing-workflows.md` (a lower-tier Rust test does not substitute for a public language journey); `architecture/references/languages/python-api-and-stubs.md` (Python-visible behavior is exercised through the public Python runtime); and the approved R3 remediation acceptance criterion.
- Changed and affected locations:
  - `crates/shared/wyrd-client/src/workflow/mod.rs:107-149`
  - `sdks/wyrd-sdk-python/src/workflow.rs:537-562`
  - `sdks/wyrd-sdk-python/tests/integration/gateway/test_workflow_gateway_context.py:123-180`
  - `changes/active/skald-workflow-runtime/review/TASK-003-r3/TASK-003-R3-close-pending-renewal-and-async-context-contracts.md:120-132,148-162`
- Issue: R3 materially changed `Workflow::run_with`, the shared path invoked by Python `Workflow.run`, by moving ambient configuration and gateway-client construction into `spawn_blocking`. The R3 acceptance criterion explicitly requires both the selected-local-dependencies proof **and retained-client/language coverage** to be rerun and recorded. Its evidence records the Rust selected-dependencies/from-path/transport/auth tests, client-only Clippy and formatting, and `git diff --check`, but no Python retained-context journey or other language-boundary command. The earlier R1/R2 Python journey predates this changed shared run path and therefore is not proof of the candidate now under review.
- Impact: the recorded evidence does not establish that registered and authored-external-ref Python Workflows still retain their loading client, avoid ambient redirection, and execute through the public Python boundary after the R3 async-context correction. Source inspection is consistent with the intended delegation, but repository completion rules and the remediation itself require runtime proof; this review may not infer it from Rust-only tests.
- Testable correction: rerun and record the existing public Python integration selector `test_loaded_workflow_calls_the_gateway_through_its_loading_client` against the candidate, using its repository-managed Postgres/Python setup from the prior evidence. If it fails, fix the shared owner rather than adding a Python-specific transport or context copy. Add no new test, harness, check, option, setting, or compatibility path.

### Suggestions

None.

## Open Questions

None.

## Immutable subject and review boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved specification: `SPEC-skald-workflow-runtime`, Revision 12
- Original task: `TASK-003-remote-client-and-public-gateway.md`
- Remediations: TASK-003 R1, R2, and R3, including their implementation evidence and prior review records
- Human approvals applied as authority:
  - 2026-10-03 minimal Revision 12 amendment adding `pub model: ModelRef` to `WyrdGatewayCall`;
  - the R1 same-spec correction requiring native `401` renewal without replay;
  - approval of `spawn_blocking` for run-start configuration, client, and secret reads, while execution-secret resolution remains at run start.
- Review mode: source, cumulative diff, and recorded evidence only. No build, compile, test, Cargo, mise, pnpm, pytest, formatter, linter, code-generation, test-listing, or other verification command was run.
- `.codegraph/` is absent, so repository navigation used Git diff/source inspection as directed.

## Authority coverage

| Changed surface | Applicable authority inspected | Result |
|---|---|---|
| Shared Workflow facade, remote lifecycle handle, local dependency composition, public gateway caller, authentication owner, and HTTP transport | `AGENTS.md` §§2–6, 9, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Client model, Workflow, runtime identity, and public surfaces; `architecture/wyrd-doctrine.mdx`; architecture constraints/patterns; Rust core, errors, testing, implementation-execution, and spec-driven references | Complete; source conforms, but latest language-boundary proof is missing (`STD-R4-001`) |
| Skald Workflow planning, route ownership, and approved `WyrdGatewayCall.model` field | Wyrd design Workflow contract; Skald ownership rules; Rust struct-centered/async/import guidance; approved human amendment | PASS |
| Pure fallback-header contract in `wyrd-spec` | Foundation boundaries; doctrine; agent-harness and error references; server-contract rules | PASS |
| Authenticated OpenAI Chat/Responses, Anthropic, and Gemini ingress plus served OpenAPI | Server/contract rules; security posture; agent-harness; error and testing references | PASS from source and prior recorded evidence |
| Shared environment/file secret reader and gateway/server/client consumers | Security posture secret handling; Rust secret/visibility/dependency-cost rules; approved run-start blocking decision | PASS |
| Python shared-Workflow projection and retained-client behavior | PyO3 boundaries; Python API/stub guide; shared-client ownership; testing taxonomy | FAIL only for the missing post-R3 runtime proof in `STD-R4-001` |
| Rust SDK projection and first-class client boundary | Client-tier and SDK ownership rules; manifest/feature constraints | PASS |
| Manifests, lockfile, generated/export parity, and test placement | Dependency-cost and feature rules; client-tier rules; generated-artifact rules; test taxonomy | PASS from source; no generated artifact was hand-edited and no unsupported feature/check/harness was added |

## Applicable rule results

| Repository rule | Source-backed assessment | Result |
|---|---|---|
| `wyrd-client` is the sole SDK-facing implementation; language SDKs project it without duplicate transport, lifecycle, or durable semantics. | `Workflow`, `Workflows`, and `PublicWyrdGatewayCaller` own the behavior; Python stores `ClientWorkflow`; the Rust SDK re-exports the shared owners. | PASS |
| Stateful and dependency-backed workflows use cohesive concrete owners and inherent methods. | `Workflow`, `Workflows`, `PublicWyrdGatewayCaller`, and `SelectedRoutes` own their respective dependencies and orchestration; no duplicate graph, parser, executor, cache, or transport was introduced. | PASS |
| Skald owns reusable runtime routing and Wyrd server/client layers do not reverse that dependency. | Route planning and per-attempt immutable adapters remain in `skald-workflow`; the client supplies one approved implementation and Skald has no reverse client/server edge. | PASS |
| `wyrd-spec` remains IO-free, async-free, Tokio-free, and PyO3-free. | Fallback header encoding/decoding and the model field are pure typed contract work. | PASS |
| Async worker threads do not perform blocking filesystem work; the approved blocking boundary remains at run start. | Authored loading, ambient client/config assembly, and selected secret reads use Tokio's blocking pool. The human-approved placement is treated as authoritative, not drift. | PASS |
| Public model calls remain send-once; observed native `401` renewal precedes body collection and does not replay the call. | `HttpTransport::post_native` checks status, refreshes through `AuthMiddleware`, then reads the body; `force_refresh` rustdoc distinguishes replay-safe callers from native send-once behavior. | PASS from source and recorded focused evidence |
| Public error projection is catalog/category backed and excludes raw provider messages and arbitrary details. | Recognized codes use the derive-backed catalog title/remediation; uncoded statuses use existing provider categories; only the permitted OpenAI field survives. | PASS |
| Authentication, tenant identity, gateway authorization, and invocation audit remain server-owned. | Fallback decoding runs only after the authenticated caller branch and feeds the existing `GatewayInvocation`; no header supplies tenant or principal identity. | PASS |
| The fallback header is bounded, typed, consumed at ingress, and never forwarded to providers. | `GatewayFallbackOverride::{to,from}_header_value` owns the contract; ingress converts only to `GatewayCallRequest.fallback`; affected OpenAPI operations document it. | PASS |
| Secrets use redacted values, are selected lazily, and are absent from Cards, results, errors, and generated artifacts. | The shared reader returns `SecretString`; only selected external bindings resolve; the public plaintext helper was removed; server key decoding borrows exposed material only at its owning boundary. | PASS |
| PyO3 behavior stays in the Python SDK, uses `Bound`/the shared runtime bridge correctly, and does not duplicate shared-client behavior. | `PyWorkflow` retains `ClientWorkflow`, mutates the inner Skald value through explicit accessors, and detaches the GIL while the shared runtime executes. | PASS from source; runtime evidence gap remains `STD-R4-001` |
| Changed declarations use module-top imports and bare or role-specific aliased names. | The R2 remediation uses `LoadedTree`, `Request`, `Prompt`, `Response`, `SkaldWorkflow`, and `ClientWorkflow` through owning import blocks. | PASS |
| New/materially changed Rust items have substantive rustdoc for intent, errors, panic invariants, cancellation, partial progress, and retry where relevant. | R2/R3 documentation covers panic-capable helpers/tests, create/cancel/native-call partial progress, `force_refresh` retry distinctions, blocking setup, and `into_skald` context loss. | PASS |
| User-facing behavior is proved in its owning runtime; lower-tier tests do not replace a required language journey. | The latest shared `run_with` change has Rust proof but the required retained-client Python journey was not rerun or recorded. | **FAIL (`STD-R4-001`)** |
| No generated artifact was hand-edited; OpenAPI is proved through the assembled server rather than a snapshot. | No generated file changed. Prior evidence records codegen and served OpenAPI checks; the latest remediation did not alter contracts or routes. | PASS |
| No gate weakening, ignored test, lint allowance, compatibility alias, ad-hoc runtime, public arbitrary-header API, or unsupported mechanism/check/setting/option was added. | Cumulative diff contains none. The fallback header, shared config field, loopback fixture, and blocking boundary are approved or established mechanisms. | PASS |

## Verification Notes

- Reviewed the recorded original/R1/R2 evidence for focused Workflow transport, fallback codec, gateway/server/Postgres ingress, served OpenAPI, Python retained-context, shared/Rust-SDK/TypeScript, codegen, client/SDK/PyO3 boundaries, formatting, lints, and whitespace.
- Reviewed the R3 evidence for the pending-body `401` case, selected local dependencies, authored loading, remote lifecycle, and auth refresh tests, plus client-only Clippy/format and whitespace checks.
- No command was rerun in this review, per the human's strict read-only direction.
- The R3 evidence is incomplete against its own acceptance criterion: it does not record the retained-client/language journey after changing the shared execution path. That omission is the sole material standards finding.

## Overall result

**FAIL**

The candidate otherwise conforms to the applicable repository ownership, architecture, security, contract, secret-handling, Rust/PyO3, import, documentation, dependency, generated-artifact, and no-extra-mechanism standards. Repository-standards acceptance requires the already-existing Python retained-client journey to be rerun and recorded against this candidate; no implementation change is required unless that proof fails.
