# Focused follow-up: OTLP publisher identity versus replay

Result: **RESOLVED**. Source supports one shared proposal behind SEC-09,
OTEL-9, and DUR-R2-001: Gate's implicit OTLP identity does not distinguish the
verified current actor. Independent final validation still owns acceptance and
stable finding IDs. No intended verdict was supplied or presumed.

Subject: `dac4c340f835676adc29f3dcc01f98e1d3d51465..d088519ea846aba8b2a77c24b2af2491ad2b3936`.
HEAD matched the pinned candidate during source inspection. The authorized main
merge is included at the telemetry/identity seam. This follow-up does not audit
unrelated imported-main capabilities. Only this report is written.

## Authority resolves the semantic question

Approved spec REQ-011 requires generic telemetry to retain authenticated
publisher identity; INV-001 makes authentication select tenant/publisher;
REQ-017 permits deliberate identity selection through an explicit shared client.
These provisions alone do not define all duplicate-delivery semantics. The
stronger current authority does define publisher precisely:

- `architecture/wyrd-design.md:888-905`: every accepted row carries authenticated
  publisher identity; `sub_principal_id` is for whom work was done and
  `act_principal_id` is who emitted it, derived from the verified JWT per request.
- `architecture/bifrost-design.md:86-91`: direct publishers occupy both fields;
  delegated publishers retain subject and themselves as actor. These fields do
  **not** participate in row identity.
- `architecture/references/domain/telemetry-observations.md:8-35,103-106`:
  authenticated subject/actor are the publisher spine; duplicate transport
  attempts converge to one logical row set or a typed identity conflict.
- `architecture/wyrd-security-posture.md:278-288`: delegation retains subject,
  records the outer actor, and intersects authority; actor is attribution only.

There is no authority requiring another row for every HTTP/gRPC request or
credential refresh. A retry under the **same subject/current-actor pair** must
converge despite changed request ID, admission time, token expiry/refresh, or
older delegation history. Nor is there authority to change native caller-owned
batch-ID semantics: Bifrost design `:102-123` explicitly gives native clients the
batch identity and requires preserving it on retry.

OTLP supplies no native `wyrd_batch_id`. Its existing owner already defines a
narrower derived-ID contract: `gate/mod.rs:105-123,934-945` says distinct
authenticated publishers receive their own fences and attributed rows, and
`:2888-2898` states the same property in its existing regression test. This
resolves the possible interpretation that arbitrary re-publication by another
actor intentionally retains the first publisher. The managed subject/actor
pair now defines that publisher. Applying the existing derived-ID separation to
that verified pair preserves the current OTLP contract; it does not introduce
row identity or require a new public API. The comments/tests corroborate this
reading of architecture; agreement among discovery reviewers is not evidence.

## Public reachability and source-to-fence trace

1. `wyrd-client/src/client.rs:380-412::on_behalf_of` is a real public operation:
   actor client exchanges an inbound subject token and retains the resulting
   shared auth path. `wyrd-auth/src/exchange_api_key.rs:267-315,362-389` verifies
   both tokens, refuses same-principal/unbound/delegated actors, and issues the
   subject with current actor and intersected permissions. A Card-bound actor
   holding record-write authority and a subject holding that authority can
   therefore reach the positive ingest path. Existing public delegation journey
   `tests/bifrost/server/query.rs:1225-1314` proves the same exchange mechanics;
   its particular subject lacks write permission, so that fixture's refused
   write is not evidence that delegated OTLP writes are impossible.
2. `wyrd-auth-verify/src/lib.rs:339-369,767-791` verifies signature/issuer/audience/
   tenant and reconstructs principal plus chain. HTTP middleware
   `http/middleware/authenticate.rs:64-121` authenticates OTLP bearer requests on
   the **Wyrd** audience; `Caller::from_authenticated` retains that chain
   (`components/auth/caller_extractor.rs:47-66`), and HTTP OTLP's
   `caller_auth_context` copies it (`http/otlp.rs:531-537`). gRPC's authentication
   owner (`grpc/otlp.rs:265-288`, `gate/auth.rs:147-156`) accepts Bifrost-surface
   tokens; verifier validation also accepts Wyrd tokens on this surface.
   Use Wyrd-audience delegated tokens for one test exercising both transports.
3. `wyrd-runtime/src/request_context.rs:53-65` and `gate/auth.rs:79-85` map direct
   subject S to `(S,S)` and an authorized actor A for S to `(S,A)`. Full chain
   history is not the current publisher pair and must not become a new batch-ID
   input.
4. All three signal entry points authorize record writes before dispatch
   (`gate/mod.rs:827,866,905`), then call the same `dispatch_canonical`
   (`:842,881,920`). Send the **identical valid batch**, including trace/span
   identifiers, timestamps and optional correlation, as S and then A-for-S.
   This is an explicit re-publication case, not a claim that two independently
   generated framework executions naturally share trace/span IDs. Public
   exporters/collectors can submit retained content using deliberately selected
   clients. Neither transport forbids it.
5. `tables/signal.rs:642-655` appends only Card/Run correlation to the canonical
   pre-stamp rows. `dispatch_canonical:975-988` hashes tenant, subject ID, table,
   logical digest and correlation digest, but separately sends the full pair to
   Scribe. `correlation_data_identity` (`scribe/preprocess.rs:410-437`) can hash
   publisher columns only if present; they are not present at this producer.
   Hence `(S,S)`, `(S,A)`, and `(S,B)` derive the same ID for these same rows.
6. Scribe ingress passes the pair to decode (`scribe/ingress.rs:338-365`);
   `execution_lanes.rs:1084-1110,1228-1267` stamps both verified values.
   `logical_data_identity` (`preprocess.rs:553-565`) deliberately excludes
   **all** managed columns. Actor therefore does not enter the logical WAL or
   durable digest indirectly. Schema, row ordering and user content are equal.
7. Active/immutable dedup (`scribe/shards.rs:3814-3826`,
   `memtable.rs:163-181,346-401`) matches batch ID plus schema/logical slice
   facts and returns the retained rows without inserting again. After retained
   memory retires, durable SQL uses the same logical slice-set digest and count
   (`scribe_batch_commits.rs:92-103,228-229`); Scribe treats AlreadyCommitted as
   spent (`shards.rs:3643-3656,3751-3759`) and suppresses insertion. This case is
   neither a schema conflict nor a different-user-payload conflict.

Observable result: second publisher's valid export can succeed while only the
first publisher's pair remains in retained telemetry. No cross-tenant access,
permission escalation, or credential disclosure is established or required for
this proposal.

## Sibling identity owners

| Writer / consumer | Identity owner and actual behavior | Correction scope |
|---|---|---|
| OTLP traces, logs, metrics over HTTP and gRPC | Shared Gate implicit ID before server stamps; only subject bound | One Gate correction covers every signal/transport |
| Native Bifrost inserts | Explicit caller-generated batch ID at `gate/mod.rs:1058-1085`; same downstream stamps and fences | Preserve native retry semantics; do not add actor to Scribe logical identity |
| Server-internal Scribe outbox | `wyrd-server/src/scribe_outbox.rs:787-804::identify` hashes tenant, destination, **both pair members**, optional request and rows | Already owns correct pair separation; reuse the value type/convention, not a second service |
| Active/immutable memory and durable SQL fence | Existing batch ID plus stable logical facts; managed stamps deliberately excluded | Correct implicit ID at producer; no downstream attribution guard or fence redesign |
| Existing OTLP attribution journey | `negative.rs:385-481` explicitly tests same content/different publisher and same publisher replay, but reads old `principal_id` at `:433,440` | Update existing readback to subject/actor pair as part of proof closure; no new harness |

## Smallest correction and focused proof

Retain the three proposals as one source-local **REGRESSION / INCORRECT**
proposal at `gate/mod.rs::otlp_batch_id` and its `dispatch_canonical` call.
Use the already verified `auth.attribution()` and existing `Attribution` value
so the deterministic ID binds subject **and current actor**. Keep tenant, table,
logical content and accepted Card/Run correlation inputs and exclude request ID,
admission time, credentials and historical actors. Do not change Scribe's
logical digest, native explicit-ID semantics, outbox grouping, authorization,
provider integration, or transport APIs.

The existing unit test is
`gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload`.
Extend it to distinguish `(S,S)`, `(S,A)`, `(S,B)` while repeated equal pairs
remain stable and its current divergence checks remain intact. Exact command:

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload)'
```

Extend the existing
`negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution`
journey, using `OtlpJourney::client_with_permissions`, `.token()`, `.server()`,
public `on_behalf_of`, `export_traces_over_grpc_as`, and HTTP `post_otlp_raw`.
A writer actor obtained with existing service bootstrap is Card-bound. Exchange
for Wyrd audience; send identical content directly and delegated, replay each
pair, publish and read the current `sub_principal_id`/`act_principal_id` columns.
Require each accepted pair exactly once while preserving existing independent
subject and Run checks. The old `principal_id` query is obsolete after the
merge; a failure there must be corrected in this same existing proof, not
skipped. A separate framework or per-signal test is unnecessary for this shared
producer. Exact existing fixture command, matching the owner mise lane with no
extra features or setup beyond managed Postgres/migration:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'
```

## Verification limits

Source/value tracing establishes the collision and reachable authorized path.
This reviewer ran no test, build, install, or isolated probe. Root is executing
the existing exact journey independently; consult current `verification.md`
for its result. No new API, test harness, requirement, or guarantee is needed.
No source/spec/task/index/HEAD edits were made, and other reviewers' files were
preserved. The remaining decision is independent final validation of this
bounded proposal, not unresolved publisher/retry semantics.
