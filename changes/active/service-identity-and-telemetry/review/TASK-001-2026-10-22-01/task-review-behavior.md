# TASK-001 independent behavior review

Result: **FAIL**.

Base: `f38dff80327bb771975e2cdd8fbed283ab145159`.
Candidate: `0a29bae4f4ff20e98d2c7937f3943877ac31bc62`.
Subject: cumulative base-to-candidate range, approved specification revision 12,
its normative supplements, and original `TASK-001-flat-card-identity.md`.
HEAD was checked against the candidate during inspection. No source was changed,
no sibling report was consulted, and no build or test command was run.

## Coverage and caller paths

I inspected the cumulative change across the contract, loader/config, registry,
server, client/state, bindings, CLI, fixtures, executable examples, runtime
guides, generated projections, and affected tests. The navigation map was
expanded to the HTTP extractor/error boundary, service-account lookup,
verification hosted registry, operator context rendering, SDK test support,
and request-ID middleware. No `.codegraph` directory exists.

| Changed boundary | Caller-to-result path and regression check |
|---|---|
| Identity and reference values | `Metadata`, `CardMetadata`, `CardRef`, `Ref`, `InlineableRef`, `CardRefIdentity`, `CardRefScope`, graph hashing/topology/canonical order; flat parsing and UID consistency continue through existing owners. |
| Authoring and defaults | YAML parse → typed spec/reference visitor → path resolver → map defaults → duplicate/reference validation; Data/Model/Prompt/Agent projections remove space without changing label/annotation precedence. |
| Persistence | Declaration transaction → version-line advisory lock → existing-pin/hash resolution → insert → relationship recheck/persist; flat unique/latest/hash indexes, row mappings, exact reads, latest/list/versions and deletion inspected. |
| Server | HTTP/MCP → authorization/audit → service → tenant transaction → storage integrity hydration; flat latest/versions routes and by-ref query paths inspected separately. |
| Client and offline state | `Cards` selectors → shared reads/declaration/pull → aliases/bundle manifest → `BundleReader`; UID and artifact checks retained, fallback aliases remove the space component. |
| Python | Public `CardRef`, typed registries, `Cards`, config, testing CLI, authored stubs and kwargs boundaries; list's `PyResult` correction matches its stated failure diagnosis. |
| TypeScript | Public wrappers → N-API → Rust `Cards`; native/generated signatures, Card types, CLI testing projections and state journeys agree. |
| Other runtime consumers | Auth scope, issue/exchange/refresh/revoke fixtures, workload bindings, Skald Agent projection, Vala references, verification caches/hosted registry, operator dispatch text and SQL projections. Account behavior remains with its existing owners. |
| Documentation and UI | Owning Card/config design sections, root contributor contract, constructor examples, runtime MCP/setup guide and executable snippets changed. UI `src` contains no Card-space consumer or CardRef import; workspace/CSS text is unrelated. Final docsite prose remains TASK-008 scope. |

## Obligation matrix

Verification below distinguishes inspected tests and reported implementation
results from independent execution. A PASS means the inspected implementation
satisfies that obligation within this review's static/runtime-evidence limits;
it does not claim that I reran the lane.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-018: identity is authenticated tenant/kind/name/version; CardUid remains exact | Flat migration unique index; `CardRef::same_identity`; SQL `get`, version line and `TenantConn` paths | Inspected `flat_registry_identity_is_tenant_and_kind_scoped`; reported SQL 205/205 | PASS |
| Preserve kind/version separation | `CardRefIdentity`, graph keys, SQL query binds and advisory-lock key retain kind/name/version as appropriate | Reference contract and registry test; retained version tests | PASS |
| Preserve tenant isolation; INV-001 | Registry service obtains connection from caller tenant; references carry no tenant; RLS unchanged | Registry second-tenant proof, existing HTTP/SDK tenancy negatives retained | PASS |
| REQ-019: metadata/reference identity has no space | Removed fields/types; strict metadata/CardRef serde contracts; flat display/parser and all projections | Reference/loader contracts; Python runtime refusal tests; TS journey negatives | PASS |
| Removed HTTP selector forms fail instead of mapping | `CardRefQuery` and `DeleteCardRefQuery` lack strict unknown-field decoding after removal of space | New HTTP test covers obsolete path and metadata, but never old by-ref `space=` query | **FAIL — BEH-001** |
| Removed HTTP forms use actionable existing typed validation; REQ-032 | New serde refusals exit through raw Axum `Json`/`Query` extractors | HTTP test asserts any client error plus substring, no Wyrd problem/code | **FAIL — BEH-002** |
| Canonical `Kind/name@version[#uid]` and malformed text refusal | `CardRef::{Display, FromStr}`; scope claim text; operator context SQL and guides | Reference round trips/rejections and SDK/CLI assertions inspected | PASS |
| Paths remain loader-local; unresolved wire paths refused | Loader path projection and existing server reference visitor/validation retained | Loader tests and retained wire-path rejection tests | PASS |
| REQ-020: no namespace substitute | Identity/default scalar plumbing deleted; maps remain optional | Static inspection; map-default tests | PASS |
| Preserve label/annotation precedence | Existing config merge owner remains per-key Card > kind > defaults; no server config lookup added | Config/loader merge proofs and retained annotation tests | PASS |
| Preserve idempotency, occupied-pin conflict and automatic versions | Existing `persist_node`/`resolve_existing`/`resolve_version`; canonical hash inputs remove only identity's space | Existing declaration replay/conflict/version tests; reported card integration lane | PASS |
| Preserve relationship exactness and missing/foreign/inactive/stale refusal | Existing batch resolution, asserted UID checks, active-target recheck and row locks remain | Retained relationship identity/race/delete tests and declaration negatives | PASS |
| Preserve declaration atomicity and artifacts | Same transaction and post-commit blob/upload workflow; integrity verification retains hash/UID/tenant checks | Retained heavy-artifact, completion and integrity tests; reported hosted-artifact checks | PASS |
| Preserve permissions and audits | Existing HTTP/MCP audited owners and operation grants remain; no broader credentials substituted in affected journeys | Existing permission tests retained; fixture changes remove only space coordinates | PASS |
| Fresh schema, applied-migration immutability; REQ-027/029/031, AC-009 | One additive forward migration; applied migrations unchanged; no data backfill/hash conversion/collision policy/dual reads | Fresh-fixture runs reported; schema-column proof inspected | PASS |
| Scenario 3 Rust SDK read/declare/latest/pull/offline agreement | Shared client/selector/state owners flattened; Rust SDK remains thin | New named read/ref test plus existing declare/pull/state journeys; focused journey reported 1/1 | PASS |
| Scenario 3 Python SDK agreement and removed arguments | Native signatures/kwargs validation, exports/protocol/stubs/fixtures flattened | Public Python journey and registry refusal tests; reported 634 unit and 21 journey/state passes | PASS |
| Scenario 3 TypeScript SDK agreement | N-API/wrapper signatures and generated types flattened | New flat journey explicitly pulls and loads `WyrdState`; reported focused 1/1 and 92 integration passes | PASS |
| CLI equivalent operations and removed flags refused | Clap space fields deleted, selector calls/printing flattened, testing bindings updated | New CLI journey and issue-key refusal unit test; reported lifecycle 23/23 | PASS |
| MCP read tools and runtime schemas agree | `GetCardArgs`/`ListCardsRequest` use strict shared flat contracts; existing read-owner dispatch retained; guides updated | MCP catalog/read journeys remain existing owners; codegen/OpenAPI checks reported | PASS |
| REQ-032, AC-010 consumer/authority closure | SDK declarations, authoring/runtime consumers, examples and owning identity sections updated | Reported typecheck, napi/codegen/examples/docs checks; UI source rechecked | PASS, except HTTP validation rows above |
| INV-006: server owns durable decisions | No new SDK resolver, registry, transport, authorization workflow or durable client state | Static comparison of changed owners and call chains | PASS |
| TASK-002/004/008 boundaries and non-goals | Only space removed from account projection; no account redesign, physical storage change, namespace alias or replacement dimension | Cumulative diff inspection | PASS |

## Findings

### BEH-001 — INCORRECT: old by-ref HTTP space selectors still perform operations

**Obligation.** TASK-001 explicitly requires removed metadata/reference/config
space and removed selectors to be refused; AC-006 requires removed space forms
to fail. REQ-031 excludes compatibility reads.

**Location.** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:629`
(`DeleteCardRefQuery`) and `:636` (`CardRefQuery`); consumers at `:139` and
`:611` (`get_card_by_ref_http`, `delete_card_by_ref_http`).

**Evidence and reachable consequence.** Both DTOs removed `space` but still
derive ordinary serde `Deserialize`, which ignores unknown fields. Therefore an
authenticated writer's existing request
`DELETE /v1/cards/by-ref?kind=Prompt&space=staging&name=answer&version=1.0.0`
decodes successfully to the same reference as the flat selector. The handler
authorizes the write and calls `service::delete_card_by_ref`; the SQL owner
selects by kind/name/version and soft-deletes it. A corresponding GET returns
the flat Card. The removed coordinate is silently discarded, potentially
deleting a Card while a caller believes a namespace constrained the operation.
This is not a tenant bypass; it is an explicitly prohibited interpretation of
obsolete identity input.

**Shared owner and siblings.** `ListCardsRequest`, MCP `GetCardArgs`, `CardRef`,
`Metadata`, config defaults and workload-binding config already reject unknown
fields. CLI removed flags are rejected by Clap; Python kwargs reject space.
Only the two by-ref query DTOs retain this old-selector acceptance. Latest and
versions remove their path segment, so the old path no longer resolves.

**Smallest correction.** Make both changed by-ref DTOs reject unknown fields at
their existing HTTP trust boundary. Do not add a space parser, alias, fallback,
SQL guard or namespace mapping. Preserve their existing authorization and
service/transaction flow. Return the existing typed validation response as
described in BEH-002.

**Focused proof.** Extend the existing HTTP flat-contract test with GET and
DELETE against a real declared Card using `space=default` and a different
space value. Assert typed 400 refusal, then read the Card through the valid
flat selector to prove neither delete mutated it. Keep valid GET/DELETE
behavior and the existing tenancy/permission assertions.

### BEH-002 — INCORRECT: newly refused HTTP space forms bypass Wyrd's typed errors

**Obligation.** TASK-001 requires actionable existing typed validation for
removed forms; REQ-032/AC-010 require stable cross-surface errors.
`agent_docs/errors.md` requires HTTP problem payloads from the Wyrd error mapper.

**Location.** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:425`
(`Json<DeclareCardRequest>`), `:305` (`Query<ListCardsRequest>`), and the by-ref
extractors at `:142`/`:614` once strict decoding is corrected.

**Producer-to-consumer evidence.** The new
`wyrd-spec/src/envelope.rs:163` `deny_unknown_fields` makes a declaration with
`metadata.space` fail serde decoding. The new strict `CardRef` similarly
rejects reference space; `ListCardsRequest` rejects the now-unknown space list
filter. These routes destructure raw Axum extractors, so deserialization
failures return the extractors' rejection response before the handler's
`WyrdErrorResponse` conversion runs. Metadata/reference rejection uses Axum's
JSON data-error response, including its 422 status/plain text, instead of the
published typed 400 Wyrd problem. Query rejection likewise has no stable Wyrd
code. `http/middleware/request_id.rs` only enriches responses already marked
problem+json; it does not normalize these rejections.

**Observable consequence.** Direct HTTP clients and agents receive a framework
error without the promised `code`, `details`, remediation or problem media
type. The checked-in `flat_card_http_contract` explicitly permits this by
asserting only `is_client_error()` and `unknown field` text. The local SDK
loader tests do not exercise this HTTP decode boundary.

**Shared owner and smallest correction.** Retain the strict `wyrd-spec`
contracts and existing Card handlers. At the affected HTTP adapters, convert
extractor rejection into an existing `WyrdError` validation variant and use
`WyrdErrorResponse`; the existing
`components/principals/routes.rs:604` `Result<Query<_>, QueryRejection>` path
and gateway/storage adapter rejection handling show this repository pattern.
Do not add a new error family, middleware response rewriter or parallel
decoder. This guard belongs at the HTTP adapter because it owns framework-to-
public-contract error projection. Preserve decoding limits, authorization,
auditing and declaration/service behavior.

**Focused proof.** In the existing real-server HTTP contract proof, submit
removed metadata space and removed reference space, and request a removed
list filter. Assert status 400, `application/problem+json`, the selected
existing stable validation code, actionable field detail, and no declaration
writes. Apply the same response assertions to BEH-001's rejected by-ref
selectors. No broad journey suite is needed to close these bounded gaps.

## Reuse and proof assessment

The candidate primarily deletes a dimension through existing owners. Graph
identity, defaults, registry resolution, version locks, pull/state, auth lookup
and thin bindings were extended rather than replaced. The small `Ref`
deserializer preserves the existing forms while exposing useful CardRef
decode diagnostics; it does not add an alternate resolution workflow.
I found no additional behavioral complexity finding within this task.

The recorded Python list diagnosis is consistent with source: returning
`PyResult` at the kwargs boundary avoids converting its `PyTypeError` to a
Wyrd internal error; the four list callers use the same correction. The removed
two-space tests exercise a contract that no longer exists; retained
tenant/version/kind and lifecycle tests carry the applicable regressions.
No credential broadening, timeout/retry weakening or failure-hiding skip was
identified in the inspected test changes.

Implementation evidence reports focused and broader green lanes, but those
were not independently rerun here. OIDC identity journeys were compiled but
not run, and full-suite limitations concerning provider setup, production UI
build and private saved-login state remain unverified by this reviewer. They
do not explain or close the deterministic HTTP boundary findings. No source
mutation or historical environment reset was performed during this review.

Skipped: independent builds/tests and deferred final docsite prose.
Risk: removed HTTP inputs still either operate on a flat Card or return an
untyped framework refusal until BEH-001 and BEH-002 are corrected.
