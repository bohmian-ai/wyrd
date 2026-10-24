---
id: TASK-001
kind: implementation
status: complete
spec: SPEC-service-identity-and-telemetry
spec_revision: 12
requirements: [REQ-018, REQ-019, REQ-020, REQ-027, REQ-029, REQ-031, REQ-032, INV-001, INV-006, AC-006, AC-009, AC-010]
depends_on: []
parent_task:
remediates: []
---

> Historical baseline task. TASK-001 is treated as completed by the maintainer's revision-14 instruction; this does not claim new review or test evidence. Revision-14 authority is the current spec and supplements.


# Flat Card identity

## Outcome and Value

Authors declare, reference, read and pull Cards using kind/name/version within
the authenticated tenant: `Service/checkout@1.0.0` needs no space or workspace
default. The server still resolves the exact CardUid. This deliberately breaks
old space-bearing contracts in the resettable v0.3.0 baseline.

## What Changes and What Must Stay True

Remove Card space from metadata/references, registry keys and relationships,
version coordination, config/defaults, declaration/pull, SDKs, CLI, HTTP/MCP,
runtime schemas/guides and affected consumers. Uniqueness becomes
`(tenant_id, kind, name, version)`; CardRef is kind/name/one version/optional
UID, with canonical text `Kind/name@version`. Removed forms fail with
actionable existing typed validation; CLI refuses removed flags.

- Preserve tenant/kind separation, exact UID assertions, permissions, version
  allocation, deduplication, relationship lifecycle checks, declaration
  atomicity, artifact integrity, labels/annotations and their precedence.
- Paths stay loader-local. OTel `service.namespace` is descriptive; no project,
  team, environment or qualified-name requirement replaces Card space.
- Fresh schema only: no historical conversion, collision mapping, hash
  rewriting, dual reads or aliases. Respect applied-migration immutability;
  do not destructively reset an existing environment during implementation.
- This task owns identity consumer/architecture closure. TASK-002 owns account
  behavior, TASK-004 physical storage, and TASK-008 final docsite work.
  Coordinate shared-file edits without adding an artificial dependency.

## Approach

1. Update owning identity/reference/config authority and contributor examples.
2. Flatten existing contract, authoring, loader and config owners.
3. Flatten SQL registry keys, relationships, hashes and version coordination.
4. Close shared client/state/bundle, SDK, CLI, HTTP/MCP and generated consumers.
5. Prove the focused journeys and record consumer closure, including the UI.

## Ordered Implementation Scenarios

### Scenario 1 — Author and validate references without space

**Behavior.** Flat references round-trip with exact version and optional UID;
space-free Cards load path/external/inline references. Labels/annotations
still merge with Card values winning. Removed metadata/reference/config space,
malformed identity and unresolved wire paths are refused.

**RED.** Add the planned [reference](#reference-contract),
[config](#config-contract) and [loader](#loader-contract) proofs. Expect the
new spelling or removed-field refusal to fail; compilation errors do not
count as behavioral RED.

**GREEN.** Extend current contracts, parser, config, loader and authoring
projections; delete space fallback/inheritance while preserving map defaults.

**REFACTOR.** Remove space-only plumbing and obsolete assertions; retain the
existing reference visitor and default-merging owners with these tests green.

### Scenario 2 — Resolve exact identity within tenant and kind

**Behavior.** Fresh registry replay remains idempotent; incompatible content
at an occupied exact pin retains the registration conflict. Versions, kinds
and tenants remain distinct. Relationships, UID assertions, latest/version
queries, pull and delete use flat identity, preserving missing/foreign/inactive
and stale-target refusals. Version allocation coordinates tenant/kind/name.

**RED.** Add the [registry proof](#registry-journey). Assert exact identity,
typed conflict/tenant refusal and relationships; expect remaining space-based
schema/query requirements to fail.

**GREEN.** Extend existing registry/server owners, constraints, projections,
hash inputs and version locks within tenant-bound transactions. Rerun Scenario
1 and registry regressions.

**REFACTOR.** Delete obsolete space filters/projections; retain CardUid and
the existing resolver/transaction workflow with earlier proofs green.

### Scenario 3 — Use one contract across shipped surfaces

**Behavior.** Each SDK declares a checked-in graph, reads/resolves versions,
pulls and loads it offline without space. CLI/HTTP perform their equivalent
supported operations; MCP lists/gets flat references through existing read
tools. Runtime schemas agree, removed inputs fail actionably, and unauthorized
calls remain denied under the same grants.

**RED.** Add planned [Rust](#rust-sdk-journey), [Python](#python-sdk-journey),
[TypeScript](#typescript-sdk-journey), [CLI/MCP](#cli-and-mcp-journeys) and
[HTTP/schema](#http-and-schema-contract) real-server proofs. Expect old space
parameters or advertised shapes to fail, including negative input cases.

**GREEN.** Update shared Cards/state/bundle, thin bindings and Python stubs,
adapters, guides, consumed fixtures/examples, and generated projections.
Rerun earlier scenarios and each affected focused journey.

**REFACTOR.** Remove dead arguments/exports/defaults; keep transport and
registry behavior Rust-owned. Close affected UI consumers without redesign.

## Acceptance Criteria

| Obligations | Observable proof |
| --- | --- |
| REQ-018; AC-006; INV-001 | Scenario 2 proves idempotency/conflict and tenant/kind/version separation. Space-dependent lookup or cross-tenant resolution fails acceptance. |
| REQ-019–020; AC-006 | Scenarios 1/3 prove removed forms fail, valid flat forms work, metadata maps survive and no required namespace replaces space. |
| REQ-027, 029, 031; AC-009 | Fresh fixture databases install the flat schema; source inspection finds no backfill/conversion, aliases or dual reads. |
| REQ-032; AC-010; INV-006 | Scenario 3 plus generation/type checks proves SDK/surface/schema/consumer agreement. Owning architecture and contributor examples agree; final docsite belongs to TASK-008. |

## Implementation Reference

Inspected reuse map; paths are guidance, not a file allowlist.

| Existing owner and inspected consumers | Gap and selected extension |
| --- | --- |
| `wyrd-spec/src/reference.rs::{CardRef, CardRefIdentity, CardRefScope, Display, FromStr}`, `envelope.rs`, `metadata.rs`, `registry/reads.rs`, existing reference tests | Equality, scope text, metadata and selectors include space. Extend these contracts/parser; retain exact version and optional UID. |
| `wyrd-config/src/config.rs::{Defaults, KindOverride}`, `merge.rs::WyrdConfig::apply_defaults_fields`; `wyrd-loader/src/config.rs::apply_defaults`, `parse.rs::card_ref_for` and inline tests | Config and path projection fill/require space. Remove that behavior and reject old fields; preserve discovery, map precedence and reference-slot workflow. |
| `wyrd-sql/src/queries/cards/{declare,get,list,delete,relationships,version_sql,version_query,version_resolve,auth_projection}.rs`, `row_types/cards.rs`, cards/relationships migrations; `tests/integration/pg_cards_declare.rs` | Keys, uniqueness, relationship targets and version locks include space. Flatten within existing tenant-bound transactions; preserve inspected relationship lifecycle rechecks and exact deletion tests. Coordinate auth-projection edits with TASK-002, which removes Card-created principals. |
| `wyrd-server/src/components/cards/{routes,service,resolve}.rs`; `tests/integration/pg_card_declaration_route.rs`; shared `wyrd-client/src/cards/handle.rs::{Cards, CardSelector}`, reads/saga/pull and state consumers | HTTP and client selectors require space. Extend the same declaration/read/pull workflow and permission boundary; no second resolver or transport. |
| `wyrd-cli/src/card.rs`; `wyrd-server/src/mcp/{cards,guide}.rs`; CLI `tests/cli.rs` includes `card_lifecycle.rs` and `mcp_journey.rs` | Arguments, printed refs, MCP descriptions and guides include space. Extend existing CLI/real-MCP-proxy workflows and read catalog; do not invent MCP declaration tools. |
| `wyrd-cards/src/{card_ref,identity,data,model,prompt,agent}.rs`; Python wrappers/public exports/authored `.pyi`; TS `native/src/cards.rs`, wrappers and generated declarations | Authoring/native signatures expose space. Update all binding/export/stub layers; preserve shared Rust ownership. |
| Rust `wyrd-ai` integration module `declare_and_pull`/`support::Deployment`; Python `tests/integration/test_declare_and_pull.py`; TS `tests/integration/declare-and-pull.test.ts`/`serverTest` | Existing SDK stories declare/pull/load `fixtures/cards/declare_and_pull/`. Extend this same public story and checked-in fixtures; close every compiled fixture/example consumer broken by removed fields. |

**Consumer closure.** Recheck identity-bearing callers, state/bundle maps,
generated schemas/types, language type tests, and executable examples.
Inspection of `wyrd-ui/src` found no runtime Card-space property, selector or
rendering consumer; workspace/CSS wording is unrelated. Recheck shared-type
imports and record the result. Update any affected runtime consumer here
under `wyrd-ui`; no new UI workflow or redesign is needed. TASK-008 does not
defer source compilation or runtime guide correctness.

**Authority closure.** Amend `architecture/wyrd-design.md` Card identity,
registry idempotency, reference/declaration and config sections, root
`AGENTS.md` CardRef wording, and touched SDK contributor constructor examples.
Other tasks retain runtime-account and physical-layout authority ownership;
coordinate overlapping files rather than duplicating their changes.

## Verification and Evidence

All names below are **planned**; current manifests/modules confirm packages,
targets, runners and setup. Confirm actual selectors before RED and record
nonzero test counts. Run from repo root; keep Cargo commands sequential.
Prepare `mise run build:server`, `mise run py:setup`, `mise run ts:build` and
`mise run ts:build:testing` before surface tests. Postgres commands retain the
existing lifecycle/migration wrappers. These commands are planned, not run.

### Reference contract

Existing `wyrd-spec` lib: planned flat reference round-trip/rejection test.

`mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=reference::tests::flat_card_reference_contract)'`

### Config contract

Existing `wyrd-config` lib: planned removed-default rejection/map preservation test.

`mise exec -- cargo nextest run --locked -p wyrd-config --lib -E 'test(=config::parse::removed_space_defaults_are_rejected)'`

### Loader contract

Existing `wyrd-loader` lib: planned flat graph load and invalid-input test.

`mise exec -- cargo nextest run --locked -p wyrd-loader --lib -E 'test(=parse::tests::flat_card_graph_loads_without_space)'`

### Registry journey

Existing `wyrd-sql` integration/PgFixture: planned identity, conflict,
tenancy, relationship and version-coordination proof.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-sql --test integration -E 'test(=pg_cards_declare::flat_registry_identity_is_tenant_and_kind_scoped)'"`

### Rust SDK journey

Existing `wyrd-ai` integration, `Deployment`/WyrdTestServer, journey profile:
planned flat public workflow and negative cases.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=declare_and_pull::flat_card_identity_round_trip)'"`

### Python SDK journey

Existing pytest story/WyrdTestServer fixture: planned same Rust story,
including native signatures and returned CardRefs.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_declare_and_pull.py::test_flat_card_identity_round_trip'`

### TypeScript SDK journey

Existing Vitest story/`serverTest`: planned same story and typed results/errors.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/declare-and-pull.test.ts -t "^flat card identity round trip$"'`

### CLI and MCP journeys

Existing `wyrd-cli` cli target/bound WyrdTestServer: planned CLI round-trip
and removed-flag refusal, plus real MCP reads/catalog using existing transport.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::flat_card_identity_round_trip)'"`

### HTTP and schema contract

Existing `wyrd-server` integration/WyrdTestServer: planned flat HTTP contract,
old-selector refusal, runtime OpenAPI shape and permission/tenant negatives.

`mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_card_declaration_route::flat_card_http_contract)'"`

### Owner and static closure

- Registry regression: `mise run test:cards:unit`, `mise run test:cards:integration`.
- Rust format/lints and boundaries: `mise run fmt`, `mise run lints`,
  `mise run check:deps`, `mise run check:tenant-isolation`.
- Python signatures/exports/stubs: `mise run py:format`, `mise run py:lints`, `mise run py:typecheck`.
- TS projections: `mise run ts:format`, `mise run ts:lints`, `mise run ts:typecheck`, `mise run ts:napi:check`.
- Generated projections: `mise run codegen:regen`, then `mise run codegen:check`; never hand-edit outputs.
- Consumed executable examples: `mise run check:examples`.

Non-TDD proof: inspect source/schema/authority for complete removal, fresh
constraints, no aliases/replacement namespace and no unowned callers. Record
UI closure. Final docsite and whole-change journeys belong to their integrated
owners. Record RED/GREEN outcomes, selectors/counts, setup and unavailable proof.

## Material Stop Conditions

Stop for a new identity dimension, compatibility/migration/collision policy,
changed version/conflict semantics, weakened tenancy/permissions/lifecycle, or
an unresolved public reference/route/error decision. Coordinate shared-file
conflicts; do not absorb another task's behavior silently.

## Authority Links

- [Approved specification, revision 12](../spec.md), including all normative
  [auth](../supplements/auth-system.md), [telemetry](../supplements/telemetry-schemas.md)
  and [storage](../supplements/table-storage-and-correlation.md) supplements.
- [Wyrd design](../../../../architecture/wyrd-design.md#reference-forms),
  [workspace config](../../../../architecture/wyrd-design.md#workspace-config-wyrdtoml),
  and [security posture](../../../../architecture/wyrd-security-posture.md).
- [Repository rules](../../../../AGENTS.md),
  [SQL rules](../../../../crates/wyrd/wyrd-sql/AGENTS.md),
  [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md), and
  [TypeScript rules](../../../../sdks/wyrd-sdk-ts/AGENTS.md).
- [Development workflow](../../../spec-driven-development.md),
  [execution](../../../implementation-execution.md), and
  [testing](../../../../TESTING.md).

## Implementation Evidence

Commits `659c9ad26`..`ac9de5bdc` on `change/service-identity-and-telemetry`.
Postgres commands ran under `scripts/postgres/with-test-postgres.sh` after
`mise run db:migrate:all:inner`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-018; AC-006; INV-001 | Migration `crates/wyrd/wyrd-sql/migrations/20261024000000_flat_card_identity.sql` keys `wyrd.cards` on `(data_tenant_id, kind, name, version)` and removes `space`/`target_space` from cards, relationships, and service accounts; `wyrd-sql` queries (`get`, `version_*`, `list`, `auth_projection`, `service_accounts`, `verifier_executions`) resolve by kind, name, version | `wyrd-sql --test integration pg_cards_declare::flat_registry_identity_is_tenant_and_kind_scoped` 1/1 (no space columns; same-tenant duplicate refused; other kind and other tenant register; exact read by kind/name/version); full `wyrd-sql --all-targets` 205/205 | PASS |
| REQ-019–020; AC-006 | `wyrd-spec` `CardRef`/`Metadata` deny `space` and parse `Kind/name@version[#uid]`; `wyrd-config` `[defaults]` takes only labels/annotations; loader graph is flat; CLI selectors have no `--space`; Python/TS/Rust SDK signatures drop `space` | `wyrd-spec reference::tests::flat_card_reference_contract` 1/1; `wyrd-config config::parse::removed_space_defaults_are_rejected` 1/1; `wyrd-loader parse::tests::flat_card_graph_loads_without_space` 1/1; `wyrd-cli auth::issue_key::tests::issue_key_refuses_removed_space_flag` in lib 73/73; `py:test:unit` 634 passed (removed `space=` raises `TypeError`, spaced split ref raises `WYRD_DATA_400_INVALID_SPLIT_RULE`) | PASS |
| REQ-027, 029, 031; AC-009 | New forward migration only; applied migrations untouched; no backfill, alias route, dual read, or compatibility shim (old `/cards/{kind}/{space}/{name}/…` routes deleted) | Fresh fixture databases install flat schema in every Postgres run; `pg_card_declaration_route::flat_card_http_contract` asserts the old space route is 404; source audit `git grep -nwi space` over crates/sdks/examples/architecture finds only unrelated (disk/label/IP) uses and deliberate refusal tests | PASS |
| REQ-032; AC-010; INV-006 | Server routes/OpenAPI/MCP guide, `wyrd-client`, CLI, Rust/Python/TS SDKs, test harnesses, examples, README, and `architecture/wyrd-design.md`/`AGENTS.md` agree on flat identity; generated TS card types, napi `.d.ts`, error catalog docs regenerated | Rust SDK `declare_and_pull::flat_card_identity_round_trip` 1/1 (journey profile); Python `test_declare_and_pull.py::test_flat_card_identity_round_trip` + state journey 21 passed; TS `declare-and-pull.test.ts -t "^flat card identity round trip$"` 1/1 and edited integration files 92 passed; CLI `card_lifecycle::flat_card_identity_round_trip` within card_lifecycle 23/23; server `pg_card_declaration_route::flat_card_http_contract` 1/1, plus fork run of `pg_card_declaration_route`/`pg_openapi_contract`/`pg_hosted_artifacts` 62/62 and server lib 567/567; `codegen:regen` + `codegen:check`, `ts:napi:check`, `docs:check` clean | PASS |

Owner and static closure: `mise run fmt`, `lints`, `check:deps`,
`check:tenant-isolation`, `py:format`, `py:lints`, `py:typecheck`,
`ts:format`, `ts:lints`, `ts:typecheck`, `ts:napi:check`, `codegen:check`,
`test:cards:unit`, `test:cards:integration`, `check:examples`, `docs:check`,
and `git diff --check` all pass.

Environment-limited failures (not caused by this task, unchanged by it):
the full CLI, Rust SDK, and Python integration suites additionally fail only
tests that need the OIDC provider lane (`WYRD_AUTH_503_DISCOVERY_UNAVAILABLE`),
a built production UI (`wyrd-ui/build/index.js`), or a private
`~/.config/wyrd` on this host. `identity_e2e` (Keycloak/Dex lane) compiled but
was not run.

Diagnosis — Python `list(space=...)` raised `WyrdError` instead of `TypeError`:
- **Symptom:** `test_registry_surface.py::test_*_refuse_a_card_space[list-*]` failed with `Python boundary failed`.
- **Evidence:** `RegistryListQuery::from_kwargs` returned `CardPyResult`, so its `PyTypeError` crossed `From<PyErr> for WyrdPyError` and was re-projected as an internal error.
- **Cause:** the list boundary wrapped its own argument error, unlike `RegistryGetQuery::from_kwargs` (`PyResult`).
- **Fix site:** `sdks/wyrd-sdk-python/src/state/mod.rs` — `from_kwargs`, `optional_i32`, and the four `list` methods return `PyResult`; every caller of `RegistryListQuery::from_kwargs` is one of those four.

Non-goals confirmed excluded: no account/principal behavior change (TASK-002;
the Card-created service-account projection only lost its `space` column), no
physical storage change (TASK-004), and `docs/` prose left to TASK-008 except
two walkthrough commands the Python journeys execute verbatim
(`get-started/quickstart.md`, `connect/integrate-asgi.md`) plus regenerated
`api/errors.md` and `llms-full.txt`. `wyrd-ui` had no Card space. Residual risk:
GET query structs ignore an unknown `?space=` rather than refusing it, matching
existing behavior for other unknown query keys.

Status: IMPLEMENTED.

## Approval

Approved by the human owner on 2026-10-22 after the TASK-001-2026-10-22-01 review and the TASK-001-R1 remediation (`f5aa6f2cb`, `73843e810`).
