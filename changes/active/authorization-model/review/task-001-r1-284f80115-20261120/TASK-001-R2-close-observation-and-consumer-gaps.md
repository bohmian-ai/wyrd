---
id: TASK-001-R2
kind: remediation
status: proposed
spec: SPEC-authorization-model
spec_revision: 2
requirements: [REQ-002, REQ-004, REQ-006, REQ-008, REQ-010]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-2, FIND-TASK-001-8, FIND-TASK-001-11, FIND-TASK-001-12, FIND-TASK-001-13]
---

# Preserve observed authority and finish affected consumers

A caller must never use grants older than a permission change this process has already observed, including during its first tenant load. Administrators and approvers must also receive instructions and executable examples that match the revised authorization contract. Complete these bounded corrections and the missing CLI administration proof; preserve the eight closed R1 findings.

## Contract and immutable review context

Approved contract: `changes/active/authorization-model/spec.md`, revision 2. Original task: `tasks/TASK-001-scoped-authorization.md`. Prior remediation: `review/task-001-c78ad7b52-20261120/TASK-001-R1-scoped-authorization-corrections.md`.

Reviewed candidate: `284f801159a391e92493c3072878fd644c9d173b`; remediation base: `3139c08cb0b7c0194583a66af3ab31bef28dc14f`; original candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`; cumulative base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.

The authoritative diagnosed gaps and independent dispositions are in `findings-validation.md`, with focused source traces in `focused-validation.md` and `consumer-validation.md`. This task implements existing requirements and mandatory repository rules; it adds no product decisions.

## Correction boundaries

### Preserve observations across first loads and invalidation

**FIND-TASK-001-2, REQ-004.** `AuthorityCache::observe` only updates an installed entry. On a replica's first request for a tenant, a load can read revision N, receive a notification of N+1 before installation, discard that observation, then install N as usable. A malformed notification also cannot fence an in-flight load independently of an existing numeric revision fence. The production listener and request load run concurrently; this is a supported request path.

Correct the existing authority owner so observations survive absent entries and pending loads. Authority read before an observed change or unknown invalidation cannot become usable merely by finishing later. Acquire state proven current or refuse unavailable. Reuse the current cache/listener/reload mechanism; preserve the sanctioned miss/invalidation acquisition, short locks without IO, memory-only warm authorization, the five-minute unobserved-outage bound, and shared request/stream enforcement. No new policy service, schema, notification protocol or global outage is authorized.

This must fix the shared cache lifecycle at its root: observation, load, installation and confirmation must use one consistent authority-validity rule for cold and warm tenants. Replace the incomplete installed-entry-only invalidation handling and remove any state, branches, helpers or fallback paths superseded by that correction. Route affected request and stream consumers through the same owner; do not layer handler-specific checks or retain an obsolete alternate validity path. Preserve valid credential-settlement, ceiling and outage behavior. The implementation evidence must identify the replaced logic and its removal, or explain which existing logic remains necessary under the corrected invariant.

### Finish current account and grant guidance

**FIND-TASK-001-8, REQ-002/008/010 and docs/AGENTS.md.** `operate/add-people.md:9` still says passwords and invitations do not exist; its no-IdP section at line 182 recommends Service keys for people. Correct these affected passages to the existing local User/invitation and optional-provider flows. Do not invent enrollment behavior.

`operate/grant-and-inspect-permissions.md:46,60,78` introduces manually authored copyable SDK snippets. Reuse journey-executed files under `examples/` through the mandated `CodeFromFile` mechanism, showing the same current explicit-scope operations, or remove unnecessary duplicate snippets while retaining usable tested guidance. Do not require new examples where existing ones suffice. Do not introduce aliases, duplicate durable behavior, or redesign unrelated guides. Update generated served documentation through its owner when required.

### Align language approval journeys with parent visibility

**FIND-TASK-001-11, REQ-006/008/010.** Python `tests/integration/test_deferred_evidence.py:108-111,152-159` and TypeScript `wyrd/tests/integration/deferred-evidence.test.ts:134-135,148-153` create approver-only credentials and expect successful approval of Repository-owned work. The common approver role does not grant execution-parent read; the production checker correctly requires that read independently.

Correct these existing language consumers: demonstrate refusal before parent access, assign the required Repository visibility through public Principals, then approve and replay the same execution successfully. Preserve viewer/editor refusal and actor attribution. Reuse the existing Rust journey's approved behavior. Do not weaken production authorization or expand the approver bundle to make stale tests pass.

### Prove the shipped CLI administration adapter

**FIND-TASK-001-12, REQ-008/AC-003 and AGENTS.md Testing.** The new `wyrd-cli/src/principal/access.rs` command family lacks a real-server journey. Existing principal creation with `--grant` and principal revocation do not exercise access show/grant/revoke or the new role/group dispatch.

Add one focused journey in the existing CLI journey owner. An administrator assigns explicit family access, inspects it and removes it; a caller's covered read changes accordingly. A non-admin administration attempt refuses without mutation. Exercise the related shipped role/group adapter flow within that cohesive journey as needed. This is an evidence gap: production changes are needed only if the test demonstrates an actual defect. Do not add a separate top-level scenario per command.

### Use checked-in Card inputs in the new journeys

**FIND-TASK-001-13, TESTING.md checked-in-YAML rule.** The new scoped-editor Rust SDK journey at `principal_roles.rs:1328-1343` edits a Model's version in YAML and writes a temporary Card. The new public peer journey at `distributed.rs:2090-2099` edits a Service name in YAML. Replace these authored inputs with checked-in second-release/second-parent fixtures using the existing fixture owner. Preserve the real upload/completion/delete and actual peer/hidden-parent assertions. No generic fixture generator or production change is justified.

The same-family second-version peer variant is not a new requirement. Existing exact-versus-family assertions and the actual two-parent peer journey supply the composed proof retained by this review.

## Observable closure and verification

Use the repository-managed dependency setup from the named owning task. Confirm actual test selectors before execution and record commands/results against the next immutable candidate. Cargo-backed commands must run sequentially.

| Expected system behavior | Findings | Narrowest credible proof | R2 evidence |
| --- | --- | --- | --- |
| First load reads N; notification N+1 arrives before installation; N refuses and sufficiently current state serves. Unknown invalidation during a load cannot be cleared by that pre-invalidation load, without a prior numeric fence masking the case | 2 | Deterministic tests in `wyrd-auth/src/authority.rs`, exact `cargo nextest run --locked -p wyrd-auth --lib` selectors. Retain existing warm-entry observation and settled-token tests. Trace observation/load/install/confirmation and request/stream consumers to the shared validity rule; inspect the diff for removal of superseded invalidation logic. Connect changed acquisition to the existing real-replica request/stream proof if the corrected consumer path changes | PASS. `AuthorityCache` (`wyrd-auth/src/authority.rs`) now keeps one validity rule, `Entry::usable(generation)`: snapshot present, revision ≥ observed, confirmed under the current invalidation generation, within `FRESHNESS_BOUND`. `observe` records into an observed-only entry for cold tenants (replaces the installed-entry-only `get_mut`); a malformed notification advances the generation (replaces the loop clearing `confirmed_at` on present entries; `confirmed_at: Option` removed). `reload` and `confirm` capture `generation()` before their statement; `install(tenant, generation, loaded)` and confirm's unchanged branch confirm only under the captured generation. Pattern reused from `GatewaySnapshots` fill generation. Request (`authorize`/`acquire`) and stream (`StreamAuthority::admit`/`revalidate`) consumers still route only through `acquire`→`cached`→`usable`; their signatures are unchanged, so the real-replica proof's consumer path is unchanged. RED: `a_first_load_never_serves_after_a_newer_revision_is_observed` ("the cold load of 3 does not serve once 4 is observed") and `an_unknown_invalidation_refuses_a_load_begun_before_it` ("a load begun before the invalidation does not serve") failed on their assertions before the change. GREEN: C1, 11/11 incl. retained warm-entry, settled-token and Postgres outage tests. Waits: none added or moved; acquisition remains the sanctioned one-statement miss/invalidation reload, locks stay synchronous with no IO. Limit: observed-only entries persist per notifying tenant (`ponytail:` note). Commit `48788a3fc`. |
| Local users follow current enrollment guidance and retained SDK grant examples execute the same explicit scopes the guide describes | 8 | Content/source review of the affected passages and retained `CodeFromFile` inputs; applicable journey evidence for those example files; owning docs/example checks | PASS (content review + docs checks). `add-people.md` intro and *Without an identity provider* now describe `wyrd principal user invite` (one-time code, signup URL, chosen username/password) and `wyrd principal user create` (password on stdin), explicit-scope grants, and device-approved `wyrd auth login`; Service-key advice for people removed. No `examples/` file exercises `Principals`, so the three hand-written SDK snippets in `grant-and-inspect-permissions.md` were removed (allowed by this task) in favour of the journey-proven CLI commands plus a pointer to the same `Principals`/MCP calls. `llms-full.txt` regenerated through `docs:generate`. C6. `docs:build`/`docs:a11y` not run. Commit `b830cdeee`. |
| Approver without parent visibility is refused; after the explicit Repository grant the same execution can be approved and replayed with unchanged attribution | 11 | Exact existing Python and TypeScript deferred-evidence test selectors under their repository-managed integration setup; applicable language checks for changed files | PASS. Deviation: running the stated mechanics showed `approver` bound at `all` compiles to `cards:read`, `executions:read` and `verifier:approve` tenant-wide (access summary printed during diagnosis), so it reads the Repository parent and its approval is correctly allowed; the prior assertions were not stale for that binding. The required refusal exists for an approver bound only on the shared Verifier, which is what the Rust journey proves. Both journeys now create `approver` at `Verifier/signoff` through `Principals.create_service_principal`, assert `WYRD_PERMISSION_403_DENIED_RBAC`, bind `viewer` at `Repository/web` through `Principals.bind`, then approve, replay, pass and keep `actor_id`; viewer/editor refusals retained. C3 (Python 2/2 in file), C4 (TypeScript 2/2 in file); py lints/typecheck, ts format/lints/typecheck pass. No production change. Commit `300542877`. |
| CLI administrator grants, inspects and removes family access through the new command adapter; target visibility follows it; non-admin writes refuse without mutation | 12 | One exact real-server journey selector under `test:cli:journey` setup; include relevant role/group adapter coverage in the same flow | PASS. `card_lifecycle::pg_tests::administrators_manage_scoped_access_through_the_cli`: target created by CLI with no grant; its own `access grant` refuses `WYRD_PERMISSION_403_DENIED_RBAC` and `show` lists no grant; admin `grant viewer:Verifier/code-review` prints the exact binding line, `show` names it, the target's `latest` read of code-review succeeds within propagation while unrelated-check stays hidden; `put-role`, `create-group`, group `grant`, `add-member` give unrelated-check (show names `group:cli-readers`), `remove-member` removes it; `revoke` prints the removed binding and ends the read; `delete-group`, `delete-role`, `roles` confirm removal. Passed first run (evidence gap, no defect; no RED applicable). C5. Commit `300542877`. |
| Checked-in fixture Cards retain successful scoped declaration/upload/completion/deletion and real peer parent filtering | 13 | Rerun exact `principal_roles::scoped_editors_change_only_the_cards_they_cover` in `wyrd-ai --test integration -P journey --run-ignored=all` and `distributed::scoped_readers_query_only_their_parents_records_across_pods` in `wyrd-testing --test oracle -P journey --run-ignored=all`, using their owning Postgres setup | PASS. Checked-in `fixtures/cards/declare_and_pull/support-model-2.yaml`, `repository_declaration/code-review-2.yaml` and `scoped_peer_reads/other-reply.yaml` (+ its `no-refund-promise.yaml`) replace the runtime rewrites in `principal_roles::scoped_editors_change_only_the_cards_they_cover` and `distributed::scoped_readers_query_only_their_parents_records_across_pods`. The same TESTING.md rule was also broken by two branch-added rewrites in `principal_roles.rs` (`a_family_bound_viewer_queries_only_that_familys_records`, `group_custom_and_exact_version_bindings_scope_reads`); fixed with the same fixtures. `fixtures/README.md` principal_roles row updated; the extra file in `repository_declaration/` is inert to the copied-checkout journeys (policies select by path). C2a 3/3, C2b 1/1. Commit `07b3ab8b7`. |


R2 commands (sequential; Postgres via `scripts/postgres/with-test-postgres.sh`):

- C1: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-auth --lib -E 'test(authority::)'`
- C2a: `... cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=principal_roles::scoped_editors_change_only_the_cards_they_cover) | test(=principal_roles::group_custom_and_exact_version_bindings_scope_reads) | test(=principal_roles::a_family_bound_viewer_queries_only_that_familys_records)'`
- C2b: the finding-13 `distributed::` selector below, unchanged.
- C3: `mise run py:setup && mise run build:server`, then `... bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q tests/integration/test_deferred_evidence.py'`
- C4: `mise run ts:build && mise run ts:build:testing`, then `... bash -lc 'mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/deferred-evidence.test.ts'`
- C5: `... bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E "test(=card_lifecycle::pg_tests::administrators_manage_scoped_access_through_the_cli) | test(=card_lifecycle::pg_tests::a_family_bound_viewer_lists_only_that_family)"'`
- C6: `mise run docs:generate && mise run docs:test && mise run docs:check:commands && mise run docs:linkcheck`
- Static: `cargo fmt --all --check`; `cargo clippy --locked -p wyrd-cli -p wyrd-ai -p wyrd-testing -p wyrd-auth -p wyrd-server --all-features --all-targets -- -D warnings`; `mise run py:format py:lints py:typecheck`; `mise run ts:format:check ts:lints ts:typecheck`; `git diff --check`. All pass.

TDD: FIND-2 test-first (RED recorded). FIND-11/12/13 are consumer and proof corrections with no production change; FIND-8 is documentation. No TDD violation.

The existing finding-13 selectors run with repository-managed Postgres:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=principal_roles::scoped_editors_change_only_the_cards_they_cover)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=distributed::scoped_readers_query_only_their_parents_records_across_pods)'
```

Complete format/lints and the affected contract, dependency, tenancy or documentation checks required by actual edits. List any added/moved hot-path waits and match them to sanctioned permission acquisition. Do not substitute broad aggregates for focused closure proof or manufacture historical TDD evidence.

## Preserved behavior and exclusions

Retain User/Service principals, runtime family restriction, exact workload attribution and owning Cards, additive scoped roles/bindings, restrictive ceilings, accepted-work authority, tenant isolation, retained-parent visibility, provider trust and local no-IdP authentication. Preserve the eight closed findings and existing direct/peer enforcement. No new UI, onboarding system, migration, role semantics, access-request queue, compatibility surface or architecture revision is authorized.

Return implementation and focused verification evidence to the caller. Do not launch review, commit, push, or deploy.
