# Client transport domain review

Subject: cumulative base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf` to candidate `1b2db8634bf83c03ba210ebf55700983dd9091e6` (tree `52b3fe3243df7f74ba793eaa5be6caa84fba78ad`). The implementation tree is `117f668f6046f15dcfb7b197fc53f8557f4c1757`; the last commit changes only the R2 evidence document. No `.codegraph/` index exists.

## Reviewed boundary and authority

| Boundary | Authority and source coverage | Result |
|---|---|---|
| One shared SDK transport, control versus streaming deadlines | Approved spec revision 9; original TASK-003 and R2 `FIND-TASK-003-R1-8`; `AGENTS.md` §§2–6, 9, 11, 16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` client model; `architecture/references/architecture/patterns.md`; `architecture/references/languages/rust-core.md`; `transport/{http,config}.rs`, `client.rs`, and their base-to-candidate diff | PASS |
| Query cancellation and public deadline | `architecture/bifrost-design.md` public surface and deadline; spec REQ-056 and public deadline invariant; `bifrost/query.rs` → `WyrdClient::request_json_stream_with_id` → `HttpTransport::request_json_stream_inner`; `tests/transport/http.rs` | PASS |
| Authenticated LocalFs and credential-free presigned transfers | `architecture/wyrd-security-posture.md` credential boundary; `storage/upload/{local_fs,single_put,s3_multipart,gcs_resumable,azure_block_blob}.rs`, `storage/download/{local_fs,single_get}.rs`; `auth.rs`; all four transport helpers and `authenticated_url`; `tests/transport/http.rs` | PASS |

## Validation

- `HttpTransport::new` constructs one Reqwest client with `connect_timeout(config.timeout_ms)` and stores the same duration for `send_with_retry`'s per-attempt `RequestBuilder::timeout`. That loop remains the owner of JSON/control calls, including Card, Bifrost table/control, storage planning/completion, and the public but production-unused `request_arrow`; the latter API was not removed or repurposed. `request_json` and idempotent submission still read response bodies under the request timeout. Existing replay-safe, `401` refresh, and problem-response handling are unchanged.
- `request_json_stream_inner`, `request_stream`, `request_raw`, and `request_external_stream` use the connect-bounded client without a total deadline. Query production use is `QueryClient::query` → `request_json_stream_with_id`; it still validates the echoed request ID and media type, and the returned `QueryResultStream` owns the unbuffered response so dropping it abandons consumption. LocalFs upload/download reach `request_stream`/`request_raw`; S3/GCS/Azure transfers reach `request_external_stream`.
- Authenticated helpers still call `authenticated_url`, rejecting a foreign absolute origin before attaching `x-wyrd-access-token` and `wyrd-request-id`. The external helper attaches neither header. `AuthMiddleware` has its own bounded token-exchange client, outside the one-transport-client requirement. No new retry of one-shot bodies was introduced.
- The five focused transport tests exercise delayed JSON-body timeout, terminal-query body survival, authenticated and external slow GETs, and authenticated and external slow PUTs. The PUT fixture consumes the delayed body fully and asserts exact received bytes before reporting success; tests assert Wyrd credentials only on authenticated requests. The R2 evidence records those five passing, `mise run test:shared` with 649 passing, the broader gate and SDK journeys, and all three local live-cloud runs on the unchanged implementation SHA.

## Verification limits

This was a source/diff review, not a rerun of the 25-minute gate or credentialed cloud lanes. The R2 evidence table is the available run record; it gives test counts and candidate identity but not raw CI logs. The four transfer tests use short local fixture delays, which directly test timeout selection; they are not a live long-duration storage guarantee. The approved non-goals remain: Azure SAS lifetime and LocalFs server-side buffering, body-size, and route limits are not claimed closed by this client change.

## Material findings

None.

Overall: **PASS** for the client transport/security/query-stream domain.
