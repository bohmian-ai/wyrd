---
id: TASK-004-R3
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 27
parent_task: TASK-004-R2
remediates: [FIND-TASK-004-10]
---

# Remove the duplicate request-body limit

Only **FIND-TASK-004-10** remains open. F1–F9 are closed and require no further implementation.

Implement this task through `$wyrd-implement`, starting from reviewed candidate `4478271030da79e7e2eb09fe108fbf1ed10145d8`. The [approved specification](../../spec.md), [review verdict](verdict.md), [validated finding](findings-validation.md), and [previous remediation](../remediation-0c2bfe70d-r1/TASK-004-R2-recovery-boundary-and-histogram-selection.md) remain acceptance context. Original review base: `9692bfd06`; previous candidate: `0c2bfe70d`.

## Finding: two independent limits govern the same request

Wyrd's HTTP middleware enforces the configured request-body limit. Axum then applies its own default 2 MiB limit during body extraction. Raising Wyrd's configuration therefore does not reliably raise the effective limit: a valid 3 MiB request under a configured 4 MiB ceiling is rejected by Axum.

The credential handler exposes this as a plain-text 413 instead of Wyrd's structured error. Other Bytes/Json handlers also inherit the hidden ceiling. The cause is shared router composition, not credential parsing. No privilege escalation or data loss was demonstrated.

Evidence: `crates/wyrd/wyrd-server/src/http/router.rs::apply_protected_edge` installs `WyrdBodyLimitLayer`; that middleware bounds and rebuilds the request but does not disable Axum's extraction limit. Locked `axum-core 0.5.6` applies the additional 2 MiB default. See the [request-boundary review](r3-shared-limit-revision/domain-review-request-bounds.md) for the source trace and existing exception owners.

## Decision-complete remediation

Add Axum's existing `DefaultBodyLimit::disable()` layer in **`http/router.rs::apply_protected_edge`**. Wyrd remains the owner of request-body size policy for every router using that protected edge. Do not add per-handler rejection wrappers or another size setting.

This is the maintainer-directed replacement for the earlier handler-only error-mapping recommendation. Keep the historical review reports unchanged.

Preserve these existing protections:

- Ordinary protected requests remain bounded by Wyrd's configured `limits.body_bytes`, with its existing structured oversize/read-error responses.
- OTLP keeps its Scribe wire limit, transport admission and separate decoded-data bounds.
- Audio, verification and local-upload routes keep their existing explicit readers: configured audio size, verification input size, and authorized upload's declared size respectively.
- Credential authorization, tenant-local principal lookup, delayed typed parsing and normal issuance behavior remain unchanged.
- Apply disablement only through the shared protected edge; independently mounted/unprotected routers retain their current policy.

The existing middleware and handler readers already enforce these bounds. Do not redesign them, raise their limits, change authentication/error policy, add dependencies or introduce a new abstraction. Document the shared owner briefly where Axum's default is disabled.

## Acceptance and focused proof

1. **Configured size works:** through the actual protected router, configure Wyrd to 4 MiB and submit a valid 3 MiB User-key request, using JSON whitespace padding without changing its valid fields. It must succeed with normal issuance/no-store behavior. The current candidate must first fail this case at Axum's 2 MiB ceiling. The 2 MiB figure is only the removed-limit regression boundary; it is not retained policy.
2. **Wyrd's size still applies:** a request exceeding the configured ceiling must return HTTP 413, `application/problem+json`, `WYRD_SPEC_413_PAYLOAD_TOO_LARGE` and normal request correlation. Cover actual-body enforcement without relying solely on Content-Length; reuse the existing router oversize case.
3. **Other protections remain:** retain existing credential refusal/lifecycle cases and narrow body-limit/multipart guard tests. An under-ceiling request to an unauthorized or foreign principal must still receive its existing refusal, and malformed JSON must still receive typed Wyrd validation. Confirm the explicit route-owned bounds above remain in place.

Use existing fixtures, including `WyrdTestServerBuilder::with_limits_for_test`. Add only the focused regression needed for configured-size success; a malformed-body test alone cannot distinguish this fix from an error-message workaround. Record its actual exact selector and RED/GREEN results. No new harness or broad journey matrix is required.

Run the new test and these existing focused checks:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E "test(=pg_verification_routes::system_writer_is_unreachable_through_public_principal_routes) | test(=platform_admin_e2e::one_tenant_cannot_reach_another_tenants_identities) | test(=pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle) | test(=pg_router_smoke::oversized_body_returns_413_problem_json)"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^http::middleware::body_limit::tests::/) | test(=components::gateway::multipart::tests::oversized_and_broken_bodies_are_refused)'
mise exec -- cargo fmt --all -- --check
mise exec -- cargo clippy --locked -p wyrd-server --all-features --all-targets -- -D warnings
```

Run Cargo commands sequentially and document changed Rust items. No full gate, unrelated UI/recovery suite or new performance workload is required. This packet revision changes no implementation and claims no passing implementation tests.

Next review checks `447827103..<new-candidate>` for F10 closure at the shared edge and affected body consumers, preserving F1–F9 acceptance. The two prior remediation rounds resolved their original defects; this remaining correction addresses the duplicate limit exposed by the added credential fix. No new specification or product decision is needed. Commit or push only when asked.

## Implementation evidence

Candidate `447827103..<this commit>`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Configured size works: under a 4 MiB limit, a valid User-key request padded with 3 MiB of JSON whitespace is issued, with no-store | `http/router.rs::apply_protected_edge` layers `axum::extract::DefaultBodyLimit::disable()`, and the traversal doc names it | New test `pg_browser_sessions::a_user_key_request_within_the_configured_body_limit_is_issued`. RED on `447827103`: the 413 response body was not JSON (`JSON body: Error("expected value", line: 1, column: 1)`, Axum's plain-text 413). GREEN after the change: 200, `Cache-Control: no-store`, key returned | PASS |
| Wyrd's size still applies: 413, problem+json, `WYRD_SPEC_413_PAYLOAD_TOO_LARGE`, request correlation, including a body of unknown length | `pg_router_smoke::oversized_body_returns_413_problem_json` now sends both a sized body and a `Body::from_stream` body of unknown length, and asserts `wyrd-request-id` on each | That test | PASS |
| Other protections remain: a foreign or unauthorized principal gets its existing refusal, invalid bodies get typed validation, and the explicit route bounds are unchanged | No change to `issue_credential`, `WyrdBodyLimitLayer`, the OTLP limit or admission, audio (`limits.audio_upload_bytes`), verification (`MAX_EXECUTE_BODY_BYTES`), or local upload (`to_bytes(body, limit)`) | `pg_verification_routes::system_writer_is_unreachable_through_public_principal_routes`, `platform_admin_e2e::one_tenant_cannot_reach_another_tenants_identities`, `pg_browser_sessions::user_keys_preserve_owner_scope_and_credential_lifecycle`, `http::middleware::body_limit::tests::*`, `components::gateway::multipart::tests::oversized_and_broken_bodies_are_refused` | PASS |

The disablement is applied only in the shared protected edge. Routers mounted outside it keep their current policy.

The only changed Rust item is `apply_protected_edge`: one layer was added and its traversal rustdoc updated. The two test functions are documented.

Commands, run one after another:

- `scripts/postgres/with-test-postgres.sh -- … cargo nextest run --locked -p wyrd-server --test integration -E "<the five exact selectors above>"` passed (5).
- `cargo nextest run --locked -p wyrd-server --lib -E 'test(/^http::middleware::body_limit::tests::/) | test(=components::gateway::multipart::tests::oversized_and_broken_bodies_are_refused)'` passed (6).
- `cargo fmt --all -- --check` passed.
- `cargo clippy --locked -p wyrd-server --all-features --all-targets -- -D warnings` passed.
- `git diff --check` was clean.

Non-goals stayed excluded: no per-handler rejection wrapper, new size setting, dependency or abstraction, and no change to authentication or error policy. Historical review reports are unchanged.
