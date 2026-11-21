# Authority lifecycle review

## Subject and boundary

Base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
Candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
The implementation source at worktree HEAD `3139c08cb0b7c0194583a66af3ab31bef28dc14f` is unchanged from the candidate. Later authority/design/spec commits are excluded from the implementation diff. Authority is SPEC-authorization-model revision 2, primarily REQ-004, REQ-005 and REQ-010, with the current-authority and freshness section of `architecture/wyrd-security-posture.md`.

Scope: current-authority population, invalidation, confirmation, account/credential mutation, stream output, recovery, and added waits. This is independent discovery; proposed findings below require separate validation.

## Coverage and evidence

- `AuthorityCache::cached` checks confirmation age and invalidation under a short mutex; `acquire` reads the cache before awaiting. Its guard never spans database IO. `TenantAuthority::compile` resolves direct/group bindings once into shared account grants; ordinary containment is synchronous and store-free.
- `wyrd-sql/src/queries/auth/role_bindings.rs:59` advances the tenant revision and emits `pg_notify` in the caller's transaction. Principal access mutations route through `components/principals/access.rs:58` before committing; provider replacement advances only on changed assignments in `wyrd-auth/src/callback.rs`. Account insertion, deletion, suspension, workload-restriction replacement, and key insertion/revocation advance through their SQL owners. The separate bootstrap, invitation, and platform-provisioning owners use the same scoped-binding/revision mechanism.
- `components/auth/state.rs:104` runs notification watching concurrently with two-second revision confirmation. `boot/mod.rs:1485` starts this worker on API-serving production targets. The listener-reset callback deliberately does nothing; the five-minute fallback is governed by successful background confirmation, rather than listener connectivity alone.
- `pg_principal_roles::access_changes_reach_another_replica_within_the_bound`, at `tests/integration/pg_principal_roles.rs:1147`, starts two actual in-process servers over the same repository PostgreSQL. Public binding creation and deletion reach an existing Service bearer on the second replica within the five-second helper bound. Twenty successful warm requests preserve the compile counter. This proves the normal Service-key path, not the refreshed human-token path in INV-002.
- `authority::pg_tests::an_outage_serves_only_within_the_bound_then_refuses_as_unavailable` closes the real application pool and ages a populated snapshot around the five-minute boundary. The unit tests also exercise cold state, equal/newer notifications, expiration, and request reuse. They do not interleave notification delivery with an in-flight load or confirmation.
- `TenantAuthority::authorize` applies signed and stored key ceilings and each live delegation actor's current grants. `StreamAuthority` reacquires through the same cache before each output and compares the subject, actors, and stored credential ceiling after a snapshot replacement. `query/service.rs:237` wraps ordinary streams, while accepted work retains its frozen authority.
- `principal_roles::an_open_stream_stops_once_its_grant_is_removed`, at `sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs:177`, drives the real SDK stream, removes its workload binding, and asserts a failed stream with fewer than all rows and no successful terminal. The recorded result is 21/21 for the focused principal/query stories.
- `pg_verification_runtime::accepted_authority_survives_account_and_context_changes`, at `tests/integration/pg_verification_runtime.rs:5901`, admits delegated work, retries it, stops the runtime, changes the actor's role/restriction/key, restarts, and checks byte-identical accepted authority and original attribution. This is relevant recovery proof; it does not repair current-request invalidation races.

The three exact existing unit tests `authority::tests::snapshots_serve_only_while_fresh_and_current`, `authority::tests::requests_never_extend_an_unconfirmed_snapshot`, and `authority::tests::a_stream_keeps_only_unchanged_authority` were rerun through `mise exec -- cargo nextest run --locked -p wyrd-auth --lib`, with an expression selecting those exact names: 3/3 PASS, default profile, run ID `199d0eea-aafa-4312-a942-1ec910840ffa`. No candidate source was modified to construct race regressions.

## Proposed finding INV-001 — A newer observed revision can be forgotten

Classification: `INCORRECT`.

Requirement: REQ-004 says that a process observing newer state must refresh before allowing, and known invalidation must refuse unavailable state rather than use obsolete grants. `architecture/wyrd-security-posture.md:685` states the same requirement.

Locations: `crates/wyrd/wyrd-auth/src/authority.rs:543` (`install`), `:568` (`notify`), and `:614` (`confirm`'s changed-revision branch); affected consumers are `reload` at `:491`, `authorize` at `:523`, and stream `revalidate` at `:698`.

Starting state and trigger: a process holds revision N with a grant to Alice. A foreground reload or background reload reads revision N. An administrator commits removal at N+1, and this process receives the N+1 notification before the older load installs. This is a supported concurrent access change, not a forged notification.

Expected: once N+1 is observed, revision N cannot become usable again. A load that does not reach N+1 must leave the entry invalid and cause acquisition to reload or refuse unavailable.

Actual: `notify` retains only `known_newer = true`, not the observed revision. `install` compares the loaded revision only against the installed snapshot's revision. Since N is not less than N, it replaces the entry with revision N and `known_newer = false`, renews confirmation time, and returns it. Subsequent requests and stream frames can use a grant already known to be revoked.

The exact public-method sequence is `install(N)`, `notify(N+1)`, `install(N)`, `cached(tenant)`. The last operation returns a snapshot despite the unmet N+1 observation. The awaited load/commit between `reload:498` and `install:506` makes that sequence possible in the real request path.

There is a second manifestation of the same lost-observation cause: when LISTEN missed a change, `confirm:604` can successfully read N+1 while N is installed. Its `Ok(_)` branch does not mark the entry invalid before awaiting the full load. Other requests continue using N during that load; if it fails, `:621` only logs, leaving N usable until its old freshness deadline. Observing N+1 is already sufficient to prohibit that fallback.

Consequence: revoked resource reads or output can resume after this process has learned the revocation. The next healthy confirmation often repairs it, but the requirement explicitly forbids this known-stale interval, and a following outage can preserve it until the freshness bound.

Smallest correction boundary: keep the highest observed revision at `AuthorityCache`, including observations made by confirmation, and make installation/acquisition preserve invalidation until a loaded snapshot reaches it. Retain the existing cache, one-statement reload, and per-account compilation; do not add a second resolver or request-time permission query. Add deterministic notification/load interleaving tests, including changed-revision confirmation followed by failed reload, and a focused server refusal case for the corresponding access removal.

## Proposed finding INV-002 — A refreshed human bearer repeatedly reloads warm authority

Classification: `VIOLATION`.

Requirement: REQ-004 prohibits permission SQL and grant compilation merely because a warm request arrives. The same requirement permits a previously confirmed usable snapshot to serve during confirmation failure until its freshness deadline. REQ-002 preserves rotating CLI refresh tokens.

Locations: `crates/wyrd/wyrd-auth/src/authority.rs:402` (`predates`), `:529` (`authorize`), and `:543` (`install`); credential producer `crates/wyrd/wyrd-auth/src/issuance.rs:224` and refresh caller `crates/wyrd/wyrd-auth/src/refresh.rs:230`. The snapshot credential source is `crates/wyrd/wyrd-sql/src/queries/auth/authority.rs:44`, which loads API keys only.

Starting state and trigger: a human CLI session rotates its refresh token and immediately issues several ordinary reads. The returned bearer names the consumed refresh row as its `cid`. That UUID is correctly absent from the snapshot's API-key map. The first cache reload completes in the same whole second as the token's `iat`, a normal low-latency request sequence.

Expected: after successful current-state acquisition, repeated warm requests use memory. The consumed refresh row must not be treated as a newly inserted API key on every request. An actual newly created API key must still receive its current restrictive ceiling before authorization.

Actual: `predates` tests `verified.iat.timestamp() >= loaded_at.timestamp()` and an unknown credential. A reload in that second leaves the comparison equal and the refresh UUID absent. The next request therefore calls `reload` again, loading and compiling the entire tenant authority. Every request in the issuance second repeats this work. If PostgreSQL becomes unreachable after an already successful reload in that second, the next request attempts another reload and refuses unavailable even though a current, confirmed snapshot is present.

Evidence: `TenantGrant::Refresh { consumed }` returns `Some(consumed)` from `credential_id`, and `RefreshTokens::execute` passes the consumed row into human issuance. `TenantAuthority::compile` never inserts refresh rows into `keys`. `authority::tests::an_unknown_credential_reloads_only_for_a_newer_token` tests an unknown credential five seconds before `loaded_at`, but does not test equality after a successful acquisition; the replica warm test uses a stored Service API key and cannot expose this path.

Consequence: an ordinary refreshed CLI session puts database waits and full tenant grant compilation back onto otherwise warm direct-verify, gateway, query, or read requests during this normal interval. This also weakens the intended bearer outage behavior. No high traffic assumption is needed: two reads issued within the same second are sufficient.

Smallest correction boundary: make the cache's credential-miss acquisition establish whether the missing credential is an API key without repeating that acquisition for the same already-confirmed token/state. Preserve credential provenance, signed/stored ceiling checks, and the real new-key race handling. A focused local CLI refresh journey should perform repeated reads immediately after rotation and assert no additional authority compilation/permission reads after acquisition; cover continued use of that confirmed snapshot during database confirmation failure. Add an in-owner deterministic same-second regression rather than a timing-sensitive sleep test.

## Wait inventory

| Added or moved wait | Sanctioned boundary | Assessment |
| --- | --- | --- |
| Bearer cache miss, observed invalidation, or expired confirmation: `AuthorityCache::reload` | Permission-state reload | Permitted; INV-001 concerns the freshness state around that reload. |
| Token naming potentially newer account/credential: `AuthorityCache::authorize` → `reload` | Permission-state cache miss | The genuine miss is permitted; INV-002 shows repeated reload of already-confirmed state. |
| Ordinary stream frame: `StreamAuthority::revalidate` → `acquire` | Memory normally; permission-state reload on invalidation/expiry | Permitted in shape; affected by INV-001. |
| Two-second background revision confirmation and changed-state load | Background control-plane work | Not a request wait. No mutex is held across either await. |
| Audit staging | Existing process Scribe outbox | No added acknowledgement or persistence wait in this boundary. |

No added object-storage or Vault wait was found in this boundary. Frozen accepted work does not reacquire current request authority; its persisted ceiling and attribution remain the execution authority.

Live PostgreSQL reruns could not start because repository setup could not access the Docker API socket. The task's recorded real-dependency results and their actual source assertions were inspected, but were not independently rerun here. The proposed findings are source-backed concrete paths, not claims of an observed live exploit. No implementation, Git configuration, commit, or push was performed.
