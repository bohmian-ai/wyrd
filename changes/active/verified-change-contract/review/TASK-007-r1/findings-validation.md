# TASK-007 Wave 2 Findings Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `2bd4ded8f212f7885b0dd78bcd450fa0ff118f3e`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Prior stable findings: none

The candidate remained the repository `HEAD` while this report was written. The
untracked review directory does not alter the reviewed commit.

## Independent validation and caller coverage

CodeGraph reported that this checkout has no `.codegraph/` index, so source and
caller tracing used the immutable candidate directly. The following complete
owner/caller paths were inspected before validating corrections:

| Boundary | Complete reachable path inspected | Result |
|---|---|---|
| Key failures and diagnostics | `OperatorKeys::key` -> `seal` -> `OperatorConnectionControl::{create,update}` -> HTTP/MCP/shared-client surfaces; `key` -> `open` -> `OperatorWorker::credential`; `key` -> `rewrap_tenant` -> `rewrap_pass` -> `OperatorWorker::run` | Public conversion and the rewrap warning both format selector-bearing `KeyError` values |
| Production key readiness | configuration validation -> boot state construction -> `OperatorKeys::new`; later reads from create/update, delivery, and rewrap | No boot path reads or validates an active tenant KEK |
| Authenticated HTTP delivery | `OperatorWorker::attempt` -> `credential` -> `OperatorDelivery::send` -> `HttpRequest::render` -> `OperatorDelivery::http` -> `client` -> reqwest send; redirect loop reuses the request headers | Resolve/screen/pin precedes the network send, but credential headers are materialized before screening |
| Rotation work | `OperatorWorker::run` -> `OperatorKeys::rewrap_pass` -> `rewrap_tenant` -> `key` per row | The sole claim loop awaits the whole pass; a 100-row batch can perform one active-key read plus 100 sequential old-key reads |
| File-backed secrets | `OperatorKeys::key` and `vault_key`, reached by connection writes, delivery, rewrap, and proposed boot readiness | Both file paths perform blocking `std::fs` work in async execution; only the KEK path checks owner-only mode |
| SDK projections | Rust closed request/view contracts -> shared JSON client -> PyO3/N-API wrappers -> Python stubs and TypeScript ergonomic declarations | Runtime views flatten provider fields; TypeScript declares nonexistent `config`, and Python/TypeScript update types erase closed variants |
| Function-scoped imports | all changed non-test modules and the `wyrd-testing` library helper | Three ordinary functions violate the top-of-module import rule |
| Workflow-skill edits | complete four-file skill diff and TASK-007 scope/consumer list | No Operator requirement or consumer uses the edits |

The apparent Wave 1 disagreement about SSRF is resolved by authority, not by
compromise: the candidate satisfies the repository's general resolve/reject/pin
rule before a fetch, which is why `standards-review.md` passed that baseline.
REQ-149 and TASK-007 Scenario 7 additionally require screening before
credential attachment. The source violates that stronger task-specific order,
so the task/domain finding is retained with a narrower correction. It is not a
claim that the candidate currently transmits a credential to a blocked address.

## Wave 1 finding decisions

| Wave 1 source ID | Decision | Disposition |
|---|---|---|
| `TREV-001`, `SEC-003` | **CONFIRMED** | Deduplicated into `FIND-TASK-007-1`; caller tracing also found the same selector leak in the rewrap warning |
| `TREV-002`, `SEC-001`, `DR-DEL-1` | **CONFIRMED** | Deduplicated into `FIND-TASK-007-2` |
| `TREV-003`, `SEC-004`, `DR-DEL-2` | **REVISED** | Retained as `FIND-TASK-007-3`; decryption-before-screening is not itself prohibited after authority matching, but attachment-before-screening is explicitly prohibited |
| `TREV-004` | **CONFIRMED** | Retained as `FIND-TASK-007-4` |
| `TREV-005`, `SEC-002`, `STD-007-01` | **REVISED** | Deduplicated into `FIND-TASK-007-5`, retaining both plaintext Vault transport and the independently proven loose token-file permission path |
| `STD-007-02` | **CONFIRMED** | Retained as `FIND-TASK-007-6` |
| `STD-007-03` | **CONFIRMED** | Retained as `FIND-TASK-007-7` |
| `STD-007-04` | **CONFIRMED** | Retained as `FIND-TASK-007-8` |
| `DR-DEL-3` | **REVISED** | Retained as `FIND-TASK-007-9`; row count is bounded, but elapsed time and interference with the sole claim loop are not bounded within delivery availability |

No Wave 1 finding was rejected. No retained correction requires a new public
API, product behavior, security policy, concurrency semantic, or persistent
data decision; `SPEC_REVISION_REQUIRED` is not indicated.

## Validated finding ledger

### FIND-TASK-007-1 — CONFIRMED — VIOLATION: key failures disclose secret-provider selectors

- **Wave 1 sources:** `TREV-001`, `SEC-003`
- **Violated obligation:** TASK-007 Scenario 4 and its acceptance criteria prohibit secret selectors in responses, errors, logs, traces, audit, CLI diagnostics, and parsed-argument debug output.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/operators/keys.rs:40-47,57-63,150-173,187-229,374-391`; reachable public proof at `crates/wyrd/wyrd-server/tests/pg_operator_connection_routes.rs:497-535`.
- **Evidence:** `KeyError::Unavailable` stores the environment name, KEK file path, or Vault mount/prefix/tenant/version plus the raw cause. `From<KeyError> for WyrdError` logs and returns `error.to_string()`. `rewrap_pass` separately converts the same error to a string and logs it. The route test requires selector detail in the client-visible problem.
- **Observable consequence:** a connection writer can receive deployment secret-location metadata, and background rewrap can emit the same selectors and provider/filesystem details into logs.
- **Decision-complete correction:** keep detailed source errors internal to `OperatorKeys`, but make every outward `WyrdError` detail a constant safe key-unavailable message and replace selector-bearing logs in both public conversion and rewrap with bounded fields such as source kind, key version, and a stable failure class. Do not log env names, file paths, Vault URLs/paths, tenant-bearing selectors, or raw I/O/provider text. Preserve the stable public code and retry classification.
- **Focused closure proof:** route failures for env-, file-, and Vault-shaped selectors and a rewrap key failure using sentinel values; assert the response and captured logs contain neither sentinel selectors nor secret bytes while retaining `WYRD_OPERATOR_503_KEY_UNAVAILABLE` and retry behavior.

### FIND-TASK-007-2 — CONFIRMED — INCORRECT: multi-tenant production becomes ready without a readable active tenant KEK

- **Wave 1 sources:** `TREV-002`, `SEC-001`, `DR-DEL-1`
- **Violated obligation:** REQ-147 and TASK-007 Scenario 3 require multi-tenant production to fail startup when the external provider or active 32-byte tenant key is unavailable.
- **Exact location:** `crates/wyrd/wyrd-server/src/config.rs:1830-1835,1872-1917`; `crates/wyrd/wyrd-server/src/boot/mod.rs:1445-1456`; contradictory candidate authority at `architecture/wyrd-design.md:1383-1392`.
- **Evidence:** validation checks only the source tag, URL syntax, and token presence. Boot constructs `OperatorKeys` synchronously and never reads a key. Unreachable Vault, missing path, invalid base64, and wrong-size active keys first fail during a later write, delivery, or rewrap. The candidate's design text changes the approved fail-start behavior to write-only refusal.
- **Observable consequence:** a production replica advertises readiness while its Operator control plane and deliveries cannot use required tenant credentials.
- **Decision-complete correction:** restore the approved fail-start statement in `wyrd-design.md`. During multi-tenant production boot, reuse `OperatorKeys` and the existing active-tenant directory query to resolve and decode the configured active version for every active provisioned tenant before readiness; abort boot on directory, provider, token, transport, missing-value, base64, or length failure. Preserve development and explicitly single-tenant env/file allowances.
- **Focused closure proof:** a production boot journey backed by a local Vault fixture covers unavailable provider, missing tenant/version, malformed base64, wrong length, and readable 32-byte keys; every failure refuses readiness and the valid tenant set boots. Development's deferred failure remains covered separately.

### FIND-TASK-007-3 — REVISED — VIOLATION: authenticated HTTP headers are attached before effective-URL screening

- **Wave 1 sources:** `TREV-003`, `SEC-004`, `DR-DEL-2`
- **Violated obligation:** REQ-149, TASK-007 Scenario 7, and the task acceptance criteria require each effective HTTP URL and redirect to retain authority and pass resolve-screen-pin before credentials are attached.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/operators.rs:663-692,749-785,842-927`.
- **Evidence:** `HttpRequest::render` inserts bearer/basic/custom credential headers into `HeaderMap`; only afterward does `OperatorDelivery::http` call `client` to resolve, reject, and pin the URL. Each redirect is re-screened, but the same credential-bearing header map exists while that screening runs. The current transport does refuse before send, so the stronger Wave 1 suggestion that decryption itself must follow screening is not required by the approved wording.
- **Observable consequence:** blocked and unresolved destinations enter the screening path with already-materialized credential headers, violating the approved containment order and exposing secret-bearing request state to pre-send handling.
- **Decision-complete correction:** preserve the existing authority check in `OperatorWorker::credential`. Render only URL, authored headers, body, method, and idempotency metadata first; for each initial or redirected effective URL, resolve/screen/pin it, then add the already-authorized credential only to the request builder used for that screened send. Do not move or duplicate registration authority checks, retry classification, redirect restrictions, or connection resolution.
- **Focused closure proof:** an instrumented HTTP delivery test proves blocked/unresolved initial URLs and rejected redirects never reach credential-header attachment or send, while an allowed pinned URL receives the correct auth header and same-origin redirects are screened before each attachment.

### FIND-TASK-007-4 — CONFIRMED — DRIFT: unrelated workflow-skill policy entered the task candidate

- **Wave 1 source:** `TREV-004`
- **Violated obligation:** TASK-007 is limited to Operator connections and delivery, and task acceptance permits no unrelated base-to-candidate change.
- **Exact location:** `.agents/skills/wyrd-implement/SKILL.md:56-78`, `.agents/skills/wyrd-task-review/SKILL.md:80-85`, and the mirrored `.claude/skills/` files.
- **Evidence:** the four changes add repository-wide diagnostician and failure-review policy. No TASK-007 requirement, implementation path, or consumer needs them.
- **Observable consequence:** accepting Operator delivery would silently change how every later implementation and review task operates.
- **Decision-complete correction:** delete the four skill-file changes from this candidate. Preserve all Operator implementation and task evidence. Deliver the workflow policy independently if desired.
- **Focused closure proof:** `git diff base..candidate -- .agents/skills .claude/skills` is empty after remediation, and the Operator-focused verification remains green.

### FIND-TASK-007-5 — REVISED — VIOLATION: the Vault boundary permits plaintext transport and a broadly readable token file

- **Wave 1 sources:** `TREV-005`, `SEC-002`, `STD-007-01`
- **Violated obligation:** REQ-147 keeps KEKs in the external secret boundary; the repository security posture mandates TLS verification and restrictive permissions for file-mounted secrets.
- **Exact location:** `crates/wyrd/wyrd-server/src/config.rs:1897-1906`; `crates/wyrd/wyrd-server/src/components/operators/keys.rs:203-223,416-433`.
- **Evidence:** production validation accepts `http://` Vault addresses before sending `X-Vault-Token` and receiving the KEK. `vault.token_file` uses unrestricted `std::fs::read_to_string`, bypassing the module's existing owner-only permission check.
- **Observable consequence:** network observers can recover or replace the Vault token/KEK, and local users can read a loosely mounted token file; either defeats all Operator credentials protected through that Vault authority.
- **Decision-complete correction:** require `https` for multi-tenant production Vault configuration, retaining plaintext HTTP only for an explicitly non-production local fixture. Route `vault.token_file` through the same owner-only secret-file policy used for KEK files. Do not add a second TLS switch or file-policy abstraction.
- **Focused closure proof:** configuration tests reject production `http://` and accept valid `https://`; key-reader tests reject group/other-readable Vault token files before any request and accept mode `0600`.

### FIND-TASK-007-6 — CONFIRMED — INCORRECT: Python and TypeScript public types do not project the closed wire contract

- **Wave 1 source:** `STD-007-02`
- **Violated obligation:** REQ-148/REQ-150 and the Python/TypeScript authorities require every first-class SDK to project the same closed provider-tagged operations with precise public types.
- **Exact location:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1315-1333`; `sdks/wyrd-sdk-python/python/wyrd/stubs/operators.pyi:1-84` and generated `python/wyrd/operators/__init__.pyi`; actual wire owner at `crates/wyrd-spec/src/operator_connection.rs:485-501`.
- **Evidence:** TypeScript's update accepts arbitrary keys and its view promises a nested `config`. The Rust view flattens provider-specific fields, so no runtime `config` exists. Python exposes every request and view as `Mapping[str, Any]`/`dict[str, Any]`, erasing the closed variants and redacted result fields.
- **Observable consequence:** type-correct TypeScript can dereference a nonexistent field, while both SDKs accept malformed provider combinations that their declared API should reject before runtime.
- **Decision-complete correction:** project the existing Rust/schema contract directly: provider-discriminated update and flattened redacted-view unions in TypeScript, and precise `TypedDict` request/update/view unions in the Python stub generator input. Regenerate owned declarations; do not hand-edit generated stubs or add a parallel DTO shape.
- **Focused closure proof:** static type fixtures reject invalid provider fields and accept every valid provider variant; integration journeys assert each provider-specific flattened read field. Run `codegen:check`, `py:typecheck`, `ts:typecheck`, and both SDK integration lanes.

### FIND-TASK-007-7 — CONFIRMED — VIOLATION: key resolution blocks async request and worker threads with filesystem I/O

- **Wave 1 source:** `STD-007-03`
- **Violated obligation:** AGENTS.md section 6 and the Rust authority prohibit blocking disk work inside async request paths without an explicit blocking strategy.
- **Exact location:** `crates/wyrd/wyrd-server/src/components/operators/keys.rs:150-173,182-223,416-433`; callers at `components/operators/service.rs:109-130,293-307` and `verification/operators.rs:481-527`.
- **Evidence:** async key resolution directly invokes `std::fs::metadata` and `std::fs::read_to_string` for KEK and Vault-token files. Create/update, delivery, and rewrap all await these paths on Tokio workers.
- **Observable consequence:** a slow or stalled mounted-secret filesystem blocks executor threads outside the configured delivery attempt bounds and reduces unrelated request/delivery capacity.
- **Decision-complete correction:** make the existing owner-only reader asynchronous with Tokio filesystem operations (or the repository's existing bounded blocking bridge if required by platform metadata), and reuse it for both KEK and Vault-token files. Preserve mode validation, zeroizing ownership, and current error classification; add no new reader abstraction or dependency.
- **Focused closure proof:** async owner tests cover readable and loose-mode KEK/token files, and a stalled test file source does not block an unrelated Tokio task; run the server/shared lanes.

### FIND-TASK-007-8 — CONFIRMED — VIOLATION: production/library imports are hidden inside functions

- **Wave 1 source:** `STD-007-04`
- **Violated obligation:** `architecture/agent-rules.md` requires non-test imports at module top; none of these sites qualifies for the single-generic-function exception.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/operators.rs:891`; `crates/wyrd/wyrd-server/src/components/operators/keys.rs:419`; `crates/wyrd/wyrd-testing/src/server.rs:668-669`.
- **Evidence:** ordinary production/library helpers contain local `base64::Engine` and Unix `PermissionsExt` imports.
- **Observable consequence:** module headers cease to represent their dependency surface and the hard repository gate is violated.
- **Decision-complete correction:** move the existing imports to their module import blocks, with the existing platform conditional applied to `PermissionsExt`; introduce no helper or suppression.
- **Focused closure proof:** source inspection finds no changed function-scoped imports and `mise run fmt` plus `mise run lints` pass.

### FIND-TASK-007-9 — REVISED — INCORRECT: inline rewrap can starve the sole delivery claim loop past dispatch deadlines

- **Wave 1 source:** `DR-DEL-3`
- **Violated obligation:** REQ-147 requires bounded tenant-scoped rewrap; REQ-142/REQ-146 require bounded, available delivery within a five-minute dispatch deadline.
- **Exact location:** `crates/wyrd/wyrd-server/src/verification/operators.rs:217-250`; `crates/wyrd/wyrd-server/src/components/operators/keys.rs:294-395`.
- **Evidence:** before the first claim and every five minutes thereafter, the only Operator loop awaits the complete cross-tenant rewrap pass. Each 100-row tenant batch holds a transaction and sequentially reads the active key plus the old key once per row; each Vault read may consume ten seconds. The row count is finite, but elapsed time is not bounded within delivery availability, and duplicated old versions are reread.
- **Observable consequence:** stale rows plus a slow/unavailable provider can stop that process from claiming unrelated tenants' due dispatches until their five-minute deadlines expire without an attempt.
- **Decision-complete correction:** keep rewrap on `OperatorKeys` and preserve `TenantConn`, `SKIP LOCKED`, and secret-version fencing, but schedule it independently of the delivery claim loop with cancellation and a finite per-pass/per-tenant elapsed-time budget. Within one bounded pass, reuse each old key version already read rather than issuing one external read per row; do not cache keys across passes or delivery attempts.
- **Focused closure proof:** a multi-tenant runtime test supplies many stale rows and a slow/unavailable local key provider, then proves a due dispatch for another tenant is claimed and settled within its normal deadline while the rewrap pass times out/cancels cleanly and remains retryable; a repeated-old-version assertion proves one provider read per version within the pass.

## Recommendation

The validated ledger contains nine bounded implementation findings. The
appropriate task-review outcome is `FIX_REQUIRED`; remediation can remain under
the approved specification and should delete the unrelated skill changes.

## Verification limits

- This validation was static and time-bounded. It inspected the complete diff,
  all Wave 1 reports, the governing requirements, and the full bodies/callers
  named above, but did not rerun Cargo, Postgres, Python, or TypeScript lanes.
- The task's recorded green lanes do not directly exercise the nine retained
  gaps. The credentialed Slack/PagerDuty live smoke remains gated and was not
  used to validate or reject any finding.
- Generated schemas and duplicate declaration outputs were traced to their
  source/generator owners rather than reviewed line by line.
