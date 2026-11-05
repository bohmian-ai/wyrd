# Request-boundary review of the directed R3 revision

The selected correction is supported: disable Axum's default extractor limit only in `http/router.rs::apply_protected_edge`, retaining the configured Wyrd limiter and the existing route-owned readers. This removes the competing 2 MiB extractor ceiling at its shared producer. No necessary bound gap or specification conflict was established in the mounted paths reviewed below. This is a recommendation for the requested packet revision, not an implementation acceptance verdict; no corrected implementation exists or was tested here.

## Subject and authority

Code subject: `4478271030da79e7e2eb09fe108fbf1ed10145d8`; prior `0c2bfe70d5186dff4dc224c7e402b57c24f354d9`; original `9692bfd06c6bdf2585e88c791884a90669a5dc26`. HEAD matched the code subject. The existing local task-review skill edit and prior reports were preserved. Only this report is authored by this reviewer.

Scope is the maintainer direction recorded in [scope.md](scope.md), under `$wyrd-task-review`. Relevant authority is root `AGENTS.md` (shared owner, required bounds, typed errors, tenancy), `changes/spec-driven-development.md` (current user direction precedes private task mechanics), `agent_docs/maintainer-style.md`, `agent_docs/errors.md` (single public problem mapper), and `architecture/bifrost-design.md` System boundary (server owns request limits; Gate requires verified authority). The existing validated `FIND-TASK-004-10` diagnosis is retained. Its historical per-handler rejection conversion is superseded by this human-directed owner choice, without creating another FIND ID or rewriting historical ledgers. No unrelated credential, UI, persistent-data or lifecycle discovery was reopened.

## Producer, extension and ordering evidence

CodeGraph was consulted before code navigation. Current `http/router.rs:303–341` wraps the already-mounted API/MCP router in `apply_protected_edge`; `app/server.rs:187–198` reuses it for `merge_http_protected`. Core authentication layers remain inside that edge. Request ID, panic handling, error mapping, capacity, timeout and existing authentication/tenant layers need no relocation.

`http/middleware/body_limit.rs:105–224` rejects an excessive declared length, then reads ordinary bodies through `Limited` and checks collected length against its selected ceiling. Both oversize branches return Wyrd `PayloadTooLarge`; stream failures return Wyrd `ServiceUnavailable`. It reconstructs the request with `Request::from_parts(parts, Body::from(bytes))`, preserving request extensions. Transport charges remain held until the inner response returns, with existing cancellation/drop ownership unchanged.

`Cargo.lock:749–750,782–783` locks Axum 0.8.9 / axum-core 0.5.6. Local dependency source `axum-core-0.5.6/src/extract/default_body_limit.rs:210–236` proves the layer inserts `DefaultBodyLimitKind` into request extensions and forwards the same request. `src/ext_traits/request.rs:316–332` proves Disable returns the existing body unchanged; without that extension it installs the 2,097,152-byte `Limited`. `src/extract/request_parts.rs:100–114` proves Bytes consumes that limited body. Thus installing disablement at the protected owner before extraction survives Wyrd's request reconstruction and removes only Axum's implicit extractor cap. It does not remove the independent Wyrd `Limited`, explicit `to_bytes` calls or FormReader checks. No repository `DefaultBodyLimit` override currently exists.

For F10, `components/principals/routes.rs:398–445` currently takes bare Bytes, then authorizes, resolves the tenant-local principal and parses via `Json::from_bytes` with `body_rejection`. Under a configured ceiling greater than 2 MiB, Axum's default rejects before those handler steps. Shared disablement lets bounded bytes reach those existing steps. It needs no BytesRejection adapter, DTO change, error code, configuration or response normalizer.

## Effective bounds and preserved consumers

Paths below are relative to `crates/wyrd/wyrd-server/src/` unless identified otherwise.

| Consumer | Effective Wyrd bound after the selected correction | Source evidence |
|---|---|---|
| Ordinary protected JSON/Bytes/raw-body routes, including credentials and MCP | Configured `limits.body_bytes`; rejected before inner service when exceeded, regardless of extractor choice | `http/router.rs:119–175,303–341`; `http/middleware/body_limit.rs:120–224`. `/mcp` is merged before the shared edge and is not an exemption. |
| OTLP traces, metrics and logs | Scribe's selected encoded request ceiling rather than generic HTTP ceiling, plus the same process transport admission; decoder preflight/reservations remain | `http/middleware/body_limit.rs:120–208`; `http/router.rs:321–331`; `http/otlp.rs:241–285,355–400,470–514`; `crates/vala/vala-bifrost-redux/src/gate/mod.rs:400–411` exposes immutable decode/wire limits and `max_frame_bytes`. |
| Gateway JSON and image forms | Generic configured body ceiling at the edge; image FormReader also receives that ceiling | `components/gateway/routes.rs:1107–1122,1162–1177,1945–1960`; `components/gateway/multipart.rs:94–103,200–216`. Gateway provider-envelope/domain behavior remains owned by its existing handlers. |
| Exact audio transcription/translation exemptions | Raw Body is read only for an authenticated caller through FormReader with `limits.audio_upload_bytes`, spooling file content; `fill` checks cumulative consumed bytes before extending its buffer | `http/middleware/body_limit.rs:107–118`; `components/gateway/routes.rs:1267–1278,1317–1328,1333–1374`; `components/gateway/multipart.rs:94–103,200–216`. FormReader intentionally stops at the closing boundary; disablement changes neither this behavior nor its cap. Audio speech remains under the ordinary edge bound. |
| Exact verification execute/enqueue/evidence exemptions | Raw Body read with explicit `to_bytes(body, MAX_EXECUTE_BODY_BYTES)` before deserialization, yielding the existing verification input-too-large error | `http/middleware/body_limit.rs:107–118`; `components/verification/routes.rs:49–59,137–147,183–192,291–308`. All three exempt handlers call that reader. |
| Local-upload prefix exemption | Authentication and upload authorization/tenant lookup precede reading; explicit `to_bytes` limit comes from the persisted upload's declared size, with checked usize conversion and existing PayloadTooLarge mapping | `http/middleware/body_limit.rs:114–118`; `components/storage/routes.rs:412–446`. No Axum extractor limit governs this raw Body path today. |
| Extension routers | `merge_http_protected` receives the same configured shared edge; `merge_http` retains its caller-owned edge and existing extractor policy | `app/server.rs:164–198`. Disablement must stay on the router passed to `apply_protected_edge`, not an outer layer on the final merged server. |
| Unprotected built-ins | Health/readiness, OpenAPI and metrics do not consume request bodies; they do not acquire the selected disablement | `http/router.rs:49–57,158–180`. |

The exemption set was traced in full, not inferred from raw Body extraction alone. Existing exemption paths and selected bounds must remain unchanged. No supported mounted consumer was found that becomes unbounded by removing Axum's default. Inventing additional extension routes under exempt reserved paths is outside this bounded review and does not justify new machinery.

## Minimum correction and required implementation proof

One native Axum layer at the existing protected-edge owner is sufficient. Per-handler conversion would retain the conflicting ceiling and reject a valid body below Wyrd's configured maximum. Increasing the default extractor ceiling duplicates limit policy; changing Wyrd buffering, exemption routing, transport reservations, decoder limits, authentication or gateway error policies is unnecessary. Changed owner rustdoc should explain the single Wyrd policy and independent route-owned bounds.

Required focused proof when implemented:

1. Drive the actual protected router with configured ceiling above 2 MiB and a semantically valid JSON request larger than 2 MiB but below that ceiling. Assert handler success/effect, not merely absence of text/plain or a canonical validation error. An existing valid credential DTO padded with JSON whitespace can demonstrate this without new public behavior or a new harness. A malformed-only test could pass a downstream mapper and would not prove this correction.
2. Retain F10's actual mounted credential malformed-body canonical validation, unauthorized/foreign-target refusal precedence and absence of forbidden mutation. Preserve existing normal User issuance, absent-User refusal, empty-Service success and identity refusal cases.
3. Above the configured ordinary ceiling, assert Wyrd 413 `WYRD_SPEC_413_PAYLOAD_TOO_LARGE`, problem media type and correlation, including an undeclared-length body so the proof does not only exercise Content-Length. The handler must not run. A shared-edge Bytes/Json test must traverse `apply_protected_edge`, not only `wyrd_body_limit`.
4. Retain existing focused body-limit and multipart tests as checks of the independent bounds/admission paths; preserve the source-proven raw-reader bounds above. Do not claim isolated middleware tests prove shared-edge installation or end-to-end exempt-handler authorization/bounds. Additional runtime proof should be focused on any actually changed bound/reader, rather than requiring every exempt family to grow another end-to-end suite for an unchanged raw Body path.

Existing exact test owners are `http::middleware::body_limit::tests` (all five existing cases, including `scribe_body_limit_is_route_local`, `undeclared_body_is_charged_while_collected_and_released_on_drop`, and `audio_uploads_pass_the_body_limit_unbuffered`) and `components::gateway::multipart::tests::oversized_and_broken_bodies_are_refused`. Confirm the exact new selector after implementation; execute it through the repository Postgres/migration owner if it uses a principal integration fixture. Existing tests alone are not RED/GREEN evidence for the new edge behavior.

## Recommendation and limits

Proceed with the directed R3 packet revision at this shared owner, retaining F10 and clearly marking the historical local-mapper remedy as superseded. A fresh independent validator should check this report and the final revised packet. No additional material finding, bound repair or specification revision is proposed.

No runtime tests, new harness, formal code acceptance, code changes, commits or pushes were performed. The principal residual risk is an implementation that places disablement outside the protected router or drops an explicit bound; the traced owner boundary and required shared-edge success/oversize proof specifically constrain those mistakes.
