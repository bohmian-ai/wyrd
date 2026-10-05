# Security Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Specification: `changes/active/skald-workflow-runtime/spec.md`, approved Revision 14
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded-evidence review only; no build or test was run

The candidate remained at the supplied commit while this report was prepared.
Revision 14 governs provider request/destination behavior. The approved
Rust/TypeScript Cards registration split, installed Python `wyrd apply` proof,
TypeScript Skald packaging, development-only wiremock allowlist additions, and
standing TASK-004 dispositions were treated as fixed decisions.

## Reviewed boundary

This pass traced the security-sensitive TASK-005 paths end to end:

1. CLI selectors, JSON input, execution mode, detach, and run-id validation,
   including refusal before client construction or provider dispatch.
2. Ambient client authentication and local `SecretRef` resolution, including
   the distinction between local `ext_gateway`, public `wyrd_gateway`, and
   native-provider credentials.
3. Public Workflow create/get/cancel authentication, typed authorization,
   canonical audit, run ownership, and indistinguishable foreign/unknown lookup.
4. External gateway protocol/origin/header constraints, private-origin local
   policy, production SSRF screening, DNS pinning, redirect refusal, and secret
   redaction.
5. Public and in-process Wyrd gateway authorization and provider-credential
   ownership.
6. Server built-in tool authority for `bifrost.query` and `cards.get`, including
   closed model-supplied arguments, caller capture, resource-specific
   authorization/audit, and redacted errors.
7. Rust, Python, TypeScript, and compiled-CLI journeys for lazy credentials,
   denied reads, route isolation, apply-without-execution, and secret handling.
8. Manifest, lockfile, CI-selection, and mock-scope changes for dependency and
   supply-chain impact.

## Authority and source coverage

| Boundary | Governing authority | Source and caller evidence | Result |
|---|---|---|---|
| CLI input and selector admission | `AGENTS.md` §§4, 9, 11; task Scenario 1; spec REQ-026–027 and AC-013 | `crates/wyrd/wyrd-cli/src/workflow.rs:64-200,285-339,342-386`; `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:193-386` | **PASS.** Clap makes source and input choices exclusive. Typed identity parsing and JSON-object decoding occur before a client or Workflow loader is constructed. Invalid JSON is not echoed. File/server and local/detach combinations refuse before registry or provider IO. No shell interpolation or unsafe deserialization was added. |
| Local client and credential selection | Security posture principles; spec REQ-025, REQ-034, REQ-042, REQ-058, INV-003/012, AC-023/030 | `crates/shared/wyrd-client/src/workflow/mod.rs:35-236`; `crates/shared/wyrd-client/src/workflow/local.rs:30-159`; `crates/shared/wyrd-client/src/global_config.rs:12-105`; CLI delegation at `workflow.rs:202-283` | **PASS.** Fully local loading constructs no client. External Card references lazily use the established client credential path. Run preparation resolves only selected external bindings. Secret values stay in `SecretString`, errors name only fixed reasons/binding identity, and the CLI adds no credential parser or secret loop. |
| Secret references and outbound headers | `architecture/wyrd-security-posture.md:310-352`; agent-rule SSRF requirements; spec REQ-034/042/049, INV-010/012/017 | `crates/shared/wyrd-utils/src/secret.rs:1-76`; `crates/skald/skald-workflow/src/route.rs:97-200,266-355,492-542`; `crates/wyrd-spec/src/card/workflow.rs:540-632`; recorded CLI/SDK route journeys | **PASS.** Mounted secrets are read through an already-open, regular, owner-only, bounded file; environment/file failures expose no value or coordinate. Binding insertion rejects reserved transport headers and invalid origins. Authored headers cannot collide with secret headers, and secret header values are marked sensitive. |
| SSRF and private-origin policy | `architecture/agent-rules.md` SSRF rules; `architecture/wyrd-security-posture.md:321-352`; spec REQ-049 and AC-016/023 | `crates/skald/skald-providers/src/endpoint.rs:1-216`; `crates/skald/skald-workflow/src/route.rs:325-350,492-500`; server profile selection at `crates/wyrd/wyrd-server/src/components/workflow/host.rs:334-358` | **PASS.** Local execution permits an explicitly configured private HTTPS origin and loopback HTTP, as approved. Server production execution requires public HTTPS on port 443. Literal and resolved addresses use the shared endpoint policy; every DNS answer is screened and used by the same resolver, proxies and redirects are disabled, and metadata/link-local addresses are always blocked. No second endpoint-security mechanism was added. |
| Public Wyrd gateway authentication | Security posture client-to-gateway boundary; spec REQ-034/038/039/041 and INV-012/020 | `crates/shared/wyrd-client/src/workflow/gateway.rs:32-125,127-330`; `crates/shared/wyrd-client/src/workflow/mod.rs:195-236`; route matrix evidence in `workflow_journey.rs:1172-1418` | **PASS.** Calls reuse one authenticated `WyrdClient`; no provider credential or endpoint enters the Workflow Card. Request dialect is projected before IO, Vertex is refused at the unsupported public edge, fallback is a bounded authenticated header owned by the existing ingress, cancellation/timeouts are per call, and upstream refusal bodies are reduced to catalog-safe fields. |
| Server create/get/cancel, ownership, and IDOR resistance | Security posture identity/authorization/audit rules; spec REQ-029/032/032A/033, INV-006/018/022, AC-009/010/018/027 | `crates/wyrd/wyrd-server/src/components/workflow/routes.rs:1-179`; `host.rs:60-203`; `runs.rs:369-415`; existing real-server evidence at `crates/wyrd/wyrd-server/tests/pg_workflow_runs.rs:1441-1644,2570-2740` | **PASS.** Authentication precedes path/body extraction. Every operation authorizes and audits `Permission::workflow_run()`. Run keys and lookups are tenant- and principal-qualified, and malformed, unknown, other-principal, and foreign-tenant IDs share the same not-found result. Accepted work captures caller authority rather than retaining bearer material; later requests authenticate afresh. |
| Server external bindings | Security posture tenant and credential isolation; spec REQ-034/038/042, INV-006/010/012 | `host.rs:290-380`; `local.rs:113-159`; shared route and endpoint owners above | **PASS.** Graph pinning uses a tenant connection. Only bindings whose configured tenant equals the verified caller tenant are resolved, and resolution precedes acceptance/provider dispatch. Missing or mismatched bindings fail closed. External calls do not receive public Wyrd or gateway provider credentials. |
| Agent tool authority | `AGENTS.md` audit rules; spec REQ-052, INV-008/022, AC-025/026 | `crates/wyrd/wyrd-server/src/components/workflow/tools.rs:35-351`; Cards owner at `components/cards/routes.rs:156-188`; bounded query owner at `query/collect.rs:173-324` | **PASS.** Only the Agent's declared built-in names resolve. Model arguments cannot select tenant, principal, credential, or endpoint. `cards.get` delegates to the canonical audited Card read, and `bifrost.query` delegates to the established SELECT-only/resource-authorized query service with closed arguments and result/deadline ceilings. Model-visible failures discard underlying messages and details. |
| SDK/apply journeys | Spec REQ-054–059 and AC-029–031; approved Scenario 5 deviation | `sdks/wyrd-sdk-rust/tests/workflow_loading.rs`; `sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py`; `sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts`; `crates/wyrd/wyrd-cli/tests/workflow_journey.rs` | **PASS.** The journeys cover absent and denied credentials, exact locked references, no dispatch during registration, public-gateway auth, external-binding secret delivery without `authorization`, and no secret in CLI run output. The fixtures use deterministic fake credentials and owner-only temporary secret files. |
| Dependency and CI surface | `AGENTS.md` dependency and gate rules; fixed human decisions for TypeScript packaging and dev-only wiremock allowlists | `crates/wyrd/wyrd-cli/Cargo.toml`; `sdks/wyrd-sdk-rust/Cargo.toml`; `Cargo.lock`; `.github/scripts/tests/test-detect-changes.sh`; `scripts/checks/mocks-scope.sh` | **PASS.** The CLI enables Tokio's existing `signal` feature for Ctrl-C. Rust SDK adds the already-workspace-pinned `wiremock` only as a dev dependency. No new external package, production mock dependency, credential helper, crypto, auth bypass, or bespoke security option entered the candidate. The changed TypeScript packaging expectation follows the now-real dependency cone. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No optional hardening is required for TASK-005 acceptance.

### Positive Controls

- CLI validation uses typed IDs and a JSON-object boundary and does not echo
  invalid inline input.
- Authenticated server clients, rather than Workflow fields, supply Wyrd
  credentials; gateway provider credentials remain server-owned.
- Local external-gateway secrets use indirection, owner-only bounded file
  reads, `SecretString`, sensitive header values, and fixed redacted errors.
- External egress reuses the established screened resolver/transport; it does
  not add a task-specific URL allowlist, proxy behavior, redirect handler, or
  alternate client.
- Server run lookup is qualified by both tenant and principal, with a uniform
  not-found projection for foreign and absent IDs.
- Built-in tools reuse existing authorization/audit owners instead of trusting
  an authenticated Workflow submission as blanket data access.
- Public gateway response normalization refuses to expose raw upstream bodies,
  prompts, credentials, or arbitrary details in Workflow results.
- Registration journeys count upstream calls, proving `apply` performs no
  model or tool execution.

## Verification assessment and limits

The task records a successful `mise run gate` at `b88102317`, successful exact
focused CLI journeys after that aggregate, successful Rust/Python/TypeScript
SDK journeys, and a clean `git diff --check`; the candidate's later commit only
records that evidence. The gate includes the Rust family, identity, Python and
TypeScript integration, gateway, codegen, boundary, tenant-isolation, mock-scope,
and dependency-selection checks (`mise.toml:1514-1545`). Existing real-server
Workflow tests exercise actual other-principal and foreign-tenant run IDs, so
the compiled-CLI journey's additional unknown-ID probe is not being used as a
substitute for ownership proof.

Per the review constraint, none of those commands was rerun. No live cloud or
provider credential lane was required: the accepted behavior is proven against
local deterministic upstreams, and the review found no production path whose
security depends on a test-only credential or mock implementation.

## Material findings

None.

## Overall result

**PASS** — the TASK-005 candidate preserves authentication, authorization,
tenant/principal ownership, credential separation, secret redaction, egress
screening, and tool authority across the changed CLI and integrated proof
surfaces. No exploitable security defect, unapproved trust-boundary change, or
unsupported bespoke security mechanism was found.
