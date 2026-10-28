# Repository standards review — TASK-001

**Result: FAIL.** Three repository-rule findings remain. This is a standards
audit, not task acceptance or an implementation-preference review.

Subject: `3899b0a35...826fcf9c742a36bcae5e8f68307e76685d88244e`.
HEAD was independently checked and still names the candidate. This is the
first review, covering the full immutable range; no prior evidence is carried
forward. No `.codegraph/` directory exists. Only this report was written; no
source was changed and no Cargo command was run by this reviewer.

## Authority coverage

All 34 changed paths were inspected through the recorded Git range and current
source. `rg --files -g AGENTS.md crates/vala crates/wyrd` confirms that the SQL
file below is the only nested instruction file in the changed trees.

| Changed surface | Applicable authorities read | Source coverage |
|---|---|---|
| Architecture documents | Root `AGENTS.md`; `architecture/README.md` normative writing and authority map; `changes/spec-driven-development.md` authority order | All changes in `architecture/{wyrd-design,bifrost-design,wyrd-security-posture}.md`; registry Card blob/heavy-artifact sections, Bifrost system/table identity, Scribe and reserved-write/internal-read sections, security peer trust/tenant binding/audit sections |
| Bifrost built-ins and write admission | Root rules; `agent_docs/{index,rust,maintainer-style,errors,agent-surfaces}.md`; Bifrost table identity, daily partitioning and reserved-write authority | `gate/mod.rs`, `namespaces.rs`, `tables/{mod,staged}.rs`, `tables/storage/{mod,objects}.rs`; changed tests and surrounding schema/layout/projection methods |
| Forge coordination | Root rules; Rust guide; Bifrost maintenance ownership; security Postgres tenant binding | `vala-sql/migrations/20261028000000_forge_storage_namespace.sql`, `row_types/forge_tasks.rs`; validated namespace constructor |
| Server inventory and Card lifecycle | Root rules; Rust/error/maintainer/agent-surface guides; Card blob and heavy-artifact authority; Bifrost inventory/internal read authority; security tenancy and audit | `components/storage/{mod,inventory,routes}.rs`, `components/cards/service.rs`; all changed read, completion, reconciliation, cleanup, envelope and deletion methods and their adjacent owners |
| Outbox and peer capture | Root rules; Rust/error guides; Bifrost Scribe/outbox ownership; security peer destination, tenant and audit rules | `scribe_outbox.rs`, `grpc/capture_peer.rs`; Object variant, every changed variant match, destination resolution, submit/tally/lane/weight/flush, ObjectGate and ACK test |
| Verification runtime and metrics | Root rules; Rust/maintainer guides; Bifrost execution-input/result owner and outbox rules; security tenant binding | `app/metrics.rs`, `verification/{drift,fitter,mod,runner,telemetry}.rs`; fitter state/signatures and resolve/read path, runtime construction, deferred-attempt telemetry |
| SQL ownership/removal | Root rules; `crates/wyrd/wyrd-sql/AGENTS.md`; security Postgres tenant binding; Rust/error guides; `TESTING.md` | Drop migration, `src/lib.rs`, deleted `queries/storage/artifact_metadata.rs`, `queries/storage/mod.rs`, `tests/integration/pg_migration.rs`; actual changed production connection signatures and test fixture capabilities |
| Tests/fixtures | Root testing and Rustdoc rules; Rust guide; `TESTING.md` tier/layout/credential/diagnosis rules; nested SQL rules where applicable | CLI `tests/card_lifecycle.rs`; server `tests/integration/{pg_card_declaration_route,pg_verification_runtime}.rs`; SQL migration test; testing `tests/bifrost/server/verification_runtime.rs`; inline gate/staged/schema/outbox tests |
| Execution documentation | `changes/spec-driven-development.md`; root testing/verification rules; `TESTING.md` | Appended TASK-001 evidence, including two diagnosed runtime failures and commands; review-time `verification.md` |

No Python, TypeScript, binding, UI, manifest, generated artifact, deployment or
operations file changes occur in this range. Their implementation-specific
rules do not independently apply to this task's write set. Later integrated
language journeys are explicitly allocated by the task; this report does not
turn them into per-task acceptance conditions.

## Applicable rule results

| Rule | Result | Source evidence |
|---|---|---|
| Use owning architecture authority and one Wyrd vocabulary | PASS | Architecture changes state the one Bifrost inventory and narrow read/write boundaries in their respective owners; no aliases or alternate public nouns introduced. |
| Durable behavior remains in server owners, dependency boundaries remain intact | PASS | No Cargo dependency/features change; table behavior is Vala-owned; `ObjectInventory` is server-owned; storage has no reverse server dependency; SDK clients remain unchanged. |
| Concrete owner for retained runtime state; synchronous pure work and async IO | PASS | `ObjectInventory` holds AppState; `BaselineFitter` holds WyrdPostgres/OperatorPool/AppState; object row projection and folding are synchronous; record/query/completion await IO. Detailed duplication/abstraction acceptance belongs to the separate maintainer review. |
| Production SQL uses real TenantConn/OperatorPool capabilities | PASS | Inventory obtains `registry_tenant_conn` before `system_principal_id`; Card helpers use TenantConn; `persist_completed_upload` takes `&mut TenantConn<'_>`; fitter fields/signature use `WyrdPostgres` and `OperatorPool`. No added production PgPool, bare connection or raw transaction signature/field. |
| Tenant transaction ownership is retained | PASS | `persist_completed_upload` composes upload/manifest writes without committing; caller `finish_upload` owns commit. New inventory authorization reads commit their own acquired TenantConn. Fitter drops its read transaction before calling the next owner's lookup. |
| SQL migrations immutable and ordered | PASS | Only new `20261028000000` migrations added; original creation migration stays immutable. Forge constraint extension and inventory drop are owner-local. No data backfill/cutover machinery. |
| Coordination timestamps remain Postgres-owned | PASS | No new coordination deadlines are persisted. Inventory `recorded_at` is an event recording time. Tokio timeout is request waiting, not a durable coordination lease. |
| Tenant/Card binding and reserved public writes | PASS for structural repository rules | Inventory uses typed tenant/Card IDs; table-scoped SystemReadAuthority; fold validates file storage path against tenant and Card; Gate reserves Storage/Objects; existing peer destination allowlist grows by one enumerated destination. Sensitive behavioral completeness is independently reviewed by domain reviewers. |
| Public failures flow through existing typed catalog mapper | PASS | New failure returns reuse BifrostError WriterUnavailable/QueryVisibilityUnavailable and WyrdError; no handwritten code()/problem JSON/new error family. Handlers return the established WyrdErrorResponse edge. |
| Handler tracing with scrubbed arguments | FAIL | Changed `complete` and `download_init` have no instrument attribute; see REPO-003. |
| Every new/materially changed Rust item has useful rustdoc, Errors/Panics where required | FAIL | Changed storage/lifecycle helpers lack docs/Errors and new fixture fields/helpers lack required docs/Panics; see REPO-001. Existing docs elsewhere do cover the new schema fields/variants, ObjectFact/CardObjects, ACK fields and primary new methods. |
| No new unchecked unwrap/unsafe/platform-specific runtime path | PASS | New production code uses fallible conversion/typed mapping; no unsafe, unsupported OS gate, new runtime or library anyhow introduced. Poison recovery is explicit. Test expect calls are separately subject to Panics docs. |
| Tests stay at appropriate dependency tier and use credential-isolated locations | PASS | Server-backed tests are pg_* files or CLI pg_tests; ACK/schema/projection tests are inline and in process; existing integration binaries reused. No test binary/dependency introduced. |
| SQL-backed tests never silently skip due to missing environment | FAIL | Newly named fresh-install proof returns successfully if database_url() is None; see REPO-002. |
| Existing assertions and producer proof retained without unexplained gate weakening | PASS within standards scope | SQL metadata assertions replaced by inventory/absence assertions; changed runtime metric selector excludes diagnosed deferred claims, matching changed emitted metric semantics; durable-row fixture now settles its actual fault outbox before flush. TASK-001 records diagnoses, caller checks and independent diagnostician claims. No added allow/ignore or new skip was used to address either diagnosed runtime failure. |
| No hand-edited generated artifacts; stable public shapes | PASS | No generated output changed; existing request/response types retained; OpenAPI annotations add applicable inventory failures. Task evidence records codegen:check success. |
| Narrow verification and exact selectors | PASS with evidence limit | Review-time verification records exact V1–V4 reruns passing. TASK-001 records format/lints/storage drift/tenant isolation/dependencies/codegen and owner regressions passing. This reviewer did not rerun broad commands. |

## Material findings

### REPO-001 — VIOLATION: changed-item rustdoc gate is incomplete

**Authority.** Root AGENTS Code: “Every new or changed Rust item has rustdoc”
with Errors on fallible items and Panics where applicable; “Missing rustdoc
blocks merge.” `agent_docs/rust.md` Rustdoc explicitly includes fields,
private/test helpers and tests, and requires meaningful durable-operation
side effects, partial progress and retry documentation.

**Locations and source evidence.**

- `crates/wyrd/wyrd-storage/src/service.rs:861` (`download_url`) and `:1298`
  (`persist_completed_upload`) have materially changed signatures/bodies and
  no rustdoc. Both return Result. The latter now persists only coordination
  and manifest verification; its transaction ownership matters to callers.
- `crates/wyrd/wyrd-server/src/components/cards/service.rs:1974`
  (`reconcile_cleanup_claim`) adds a fallible inventory write but has no
  Errors section; `:2227` (`finish_card_delete`) has no rustdoc; `:2592`
  (`cleanup_card_blob`) and `:2775` (`write_card_blob`) have one-line docs
  without Errors. These are changed cleanup/durable-write boundaries, and
  write_card_blob now awaits the inventory after writing the envelope.
- `crates/wyrd/wyrd-server/src/components/storage/routes.rs:218` (`complete`)
  and `:431` (`download_init`) are materially changed fallible handlers with
  no Errors sections. Completion docs still describe only artifact metadata
  persistence rather than its additional inventory acknowledgment boundary.
- `crates/wyrd/wyrd-server/tests/integration/pg_card_declaration_route.rs:4978–4983`
  introduces six undocumented ArtifactPrompt fields. New helpers `send_json`
  (`:5074`), `complete_request` (`:5088`), `list_artifacts_request` (`:5104`)
  and `download_init_request` (`:5113`) call expect but lack Panics sections.
- `crates/wyrd/wyrd-server/src/scribe_outbox.rs:2488` (ObjectGate::ingest_frame)
  calls expect on release admission but lacks a Panics section.

**Consequence/materiality.** This independently violates a mandatory merge
gate. It also leaves the newly split persistence/ACK/cleanup failure boundary
undocumented where maintainers change its retry behavior. The examples are
all in changed items; unrelated old undocumented helpers are not included.

**Smallest correction.** Complete the rustdoc on these changed items, including
field purpose and real error/panic behavior; describe the relevant durable
side effects and uncertain/retry outcomes at the changed lifecycle boundaries.
No API, test behavior, new helper or documentation framework is needed.

**Closure proof.** Inspect the changed items and their docs against the rule;
retain format/lint validation. Batch all of these corrections together.

### REPO-002 — VIOLATION: fresh-install migration proof silently succeeds without Postgres

**Authority.** `crates/wyrd/wyrd-sql/AGENTS.md` Tests: SQL proof uses the
repository-managed Postgres task and “never silently skip because an
environment variable is absent.”

**Location/evidence.**
`crates/wyrd/wyrd-sql/tests/integration/pg_migration.rs:1494–1497` replaces
the former metadata RLS proof with `fresh_install_has_no_postgres_object_inventory`
but returns immediately when `database_url()` is None. That helper (`:2346`)
uses `ResolvedDsns::from_env().ok()`, so absent or invalid DSNs reach a green
test without migrations or any of the three catalog assertions executing.
The pre-existing module's description of optional environment cannot override
the directory instruction for this newly authored replacement proof.

**Consequence/materiality.** A named fresh-install proof can report success
without checking the removed table or retained coordination tables. This is
the exact explicitly forbidden SQL test behavior, even though properly
provisioned runs in the execution evidence passed.

**Smallest correction.** Make missing/invalid required database setup fail
this test explicitly, preserving the existing repository-managed Postgres
runner and assertions. Do not rewrite the whole migration-test environment
helper or adjacent unchanged tests to fix this one new proof.

**Closure proof.** Run the exact new test through the existing Postgres owner
wrapper and establish that its missing-environment branch fails rather than
returns green. No new harness is required.

### REPO-003 — VIOLATION: materially changed write/read handlers omit required tracing

**Authority.** Root AGENTS Code requires server handlers to carry
`#[tracing::instrument]` with scrubbed args; `agent_docs/rust.md` Diagnostics
and secrets requires instrumented write handlers with scrubbed state/body.

**Location/evidence.**
`crates/wyrd/wyrd-server/src/components/storage/routes.rs:218` (`complete`)
and `:431` (`download_init`) have only the utoipa path attribute. Neither
has instrument. The changed complete handler now owns the transfer-to-inventory
handoff and waits for the inventory result; download_init adds manifest and
inventory reads before issuing its plan. Instrumentation inside
`ObjectInventory::record/card_objects` covers the leaf operations, not the
handler's whole caller-to-result boundary.

**Consequence/materiality.** The explicit handler diagnostic gate is unmet
at the two changed HTTP operations. Their backend-completion/lookup failures
can occur outside the inventory span, so that span does not satisfy the
required handler-level trace boundary.

**Smallest correction.** Add normal scrubbed handler instrumentation to these
two changed handlers, omitting state, caller credentials, headers and bodies;
use safe structured operation/tenant identifiers under existing conventions.
No new telemetry framework or unrelated route sweep is warranted.

**Closure proof.** Source inspection of the attributes and scrubbing plus the
existing focused route checks; no new behavioral test is needed for attributes.

## Verification limits

Review-time V1–V4 tests passed, as recorded in `verification.md`; broader
checks are execution-evidence claims rather than fresh reruns by this role.
Actual production capability types were inspected, rather than inferred from
the passing tenant-isolation scanner. Pre-existing raw-pool administrative
test helpers were not treated as newly introduced production capability
escapes. No optional refactor or task-acceptance finding is proposed here.
