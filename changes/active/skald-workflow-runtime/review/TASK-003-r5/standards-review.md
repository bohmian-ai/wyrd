# Repository Standards Review — TASK-003 r5

## Review Findings

### Important

- **STD-TASK-003-R5-01 — VIOLATION: the governing architecture was not synchronized with the new authenticated fallback-header contract.**
  - **Violated authority:** `AGENTS.md` §§1–2 and §9 require public and internal contracts to align with `architecture/wyrd-design.md`; approved Revision 12 `INV-015` (`spec.md:2245-2255`) specifically requires the Wyrd gateway authority to record the authenticated fallback header before implementation completes.
  - **Location:** `crates/wyrd-spec/src/gateway/policy.rs:126-184` defines and encodes the public `wyrd-gateway-fallback` header; `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:189-230` consumes and publishes it in served OpenAPI. The active authorities `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and `architecture/wyrd-doctrine.mdx` contain no `wyrd-gateway-fallback`, `GatewayFallbackOverride`, or equivalent public-header contract.
  - **Consequence:** the implementation and generated public operation expose a security-relevant, authenticated request surface whose encoding, validation limits, authentication order, consumption boundary, and absent-header behavior are not recorded in the repository's governing architecture. Maintainers therefore cannot derive the implemented contract from the authority that is required to govern it.
  - **Required correction:** update the existing owning gateway architecture authority to record the implemented contract: an optional authenticated `wyrd-gateway-fallback` header, unpadded base64url over JCS UTF-8 `GatewayFallbackOverride`, the 8 KiB encoded/4 KiB decoded limits, auth-before-parse, consume-without-forwarding, invalid-request refusal, and unchanged tenant-policy behavior when absent. Do not add a new mechanism, checker, setting, or compatibility path. Verify by source inspection that the active authority and the served operation describe the same contract.

- **STD-TASK-003-R5-02 — VIOLATION: new async run-start operations have incomplete cancellation and partial-progress rustdoc.**
  - **Violated authority:** `AGENTS.md:716-730`, `architecture/agent-rules.md:35`, and `architecture/references/languages/rust-core.md:751-769` require complete rustdoc for every new or materially modified Rust item and specifically require async operations to document applicable cancellation and partial-progress behavior; incomplete rustdoc is a hard blocker.
  - **Location:** `crates/shared/wyrd-client/src/workflow/mod.rs:99-106` exposes `Workflow::run`, which can dispatch model calls but documents only its delegated errors. `crates/shared/wyrd-client/src/workflow/local.rs:72-104` (`SelectedRoutes::dependencies`) and `:107-138` (`resolve_binding`) asynchronously read selected secrets via blocking tasks but document errors only.
  - **Consequence:** callers of the public default-run entry point are not told that dropping the future does not roll back an already-sent model call, while maintainers of the run-start secret path are not told that cancellation can occur after earlier secret reads and that an already-started blocking read can finish after the future is dropped. Those are the exact side effects the repository requires async API documentation to make explicit.
  - **Required correction:** add operation-local rustdoc to `Workflow::run` describing the same dispatch/cancellation behavior as `run_with`, and document cancellation/partial progress for `dependencies` and `resolve_binding`, including already-completed reads and the non-cancellable lifetime of a started `spawn_blocking` read. This is documentation of the approved run-start design, not a request to move or replace the blocking boundary. Confirm by source inspection that each async operation records the behavior at its own API boundary.

### Critical

None.

### Suggestions

None. This review intentionally excludes optional improvements and new style preferences.

## Immutable Subject

| Item | Value |
|---|---|
| Repository | `/home/thorrester/Documents/GitHub/wyrd` |
| Base | `58d07d7260df1f022a721e720a28ea48e5096e35` |
| Candidate | `5084b0e5b30fe79fdb2468ad7d0fa3a9d2106069` |
| Candidate checked | `git rev-parse HEAD` returned the candidate SHA before this report was written |
| Task packet | Original TASK-003 plus R1, R2, R3, and R4 remediation packets |
| Human approvals applied | Revision 12's minimal `WyrdGatewayCall.model` amendment; R1 native-401 wording correction; run-start `spawn_blocking` for configuration, client, and selected-secret reads |
| Review mode | Strict source-only, read-only review; no build, compile, test, lane, Cargo, mise task, pnpm, or pytest execution |

The existing untracked files in `review/TASK-003-r5/` are other reviewers' outputs and were not modified by this reviewer.

## Authority Coverage

| Changed surface | Applicable authority read and applied | Coverage |
|---|---|---|
| Shared Rust client facade, retained client context, remote run handle, authentication, transport, configuration, and run-start setup | `AGENTS.md` §§2–6, 9–12, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,errors,implementation-execution,maintainer-style}.md` | Complete |
| Public gateway fallback wire contract, error normalization, served OpenAPI, and server ingress | `AGENTS.md` §§2, 4, 9–10, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/wyrd-security-posture.md`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{agent-harness,errors,rust-core,spec-driven-development}.md`; Revision 12 | Complete |
| Shared secret utility and gateway/server/client consumers | `AGENTS.md` §§3–6, 9–10, 16; `architecture/wyrd-security-posture.md`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,errors}.md` | Complete |
| Skald workflow planning, route resolution, per-call model/fallback context, cancellation and error behavior | `AGENTS.md` §§2–6, 10, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/doctrine/architecture-constraints.md`; `architecture/references/languages/{rust-core,agent-harness,errors}.md` | Complete |
| `wyrd-spec` gateway policy types and schema-facing public contract | `AGENTS.md` §§2–4, 9, 16; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/languages/{spec-driven-development,errors,rust-core}.md` | Complete |
| Python SDK wrapper and retained-client runtime path | `AGENTS.md` §§2–3, 7–8, 11–12, 16; `architecture/wyrd-design.md`; `architecture/references/languages/{pyo3-boundaries,python-api-and-stubs,testing-workflows}.md` | Complete |
| Rust SDK exports and TypeScript consumer closure | `AGENTS.md` §§2–3, 8–12; `architecture/wyrd-design.md`; `architecture/references/languages/{spec-driven-development,testing-workflows}.md` | Complete |
| Tests, recorded evidence, manifests, dependency direction, and generated/public documentation | `AGENTS.md` §§3–4, 11–12, 16; `architecture/agent-rules.md`; `architecture/references/README.md`; `architecture/references/languages/{testing-workflows,implementation-execution,spec-driven-development,maintainer-style}.md` | Complete |

`architecture/references/README.md` was used as the router. No Bifrost, UI, SQL, storage, or durable migration authority applies to this diff. The repository has no `.codegraph/` directory, so the repository instruction correctly routes code discovery to ordinary source and diff inspection.

## Applicable Rule Results

| Rule | Exact implementation/source evidence | Result |
|---|---|---|
| Active architecture must govern public and internal API surfaces | The candidate adds `FALLBACK_HEADER` and encoding in `wyrd-spec/src/gateway/policy.rs:126-184` and publishes it in `wyrd-server/.../gateway/ingress.rs:189-230`, but the active architecture authorities do not record the contract despite Revision 12 `INV-015`. | **FAIL** — STD-TASK-003-R5-01 |
| Public wire contracts are typed, versioned, schema-visible, and use stable Wyrd errors | `GatewayFallbackOverride`/`ModelRef` are typed `wyrd-spec` values; `requested_fallback` maps contract rejection to the catalogued `GatewayInvalidRequest`; all four affected ingress operations declare the header in `utoipa` metadata. | PASS |
| `wyrd-spec` stays foundational, synchronous, IO-free, and PyO3-free | `crates/wyrd-spec/src/gateway/policy.rs` contains pure validation/serialization only; its manifest additions use existing workspace serialization dependencies and add no runtime, IO, client, server, or PyO3 dependency. | PASS |
| Server owns authorization, policy, audit, dispatch, and public ingress behavior | Public client code calls existing native gateway routes; `wyrd-server` handlers authenticate before `requested_fallback`, build `GatewayInvocationRequest`, and preserve the existing policy/audit/dispatch owner. No durable decision moved into an SDK. | PASS |
| Client tier remains server-free and uses the shared `wyrd-client` owner | Workflow handles/callers live under `crates/shared/wyrd-client`; Rust and Python project that owner. The changed client manifests introduce no `sqlx`, cloud SDK, DataFusion, Delta Lake, or server crate edge. | PASS |
| Struct-centered Rust owns stateful workflows and dependencies | `Workflows` and `PublicWyrdGatewayCaller` own one `WyrdClient`; `Workflow` owns hydrated Skald state plus the retained loading client; route-dependent assembly is kept on the `Workflow`/`SelectedRoutes` owners. Helpers are narrow conversions or synchronous halves of explicit blocking boundaries. | PASS |
| Async is limited to IO/composition; blocking filesystem work has an explicit strategy | `Workflow::from_path` and `Workflow::run_with` put bundle/config/client setup on `tokio::task::spawn_blocking`; `resolve_binding` does the same for secret reads. The human-approved run-start timing is preserved, and pure projection/validation remains synchronous. | PASS |
| New/materially modified Rust items have complete operational rustdoc | Most changed operations record errors, cancellation, retry, and partial progress, including `from_path`, `run_with`, remote `create`/`cancel`/`wait`, gateway `call`, and `post_native`. `Workflow::run`, `SelectedRoutes::dependencies`, and `resolve_binding` omit applicable cancellation/partial-progress behavior. | **FAIL** — STD-TASK-003-R5-02 |
| Native gateway model calls are send-once and 401 renewal is safe | `transport/http.rs:376-410` sends once, observes status, calls `force_refresh` on 401 before reading the body, returns the original response, and documents no resend. `workflow/gateway.rs:52-124` adds the caller deadline/cancellation bound without an internal replay loop. | PASS |
| Authentication precedes fallback parsing; request-only context is consumed, bounded, and not forwarded | Auth middleware remains outside the handlers; `ingress.rs:207-223` accepts at most one header and validates it only after handler entry; the resulting override enters the internal invocation request and the header is not copied to upstream provider headers. | PASS |
| Secrets use `SecretString`, remain redacted, and file reads enforce the shared boundary | `wyrd-utils/src/secret.rs` is the single shared `SecretRef` reader, bounds file size, validates restrictive file metadata, returns `SecretString`, and exposes only fixed error text. Gateway, server, and client callers reuse it; client async use is offloaded to the blocking pool. | PASS |
| Per-call workflow context must not leak across concurrent calls | `WyrdGatewayCall` owns model, fallback, deadline, cancellation, and correlation; `PublicWyrdGatewayCaller` is stateless apart from the shared client and builds request-local headers/span fields. No fallback or correlation is stored on the adapter. | PASS |
| Run-start configuration must use one snapshot and retain loading-client context | `Workflow::run_with` invokes one blocking `load_local_setup`; `load_local_setup` performs at most one `GlobalConfig::load`; `local_setup_from` derives both `global.workflow` and `ClientConfig::from_global_with_env(&global)`. A retained client takes precedence. `mixed_routes_use_one_config_snapshot` and the recorded Python retained-client integration directly cover these paths. | PASS |
| Python/PyO3 logic stays at the SDK boundary and delegates to Rust-native owners | The Python wrapper stores `wyrd_client::Workflow`, calls the shared async Rust run path through the existing runtime bridge, and does not duplicate workflow execution, transport, config, secret, or gateway logic. No PyO3 enters `wyrd-spec`; no Python feature is enabled in Rust/TypeScript SDKs. | PASS |
| Public Python typing/export layers remain synchronized | The user-facing Python signature/export is unchanged while its private native holder changes to the shared client facade; therefore no generated `.pyi` contract change is required. Recorded `py:typecheck` and `codegen:check` evidence is consistent with the source diff. | PASS |
| First-class Rust/Python/TypeScript consumer closure is preserved | Rust re-exports shared workflow handles; Python delegates to the shared facade; existing TypeScript local execution continues through the shared Skald path. The task intentionally does not add Python/TypeScript server-run lifecycle APIs, and the cumulative evidence records Rust/Python/TypeScript checks. | PASS |
| Public errors do not retain provider or secret-bearing body text | `workflow/gateway.rs` recognizes catalog codes and substitutes catalog metadata; uncoded responses receive fixed provider-status messages. Secret/config failures use fixed public reasons. Raw provider text and secret values are not retained in returned errors. | PASS |
| Tests follow runtime ownership and user-facing coverage rules | Rust-only transport/workflow behavior is covered in Rust; retained Python client behavior is covered by a Python integration test; server/OpenAPI behavior is covered through server tests. Python tests are top-level functions. No test initializes a foreign runtime from Rust. | PASS |
| Verification uses scoped repository commands and does not weaken a gate | Original TASK-003 records scoped shared, SDK, gateway, OpenAPI, boundary, codegen, Python, TypeScript, formatting, and lint evidence. R1–R4 record focused reruns for their changed paths. Source inspection found no added `allow`, ignored test, skip, relaxed assertion, widened allowlist, or replacement checker used to clear a failure. | PASS |
| Dependency and abstraction additions are necessary and narrow | Added dependencies are existing workspace dependencies used for the specified JCS/base64 contract, async cancellation, or shared secret handling. No compatibility layer, cache, watcher, config generation token, new setting/option, duplicate runtime, or speculative generic platform was added. | PASS |
| DRIFT calibration | Every new mechanism in the reviewed diff is required by the approved contract or reuses established Rust/Wyrd mechanisms. No mechanism/check/file/setting/option was found that lacks both repository precedent/authority and comparable standard-project precedent. | PASS |

## Open Questions

None. The two failures are directly established by source and governing authority, and neither depends on an unresolved product choice.

## Verification Notes

- Per the human standing direction, this reviewer did not build, compile, execute tests, invoke any repository lane, or run Cargo, mise tasks, pnpm, pytest, or another verification command. Review inputs were the immutable diff, current source, task/remediation packets, and their recorded evidence.
- The original task records passing focused Rust workflow/transport tests plus scoped shared-client, Rust SDK, native-gateway, served-OpenAPI, gateway PostgreSQL, codegen, client-tier, SDK-tier, PyO3-scope, Python, TypeScript, formatting, lint, and diff checks. R1–R4 record the focused proof for each remediation, including the final one-snapshot Rust test and public Python retained-client integration.
- The recorded evidence is source-consistent for the implemented behavior. It cannot cure the missing architecture synchronization or incomplete required rustdoc, both of which are visible source failures.

## Overall Result

**FAIL.** Required reviewers were available and the authority coverage is complete, so the review is not blocked. The candidate violates one explicit approved architecture-synchronization invariant and the repository's hard Rust documentation rule. Both corrections are bounded documentation changes to existing authorities/source; neither requires a new mechanism or changes the human-approved run-start blocking design.
