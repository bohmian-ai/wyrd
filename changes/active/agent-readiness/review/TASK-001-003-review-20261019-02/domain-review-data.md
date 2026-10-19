# Persistent declaration, replay and bundle integrity review

Result: **PASS within this domain**. Material proposed findings: **none**.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Complete reviewed range: `1b962bb0052e58c65af8c45be3ee0d1a04d483b6...d6b7c75b88db5eb144e6c282263f51692371bff1`.
- Remediation locator: `98d8ef46e61ea6aa5dae90d59572467a256a3ef3...d6b7c75b88db5eb144e6c282263f51692371bff1`.
- Approved specification: `changes/active/agent-readiness/spec.md`, revision 2; original TASK-001–003 and prior R1 task, verdict and validated ledger. TASK-004 and final integrated verification are excluded.
- Governing authority: `AGENTS.md` (ownership, SQL/storage, verification and documentation rules), `architecture/agent-rules.md`, `architecture/wyrd-design.md` (client model, reference binding, Registry lifecycle/idempotency and audit boundaries), `architecture/wyrd-doctrine.mdx`, and the routed architecture-constraints, patterns, spec-driven-development, maintainer-style, errors, telemetry-observations and analytical-operations-reliability references. The latter references inform cross-system acknowledgment distinctions; this change does not change Bifrost persistence.
- Spec obligations examined: REQ-003/004/009, INV-001–005, AC-003/005, and the domain portions of the original task acceptance criteria and R1 findings.

The approved pre-release change explicitly replaces the owning SQL and bundle formats in place. Historical migrations, mixed-version handling and old-format adapters are excluded rather than missing requirements.

## Source and consumer coverage

| Boundary | Producers, owners and consumers inspected | Assessment |
|---|---|---|
| Declaration wire values | `wyrd-spec/src/registry/{submission,enums,ids,mod}.rs`; shared client Cards, declaration saga, completion/receipt projections; server Cards mapping/routes/service; SDK native/public callers | Declaration identities and `declared` replace the corresponding registration values together. Pending/Active, idempotent no-op and deduplicated outcomes remain independent. |
| Persistent operation/schema | Migrations `20260601000001`, `000006`, `000014`, `000019`, `000020`; `wyrd-sql/src/queries/cards/{declare,lifecycle,version_resolve,mod}.rs` and references across `crates/`/`sdks/` | Table/column/FK/unique key/index/RLS and reconciliation default/projections agree. No stranded production reader or writer of the replaced SQL names was found. |
| Atomic declaration and replay | `service.rs::declare_card`, `replay`, `replay_operation`, `wait_for_replay`, `DeclarationWriter::write`, `persist_node`, `BindingProjector`, `replay_seed`; SQL reservation, insert and operation commit functions | Caller-owned transaction and stable seed identity remain. Exact references, principal projection, manifest and frozen binding publication retain their existing owners. |
| Post-commit lifecycle | `initialize_uploads`, `initialize_card_uploads`, manifest initialization, `complete_card_inner`, `commit_card_activation`, abort/cleanup and reconciliation callers; client saga completion | Storage calls remain outside the composite SQL transaction. Activation follows artifact verification and immutable Card blob persistence. Reconciliation and independently completed outcomes are retained. |
| Authorization audit projection | `wyrd-spec/src/vala/audit_detail.rs`; declaration/complete/delete route authorization and server audit calls; authenticated replay test retained-audit readback | New declaration operation/tag names retain their meaning; audit `Created` remains distinct. Route decisions still stage through the existing non-blocking audit owner; no SQL audit staging or second writer was added. |
| Graph publication | Cards handle; old `hydrate/` → `pull/{mod,graph,bundle,workspace}.rs`; `download.rs`, `download_progress.rs`; storage verified-download dispatch and both LocalFs/presigned transfer paths | Existing graph traversal, authenticated inventory/download and rollback-capable workspace are reused. R1 changes presentation only at the display construction point; verification, errors and publication order remain. |
| Runtime reader | `wyrd-spec/src/registry/bundle.rs`; `state.rs::BundleReader`, `WyrdState::from_path` and index assembly; Python state loading/hydration and pull wrapper; TS native/public pull/state projection; SDK graph journeys and fixtures | New discriminator/mode/DTOs agree. Metadata-only and retired-format refusal precede runtime admission; graph, inventory, path, size and digest checks remain before index publication. |
| Published deploy callers | Python/TS support-desk-app deploy functions, their imported Cards projections, Rust/canonical sibling deploy patterns, added caller regression tests | Neither caller deletes its current bundle before calling the shared replacement owner. Declaration and gateway policy sequencing are retained. |
| Recovery metadata | `wyrd-spec/src/error.rs` derive attributes at `:1144` and `:1188`; `state.rs::assert_pull_recovery`; generated error page/full reference | Both errors prescribe complete pull with an output directory and loading that directory. Source-derived projection is retained; codes/status/refusal paths are unchanged. |

Repository searches found no production references to `card_registration_operations`, `registration_operation_id`, `RECONCILE_KIND_REGISTRATION`, `CardRegistration`, `RegistrationReplaySeed`, `HydratedBundleManifest`, `HydrationMode`, or `card.registration` in the inspected Rust/SQL/SDK source set. Necessary runtime hydration terminology remains.

## Persistent-state and publication reasoning

`DeclarationWriter::write` (`service.rs:1078`) opens the tenant transaction, rechecks and locks exact Active dependencies, reserves the subject/actor/key identity, persists all ordered nodes, writes the replay seed, and commits once (`:1160–1161`). `persist_node` (`:1177`) composes Card, relationship, manifest, principal and verification-binding writes on that connection without ending it. A reservation loser drops its transaction before waiting for the winning replay. SQL reservation uniqueness still includes tenant, subject, actor and caller key; RLS remains the tenant boundary.

The stored object is explicitly `DeclarationReplaySeed`, not a final HTTP response. `replay_seed` (`:1530`) retains root, outcomes and manifest paths without upload URLs; SQL `commit_declaration_operation` serializes this type. The renamed serde outcome emits `declared`. `replay_operation` (`:674`) refuses expired/hash-conflicting operations, decodes that seed and changes only the replay outcome classification. Upload plans are reconstructed after commit, and completion consults actual durable Card/manifest state. No whole SQL/object-storage transaction or permanent presigned URL is implied. The SQL attribution test does not assert committed seed JSON; the serialization claim is source-backed, while the authenticated route test proves stable UID/no-op response behavior.

`CardGraphPuller::pull` (`pull/mod.rs:78`) resolves before creating staging, writes through `BundleWriter`, aborts staging on an ordinary error and calls publish only on success. The writer records local artifact paths only after `download_artifact` returns successfully and writes `metadata.yaml` after all Cards finish. The shared verified download checks both bytes and SHA-256; changing its display to `hidden()` does not skip transfer or error checks. Metadata mode omits bytes/local payload paths by design.

`BundleWorkspace::publish` (`workspace.rs:87`) moves a prior destination to its sibling backup, promotes staging, restores the backup after promotion failure and surfaces a rollback failure. Cleanup failure after successful promotion is logged. This is the existing ordinary failure/replacement guarantee. It does not establish crash-atomic replacement, fsync durability, continuous directory availability during the rename pair or coordination between concurrent destination writers; this task neither adds nor claims those guarantees. Cancellation during an awaited write can leave unpublished staging, as the owning orchestration documents; it does not replace the destination.

`WyrdState::from_path` constructs state only after header validation and complete index assembly. `BundleReader::load_index` (`state.rs:1277`) verifies every manifest Card, aliases, exact identity, declared reference/relationship closure and root reachability before assembling the immutable index. `validate_artifact` (`:1511`) requires a confined existing payload, nonnegative exact size and matching SHA-256. The new format header and metadata-only mode remain explicit refusal branches, with no old-format conversion.

## Prior finding closure and acceptance

| Obligation / prior finding | Current source and proof | Result |
|---|---|---|
| Declaration schema/replay/audit vocabulary agrees while identity/lifecycle semantics remain | Coordinated cumulative contract/SQL/service/route changes and the retained SQL/server tests described above; no R1 persistence changes | PASS |
| Complete pull verifies bytes and cannot publish an ordinary partial transfer as success | Existing verified downloader → staged writer → workspace publication chain; retained multi-Card CLI and SDK load stories; current terminal test corrupts real stored same-length bytes and asserts a typed refusal plus prior payload preservation | PASS |
| Metadata-only/retired bundles cannot enter runtime state | Shared BundleReader header checks and existing negative tests; CLI multi-Card producer output is passed to the real WyrdState reader | PASS |
| FIND-TASK-001-1 recovery points at an operation that can rebuild the bundle | Two derive attributes now prescribe `wyrd pull`/`--output-dir`; existing negative bundle tests additionally inspect the actual returned errors' remediation | CLOSED |
| FIND-TASK-003-2 caller erases safe replacement input | Python `deploy.py:24` and TS `deploy.ts:21` now call pull directly after declaration, with removal/import deleted; both wrappers reach shared Cards.pull; runtime-owned refused/repeat caller checks pass | CLOSED |
| FIND-TASK-002-2 preserves integrity while hiding terminal progress (domain portion) | `BundleWriter::write` changes only display construction; real terminal corruption test preserves the prior artifact and error code | CLOSED within data boundary |
| Reuse, no alternate loader/publisher/persistence mechanism | Base-to-candidate comparison retains the old saga/traversal/workspace/reader owners under replacement vocabulary | PASS |

The deploy tests intentionally replace declaration, pull and gateway calls. They prove exactly the caller regression: the old eager removal destroys the marker before a refused pull, whereas the fixed caller leaves it available; a succeeding replacement is reached on a repeat call. Their marker is not itself a runnable bundle, and their stand-in write does not prove atomic publication, integrity validation or rollback. That shared invariant is established separately by the source chain above, retained workspace rollback tests, retained real server/CLI/SDK graph load coverage, and the current terminal test's real artifact corruption/refusal assertions. Using caller-isolation proof here does not mock away the required shared owner or purport to replace those tests.

## Verification and limits

Fresh focused execution in this review:

- From `sdks/wyrd-sdk-ts/wyrd`: `mise exec -- pnpm exec vitest run tests/unit/support-desk-deploy.test.ts` — **1 passed**.
- From the repository root: `mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_support_desk.py::test_quickstart_deploy_keeps_the_prior_bundle_until_pull_replaces_it` — **1 passed**.

Original-task SQL/server/three-SDK proof and R1 bundle-negative/terminal corruption/quickstart proof were inspected as reported evidence and matched to their current source, not independently rerun. No Cargo, database, broad suite, crash/power-loss, cloud-backend or additional-platform run was performed. This domain PASS does not close the separate supported-host or released-artifact evidence obligations, nor constitute integrated change acceptance.

No material correction is requested. HEAD remained `d6b7c75b88db5eb144e6c282263f51692371bff1`; this review changed only this assigned report and preserved other review artifacts.
