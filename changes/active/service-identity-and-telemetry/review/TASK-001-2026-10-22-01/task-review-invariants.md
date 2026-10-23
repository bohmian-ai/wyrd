# TASK-001 independent invariant review

Result: **FAIL**.

Subject: base `f38dff803`, candidate `0a29bae4f4ff20e98d2c7937f3943877ac31bc62`, approved specification revision 12, original `tasks/TASK-001-flat-card-identity.md`. HEAD remained the candidate at the end of inspection. No sibling review or implementation completion prose was used. The task's Implementation Evidence was treated as recorded verification, not independently reproduced evidence.

## Coverage and method

Reviewed the cumulative base-to-candidate changes across contract/reference and graph owners; loader/config/default producers; SQL registry, version allocation, relationships, principal projection and verification references; HTTP/MCP and CLI; client reads, pull, bundles and offline state; Rust/Python/TypeScript projections; fixtures, executable examples and architecture edits. Traced the material identity values through existing owners and inspected surrounding source rather than treating compilation or a test name as acceptance. No `.codegraph/` exists.

Applicable authority includes root and SQL/Python/TypeScript/docs AGENTS.md, `changes/spec-driven-development.md`, `agent_docs/index.md`, maintainer guidance, the Card/reference/config/registry sections of `architecture/wyrd-design.md`, and the Postgres tenant-binding section of `architecture/wyrd-security-posture.md`. The broader account and telemetry supplement changes belong to later tasks; this review checks preservation of those existing boundaries rather than requiring TASK-002 behavior here.

The principal producer-to-sink chains inspected were:

- Authored metadata/reference → config map merging → loader path-to-sibling projection → `EffectiveSpecs` resolution → `persist_node` → canonical spec/artifact hashes → version-line lock and existing/fresh resolution → Card and relationship persistence.
- Credentials → authenticated tenant → `TenantConn`/RLS → exact reference, UID, latest/range and list lookups → server hydration → SDK selectors → graph pull and offline `BundleReader`.
- Target reference resolution → active-row `FOR SHARE` recheck → persisted exact UID edge → inbound-reference check under delete row lock.
- Public HTTP query/body → Axum extraction → typed route/service owner. This chain exposes the findings below.

## Obligation matrix

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-018: tenant/kind/name/version registry identity, retaining exact CardUid | Flat unique index in `20261024000000_flat_card_identity.sql`; `cards/get.rs`, `row_types/cards.rs`; kind is retained in every named lookup | Recorded `flat_registry_identity_is_tenant_and_kind_scoped`; inspected assertions distinguish kind and tenant | PASS |
| Preserve idempotency and incompatible exact-pin conflict | `service.rs::persist_node`, `resolve_existing`; canonical hashes still precede comparison; existing registration workflow retained | Recorded cards integration lane; existing inline Prompt declaration/replay/version tests in Rust `declare_and_pull.rs` | PASS |
| Preserve version allocation and deduplication at tenant/kind/name | `version_sql.rs::lock_version_line` keys tenant/kind/name; `version_resolve.rs` retains operation-local Pending and Active eligibility and hash comparison | Recorded SQL and cards regression lanes; no independent rerun | PASS |
| Preserve relationship lifecycle, stale target and tenant refusal | `relationships.rs::recheck_active_card_refs` still requires exact expected UID and active status with `FOR SHARE`; writes and reads use `TenantConn`; source/target tenant joins retained | Inspected `relationship_recheck_blocks_target_lifecycle_race`, `relationship_rows_preserve_exact_target_identity`, exact-delete regression tests; recorded SQL lane | PASS |
| Preserve exact UID assertions | Client `reads/get.rs::assert_selector_identity`; `routes.rs::get_card_by_ref_for`; hosted registry now always invokes the named reference owner and its optional UID check | Existing selector and server tests plus recorded owner lanes | PASS |
| REQ-019: flat reference text/object, one exact version and optional UID | `reference.rs::{CardRef,CardRefIdentity,Display,FromStr}`; `Metadata` and authoring projections remove space | Inspected and recorded `reference::tests::flat_card_reference_contract` and Python runtime tests | PASS |
| Removed metadata/reference/config forms fail locally | `CardRef`/metadata/config `deny_unknown_fields`; loader diagnostics preserve unknown-field cause; defaults retain map precedence | Inspected `removed_space_defaults_are_rejected`, `flat_card_graph_loads_without_space`; recorded Python tests | PASS |
| Removed HTTP selectors must fail rather than map to flat identity | `CardRefQuery` and `DeleteCardRefQuery` silently discard unknown `space` | Current flat HTTP test omits the old by-ref query spelling; INV-001 below | FAIL |
| AC-006/REQ-032: removed HTTP input failures use actionable stable typed validation | Bare `Json<DeclareCardRequest>` and `Query<ListCardsRequest>` return Axum extractor refusals outside `WyrdErrorResponse` | New HTTP test accepts a generic client status and text, not a problem/code; INV-002 below | FAIL |
| REQ-020: no replacement namespace; map defaults survive | No added identity dimension; `apply_defaults`/`apply_defaults_fields` continue per-key insert-if-absent, kind before defaults | Loader test asserts Card label wins and missing default label survives; existing config annotation tests retained | PASS |
| Paths remain loader-local; inline/external provenance retained | Existing typed visitor and `resolve.rs` still project paths; server `EffectiveSpecs` keeps sibling vs external resolution; removed only inheritance plumbing | Recorded loader/card/SDK suites; checked-in graph has path and inline cases | PASS |
| REQ-027/029/031: fresh-only schema, immutable applied migrations, no backfill/aliases/dual reads | Added forward migration; old migrations unchanged; no historical hash conversion or collision mapping; old path route removed | Recorded fresh Postgres runs; HTTP test asserts old segmented path is 404 | PASS |
| Preserve atomicity/artifact integrity/permissions | Existing declaration transaction, artifact manifest hashing/upload owner, audit and permission calls retained; no broader credentials introduced in focused proofs | Recorded cards, artifact, server and SDK lanes; inspected changed callers | PASS |
| REQ-032/INV-006: thin SDKs and common durable owner | Existing `Cards`, `CardGraphPuller`, `BundleReader`, registry and resolver owners extended; wrappers/stubs/generated declarations remove fields without a new transport, graph store or lifecycle owner | Recorded Rust/Python/TS journeys, generation, parity and type checks; inspected relevant signatures | PASS |
| CLI and MCP flat read surfaces and removed CLI flags | CLI argument removal uses normal Clap refusal; MCP args and nested `CardRef` remain strict and dispatch to HTTP's shared audited owners | CLI flat journey asserts usage errors; recorded existing MCP/card suite coverage | PASS |
| UI/executable consumer and authority closure | No runtime Card-space access found in UI shared-type use; executable fixtures/examples and runtime MCP guide updated; design reference/config/registry sections flattened | Recorded examples/codegen/type lanes; stale security wording noted below | PASS |
| Keep account behavior/physical-layout redesign outside TASK-001 | Principal creation/grants remain existing behavior; only their identity projections flatten; no storage-layout redesign introduced | Diff inspection | PASS |

## Proposed findings

### INV-001 — INCORRECT: removed HTTP by-ref space selectors still operate

**Obligation:** original task Scenarios 1/3, REQ-019/032 and AC-006 require removed space forms to fail with actionable typed validation; no mapping or compatibility behavior is authorized.

**Location:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:629–640` (`DeleteCardRefQuery`, `CardRefQuery`); their consumers are `get_card_by_ref_http` at 145 and `delete_card_by_ref_http` at 610.

**Evidence and reachable consequence:** both changed structs remove the `space` field but use ordinary `Deserialize` without `deny_unknown_fields`. A previously valid request such as `GET /v1/cards/by-ref?kind=Prompt&space=prod&name=flat-http&version=1.0.0` is therefore decoded as the flat selector, and reaches `get_card_by_ref_for`. The identical DELETE spelling reaches `service::delete_card_by_ref`, deleting the flat Card instead of refusing the caller's obsolete identity. `into_card_ref` cannot detect the obsolete coordinate after extraction has discarded it. Authentication and normal grants still apply; this is a removed-contract acceptance defect, not a tenant bypass.

**Sibling/owner check:** `ListCardsRequest` already uses `deny_unknown_fields`; `CardRef` and MCP `GetCardArgs` do too. `VersionListQuery` also accepts unknown query fields and should be included when establishing the consistent Card query boundary. Shared SDK selectors and SQL lookups correctly operate on flat identity and should not gain downstream special-case guards.

**Smallest correction and proof:** reject unknown fields at the existing HTTP Card selector decoding boundary and project decoding refusals through the existing typed Wyrd error mechanism. Do not retain or interpret a removed space field. Extend the existing real HTTP test with old GET and DELETE by-ref selectors against an existing active Card. Assert a typed 400 naming the removed input and verify the Card remains readable with the valid flat selector. Include the sibling Card query paths when testing removed query parameters.

### INV-002 — INCORRECT: HTTP removed-field refusals bypass the stable error contract

**Obligation:** AC-006 requires actionable typed refusal; REQ-032 and AC-010 require stable errors and agreement across shipped surfaces. The task expressly requires existing typed validation for removed forms.

**Location:** `crates/wyrd/wyrd-server/src/components/cards/routes.rs:425` (`Json<DeclareCardRequest>`) and `:305` (`Query<ListCardsRequest>`); missing proof at `tests/integration/pg_card_declaration_route.rs:4771–4780`.

**Evidence and reachable consequence:** adding `metadata.space` to a declaration now fails serde decoding inside Axum's bare JSON extractor before `declare_card_http` executes. `GET /v1/cards?space=prod` similarly fails the bare strict query extractor. Those extraction failures return Axum's response rather than any `WyrdErrorResponse`, so clients receive plain text with no catalog `code`, remediation or problem document. This is exactly the new removed-input negative flow required by this task. The new test reads the body as a string and merely checks `status.is_client_error()` and an unknown-field substring; it permits this defect to pass.

**Propagation and existing owner check:** `http/router.rs::apply_protected_edge` maps tower errors, not extractor response values. `http/middleware/request_id.rs` only adds an instance to already-problem+json responses. Existing `components/principals/routes.rs:604–611` converts `Result<Query<...>, QueryRejection>` through Wyrd's mapper, and `components/gateway/routes.rs::body` documents and implements the analogous JSON rejection boundary. No new transport, validator, catalog or global response-rewriting layer is needed.

**Smallest correction and proof:** use the established extractor-rejection-to-typed-error approach at the existing Card HTTP boundary for changed declaration and query inputs (including strict selectors corrected for INV-001), preserving authentication/permission and service ownership. Reuse an existing validation error; do not introduce another error family. Strengthen `flat_card_http_contract` to assert 400, problem media type, a stable existing catalog code and actionable space-field information for metadata, nested reference and list-query removal, with no persisted Card on invalid declaration. Retain valid flat calls and ordinary permission refusals.

## Nonblocking notes and verification limits

- `architecture/wyrd-security-posture.md:667` still says that `space` is a registry namespace inside a tenant. This contradicts the flattened design and should be removed during authority closure. It does not establish an additional runtime failure or independently change this verdict.
- Inspected `bind_scoped_card_ref_uids` because flattening its serialized comparison changes the identity shape. Repository search found no production caller, so a hypothetical generic-object rewrite there is not reported as a reachable task defect.
- The recorded Python list diagnosis is supported: argument `PyTypeError` previously crossed `CardPyResult` and was wrapped; `RegistryListQuery::from_kwargs`, integer extraction and all four list callers now keep `PyResult` at that boundary. No timeout/retry or assertion weakening was used for this correction.
- No Cargo, build, test or external-standard browsing command ran in this independent pass. The reported owner/journey/codegen checks are recorded task evidence only. The known OIDC/UI/environment-limited lanes were not independently reproduced; no conclusion here claims those lanes passed.

The existing owner extensions are materially smaller than replacement machinery and retain server ownership. The two HTTP boundary defects prevent acceptance despite that overall sound approach.
