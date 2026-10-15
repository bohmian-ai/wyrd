---
id: TASK-001-R3
kind: remediation
status: proposed
spec: SPEC-agents-trace-processor
spec_revision: 2
parent_task: TASK-001
requirements: [REQ-011, REQ-017, INV-001, INV-002, AC-003, AC-010, AC-013]
remediates:
  - FIND-TASK-001-9
  - FIND-TASK-001-10
  - FIND-TASK-001-11
  - FIND-TASK-001-12
depends_on: []
---

# Preserve OTLP publisher identity and close affected proof contracts

Route this single bounded task directly to `$wyrd-implement` under approved
specification r2. Correct the existing owners and their proof; no new spec or
plan approval is needed. This packet does not authorize unrelated main changes,
a new telemetry protocol or redesign of native retry semantics.

## Authority and reviewed subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.

- Approved [specification r2](../../spec.md) and
  [original TASK-001](../../tasks/TASK-001-framework-agnostic-telemetry.md).
- Original base: `dac4c340f835676adc29f3dcc01f98e1d3d51465`.
- First candidate: `692d390dc40e43c6e1d7e25c7850d5c826d26a9e`.
- Previous candidate: `295446005b18604030bf376de6209098a28886f7`.
- Reviewed current candidate: `d088519ea846aba8b2a77c24b2af2491ad2b3936`.
- Authorized main merge: `1b769759dfd92a82e60082fe80460166bdd860f8`, importing
  `cb29781ab6f633700f9e47befddb1f325fd8b732`.
- [R1 task/evidence](../TASK-001-r2-692d390dc-20261015/TASK-001-R1-telemetry-contract-closure.md),
  [R2 task/evidence](../TASK-001-R1-295446005-20261015/TASK-001-R2-python-overlap-and-docs.md).
- Current [verdict](verdict.md), [independent validation](findings-validation.md),
  [focused follow-up](followup-review.md) and [fresh verification](verification.md).

Read AGENTS.md, agent rules, active Wyrd/Bifrost design and doctrine, relevant
security/telemetry/runtime/language/testing authority and TESTING.md. The task
follows existing authenticated publisher and OTLP derived-identity contracts;
reviewer findings do not add product authority. Findings 1–8 remain closed.

## FIND-TASK-001-9 — OTLP replay identity omits the current actor

**Obligations:** REQ-011 and INV-001, current Observation publisher identity and
Gate's existing distinct-publisher derived-batch contract.

In `crates/vala/vala-bifrost-redux/src/gate/mod.rs:131-146`, `otlp_batch_id`
hashes tenant, one principal, table and content/correlation identities. Shared
`dispatch_canonical:975-988` supplies `auth.principal.id` (subject) to that
identity but forwards the complete verified `auth.attribution()` to Scribe.
After the main merge, the current actor may differ from the subject. The
pre-stamp signal schema contains Card/Run, not publisher columns, so neither
input digest indirectly includes that actor.

Two accepted exports of identical retained OTLP content for subject S, first
directly `(S,S)` and then by authorized actor A `(S,A)`, therefore derive the
same batch identity. This is deliberate re-publication of the same content,
including span IDs/timestamps, not two independent framework turns happening
to have identical IDs. Existing public `on_behalf_of` exchange can supply this
identity, with intersected write permission and the correct transport audience.
Scribe stamps both publisher fields afterward, but excludes managed fields
from logical replay identity. Its memory/durable fences suppress the second
insertion as already committed. The second export can succeed while its
actor's attribution is absent. There is no cross-tenant access claim.

Current stronger authority defines publisher as verified subject/current actor;
the existing OTLP derived-ID contract separates distinct publishers. Same-pair
retries remain duplicates, and native caller-supplied batch identities retain
first-acceptance behavior. No material new concurrency or durable contract is
needed. Normal current framework journeys use direct publishers and fresh
span IDs, so their passing results do not exercise this collision. Source and
independent follow-up establish the collision; the red Rust journey below
proves obsolete readback, not a dynamically executed delegated collision.

**Selected correction:** extend the existing Gate-owned implicit OTLP batch
identity to bind the already verified `Attribution` pair, and use that same
trusted value at shared canonical dispatch. Preserve tenant/table/logical
content/accepted Card/Run inputs and exclude request ID, wall clock, refreshed
token bytes and historical delegation-chain members. All three signal types
and HTTP/gRPC share this producer. Fix it once there; no downstream actor guard,
client publisher field, identity resolver, schema column, queue, registry,
exporter/auth adapter or new replay owner is required. Existing internal outbox
identity already hashes both pair members; its ownership need not change.
Do not change Scribe's managed-column exclusion, durable fence contract or
native explicit batch-ID semantics.

**Behavioral TDD:** first extend the existing derived-ID regression to hold
tenant/subject/content/correlation constant and distinguish `(S,S)`, `(S,A)` and
`(S,B)` while repeated identical pairs converge. Confirm RED for actor-only
variation, apply the minimum producer correction, then GREEN with existing
identity-divergence cases retained. Extend the existing real OTLP attribution
journey after its readback is migrated below: export identical content under
direct and delegated pairs, repeat each and query both stamps. Each publisher
pair must retain its rows exactly once, with existing independent-subject and
Run checks intact. Use existing OtlpJourney/service/delegation/export helpers;
no new harness. A Wyrd-audience exchanged token is accepted by the HTTP OTLP
middleware and by gRPC; do not mistake a wrong audience or missing permission
for the identity RED. Exercise both HTTP and gRPC using those existing helpers,
with each pair retained once and same-pair retries converging across transports.
Follow RED–GREEN–REFACTOR with exact commands below.

## FIND-TASK-001-12 — existing OTLP readback uses the removed publisher column

**Obligations:** existing Rust OTLP preservation, AC-003 and repository red-gate
completion rule. This proof-consumer regression is separate from FIND-9's
production identity cause even though the same journey can close both.

`crates/wyrd/wyrd-testing/tests/bifrost/otlp/negative.rs:433,440` still selects
and extracts `principal_id`; its nearby rustdoc at `:396` uses that obsolete
name. Current signal schema has `sub_principal_id` and `act_principal_id`.
Root ran the exact existing journey twice with tracing; both selected one and
failed when authenticated Oracle planning rejected that query. Debug logs show
normal canonical ingest, Scribe commits and publication before the schema
refusal. Adjusting timing, provider flushing or permissions cannot repair it.

**Selected correction:** migrate this existing journey's query, column
extraction, publisher assertions and explanatory rustdoc to the current pair.
Retain exact subject, actor, Run, row-count and replay assertions. Direct
publishers occupy both columns; the delegated extension for FIND-9 must assert
distinct current actors for the same subject. Do not add a compatibility alias,
catch/ignore the query error, remove assertions or weaken the schema.
Independent validation swept all seven Rust OTLP files: these are the only
obsolete publisher reads; generic query forwarding and other signal journeys
need no redesign. Revalidate that bounded sibling scope before editing, fixing
all same-cause reads within it together.

**Proof:** the current exact traced failure supplies RED for the readback
migration. Make that journey pass using current schema while retaining replay
semantics, then prove FIND-9's actor-only scenario through it. Record those
causes separately. Existing unit and journey owners are shared, so one focused
run after the completed correction can supply both relevant GREEN assertions.

## FIND-TASK-001-11 — missing-credential proof matches text instead of code

**Obligations:** REQ-017 / AC-013 and TESTING.md's exact catalog-code rule for
client-facing errors.

`sdks/wyrd-sdk-python/tests/integration/otel/frameworks/test_framework_telemetry.py:280-298`
removes ambient credentials, launches the real application and checks stderr
for `WyrdError: no credentials available; set WYRD_ACCESS_TOKEN`. The comment
names `WYRD_CLIENT_401_NO_CREDENTIALS` but the assertion never inspects it.
The child exception has a structured code that is lost when observation is
reduced to a traceback. A wrong public code with unchanged text could pass;
correct behavior with revised human wording could fail. The fresh passing
journey therefore does not prove the required stable-code boundary. No
production credential-resolution failure has been alleged.

**Selected correction:** keep the real isolated child process and public
ambient client/state path. Expose the caught public WyrdError's structured code
from that negative scenario and require exactly
`WYRD_CLIENT_401_NO_CREDENTIALS` in the parent. Preserve the successful real
gateway/export/readback assertions and upstream-key isolation. Do not parse
traceback text, accept a set of codes, mock credential resolution, add a new
authentication helper, or replace the real negative path with a unit-only test.
Existing application/test communication is sufficient; local output formatting
is implementation-owned.

**Proof:** run the existing exact ambient gateway selector below. Verify its
negative assertion observes the actual public code and rejects a different
code by construction, independent of human message wording. This is correction
of test evidence, not a request to fabricate a production RED or add a second
assertion-mirroring test harness. Record the real code and passing gateway
result; preserve isolated test configuration homes and keys.

## FIND-TASK-001-10 — required panic documentation is absent

**Obligation:** AGENTS.md §16's explicit rustdoc requirement includes Rust tests
and requires `# Panics` when a panic remains possible.

The new shared-client regression
`crates/shared/wyrd-client/src/state.rs:3953-3988`,
`state::tests::telemetry_flush_failure_settles_with_the_native_drain`, has
substantive intent prose but lacks its mandatory panic section. It calls
`expect_err` and explicitly panics for unexpected catalog variants, alongside
assertions. This is a required documentation completion gap; it does not imply
an additional production logic defect or justify a style-only cleanup pass.

**Selected correction/proof:** add a concise `# Panics` section describing
unexpected settlement results, error variants and failing assertions. Keep
code and assertions intact. Review it against the actual body and run formatting;
no manufactured RED, new behavior or extra test is necessary for rustdoc alone.
Retain this existing regression as proof of shared error precedence. Do not
rewrite surrounding docs or change its imports solely for style.

## Acceptance and preserved behavior

| Finding | Required observable closure |
|---|---|
| FIND-TASK-001-9 | Verified actor changes distinguish implicit OTLP publisher batches; identical pair retries converge; actual delegated/direct readback retains each pair once. |
| FIND-TASK-001-12 | Existing attribution journey no longer queries/extracts the removed column and passes with exact pair/Run/replay assertions; all same-cause reads in its bounded OTLP sibling scope are gone. |
| FIND-TASK-001-11 | Real ambient missing-credential scenario asserts exactly the public catalog code, independent of message text, while existing gateway assertions pass. |
| FIND-TASK-001-10 | New settlement regression has its required substantive panic contract with assertions unchanged. |

Keep all earlier finding closures and approved ergonomics: framework-agnostic
stock OTEL export, actual Python three-framework/TS OpenAI support, optional
framework dependencies and generic TS types, Run/Card scope and allowlisted
mapping, payload-free Responses counters, one managed provider, GIL-aware
same-state setup/retry, bounded current-call flush and native drain/error
precedence, active/global provider continuation, fresh ambient credentials and
separate model keys. Keep tenant/Card authorization and server-owned publisher
stamps. No new package, dependency/Cargo feature, schema, SDK API/error,
permission, transport, native batch retry contract or universal redaction claim.
No docsite rewrite is required; preserve the closed installation/procedure fix.

## Focused verification and evidence

Read mise.toml for setup/feature ownership. Run exact existing selectors; require
positive counts, never positional zero-selection proof. Commands are prospective
closure requirements, not passes claimed by this review.

Rebuild normal server/native test artifacts after the Gate source changes;
in particular run `mise run py:setup` before the Python server-backed selector.
Do not accept proof from a stale native server build. Cargo work stays sequential.

```sh
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=gate::otlp_batch_id_tests::derived_identity_is_stable_per_tenant_table_and_payload)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_LOG=info,vala_bifrost_redux=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::identical_exports_from_two_principals_each_keep_their_own_attribution)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_LOG=info mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/otel/frameworks/test_framework_telemetry.py::test_ambient_auth_gateway_telemetry'
```

Run the exact existing shared settlement regression named above:

```sh
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=state::tests::telemetry_flush_failure_settles_with_the_native_drain)'
```

Changed-owner static lanes:

```sh
mise run fmt
mise run lints
mise run py:format:check
mise run py:lints
mise run py:typecheck
```

Finish with `git diff --check` and an audit of changed files. No full framework
matrix, Rust OTLP suite or repository aggregate is required for this remediation;
final change review owns integrated selection. Do not run concurrent Cargo
commands or synthetic host load. Re-run failures with tracing, inspect their
logs and correct the cause; no skipped/ignored tests, weaker assertions, hidden
errors or compatibility columns. Preserve substantive rustdoc on each materially
changed Rust item. No generated contract or docsite surface is selected.

Append truthful RED/GREEN and static proof, exact commands/counts, the implemented
candidate, affected callers/siblings and remaining limits. Re-review the complete
cumulative original task with R1/R2/R3 afterward. Implementation does not grant
review PASS; this review applies no fixes, staging or commits.
