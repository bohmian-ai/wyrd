# BIFROST-OTEL-T04 task review

## Verdict

`SPEC_REVISION_REQUIRED`

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Base: `442da074cb316be7f580694ba8274229561935a8`
- Candidate: `81eaa346e643ac6315e517041fc838c41057f7ac`
- Approved specification:
  `changes/active/bifrost-canonical-otel-signals/spec.md`, revision 11
- Original task:
  `changes/active/bifrost-canonical-otel-signals/tasks/04-integrated-public-journeys.md`
  as it existed at the base

## Acceptance matrix

| Requirement, criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 Gate owns authenticated routing and request identity | `gate/mod.rs:627-683` | focused Gate test | PASS |
| REQ-004 equivalent public writes preserve stored semantics and row identity | deterministic OTLP identity at `gate/mod.rs:101-119` omits principal identity | current test varies tenant/table/payload only | FAIL |
| REQ-005 mixed requests atomically retain accepted siblings | existing table projection and replay journey | OTLP journey reported 9/9 | PASS |
| REQ-006–REQ-018 complete canonical trace/log/metric fidelity | preserved prior T04 implementations; no candidate regression found outside identity | task report's named raw/stock-SDK journeys | PASS |
| REQ-019–REQ-021 canonical public query, stale removal, and single mapping authority | unchanged by candidate; stale log assertion now matches ledger restamping | focused log test reported pass | PASS |
| REQ-023 payload authorization | unchanged by candidate | existing named public read journeys | PASS |
| INV-001 one logical row per accepted signal | same-tenant principals can collide on one derived batch ID | no two-principal replay test | FAIL |
| INV-002–INV-006 lossless fields, signal boundaries, canonical source, payload safety | no contrary candidate change found | existing scenario evidence retained | PASS |
| INV-007 tenant isolation | tenant is included in derived identity | unit test varies tenant | PASS |
| INV-008 acknowledgement means authoritative durable rows | a second principal can receive duplicate success without its row/audit | one-principal replay only | FAIL |
| INV-009 bounded processing | hashes the admitted bounded canonical batch | focused test and existing limits | PASS |
| INV-010 existing harness/no shadow path | existing Gate, Scribe fence, and OTLP target reused | OTLP journey target | PASS |
| AC-001–AC-005 public fidelity and GenAI coverage | prior T04 source preserved | recorded raw OTLP and language journeys | PASS |
| AC-006 partial success and exact accepted-subset fence evidence | replay journey now proves one-principal convergence | named negative journey | PASS |
| AC-007 exact identity across retry/recovery | generated value omits principal and is not an actual UUIDv7 | unit test checks only version nibble and one principal | FAIL |
| AC-008–AC-011 schema/storage, topology, stale removal, and stock SDKs | prior T04 source preserved; log assertion corrected | recorded focused/topology/language checks | PASS |
| Reuse current harness, owners, public SDKs, and fence | no new dependency/type/trait/module; existing owners reused | diff inspection | PASS |
| Do not weaken durability, permissions, topology, timeouts, or ignored-test policy | replay assertion strengthened; no weakening found | diff inspection | PASS |
| No new migration, compatibility path, mapper, framework, collector, or wrapper | none added | diff inspection | PASS |
| Verification must be exact and truthful | report says aggregate passed but records 8/9 | separate Oracle leaf passed 28/28 | FAIL |
| Only task-related changes enter the candidate | three `changes/active/py-error-refactor/` files are in the commit | `git diff --name-status` | FAIL |
| Repository standards | independent standards audit | `standards-review.md` | FAIL |

## Material findings

### FIND-BIFROST-OTEL-T04-1 — REGRESSION

- Violated obligation: REQ-022 requires every accepted row to retain its
  authenticated `principal_id`; INV-001 and INV-008 require accepted durable
  rows to remain authoritative.
- Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:101` and
  `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:453`.
- Evidence: the derived batch identity includes tenant, table, and logical data
  digest but not `auth.principal.id`. The reused logical digest explicitly
  excludes `PRINCIPAL_ID`. The current journey replays with one token only.
- Observable consequence: two authorized principals in one tenant exporting
  the same canonical OTLP payload share one Scribe fence. The second request is
  treated as `AlreadyCommitted`, so it is acknowledged without a row or its
  audit attribution.
- Required correction: the approved identity contract must distinguish
  independently attributable logical writes while keeping a genuine retry on
  one stable identity, with a public same-tenant/two-principal proof.

### FIND-BIFROST-OTEL-T04-2 — VIOLATION

- Violated obligation: `architecture/bifrost-design.md` requires every
  `wyrd_batch_id` to be UUIDv7.
- Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:111`.
- Evidence: SHA-256 supplies all initial bytes and the implementation only sets
  the version and variant bits. The first 48 bits are not an authoritative
  Unix-millisecond creation timestamp. The test checks `get_version()` only.
- Observable consequence: persisted identity claims UUIDv7 without satisfying
  its time-field semantics.
- Required correction: approve either a real stable UUIDv7 derivation with an
  authoritative retry-stable timestamp, or a distinct deterministic identity
  representation/fence key and its persistence/public-boundary consequences.

Findings 1 and 2 expose an unresolved durable-identity decision: OTLP supplies
no stable batch key, a real UUIDv7 normally carries generation time, and a
content-only identifier conflates independently attributable writes. Choosing
a timestamp source, adding a protocol key, changing the persisted identity
format, or separating row identity from retry identity changes durable or
public semantics. Revision 11 does not select that decision, so review cannot
write an implementation remediation task for it.

### FIND-BIFROST-OTEL-T04-3 — VIOLATION

- Violated obligation: module-top imports and bare signature types required by
  `architecture/agent-rules.md`.
- Location: `crates/vala/vala-bifrost-redux/src/gate/mod.rs:101`.
- Evidence: fully qualified signature types and a function-scoped `sha2` import.
- Consequence: mandatory repository Rust style fails.
- Required correction: use the module import block and bare type names.

### FIND-BIFROST-OTEL-T04-4 — VIOLATION

- Violated obligation: rustdoc for every new or materially modified Rust item.
- Locations: `gate/mod.rs:627`, `gate/mod.rs:1963`, and `gate/mod.rs:1975`.
- Evidence: the changed durable method and new test module are undocumented;
  the panicking fixture helper lacks `# Panics`.
- Consequence: repository `BLOCK_BEFORE_MERGE` standard fails.
- Required correction: document the workflow, retry/error behavior, test module,
  and panic, or remove the panic.

### FIND-BIFROST-OTEL-T04-5 — VIOLATION

- Violated obligation: exact truthful verification evidence.
- Location: task report lines 784–797.
- Evidence: it says `verify:bifrost` passed, then records the invocation as 8/9;
  only the Oracle leaf rerun passed.
- Consequence: the required aggregate proof is not green as recorded.
- Required correction: record the aggregate accurately or rerun it uncontended
  to an actual pass.

### FIND-BIFROST-OTEL-T04-6 — DRIFT

- Violated obligation: no unrelated change may enter the reviewed task diff.
- Location: `changes/active/py-error-refactor/`.
- Evidence: its spec and two task files are additions in
  `442da074c..81eaa346e` and are unrelated to BIFROST-OTEL-T04.
- Observable consequence: the candidate is not an exact T04 change.
- Required correction: remove those files from the T04 candidate range; review
  them under their own change.

## Ponytail audit

The new helper reuses the existing digest and Scribe fence and adds no
dependency or abstraction. No smaller implementation closes the actual gap:
the content-only shortcut causes FIND-BIFROST-OTEL-T04-1, while simply restoring
fresh UUIDv7 IDs restores duplicate retry writes. The missing identity decision
must be made before minimum code can be selected.

## Repository-standards result

`FAIL`. See `standards-review.md`; all material standards findings are included
above.

## Verification limits

- Independently rerun focused Gate identity test: PASS (1/1).
- Independently checked candidate diff whitespace: clean.
- Reported focused Gate/log/negative tests and OTLP journey: PASS.
- Reported `verify:bifrost`: 8/9; separate uncontended Oracle leaf: PASS 28/28.
- `mise run gate`: not run, appropriately omitted for the scoped Bifrost code
  but not evidence for the unrelated mixed change packet.

## Prior-finding closure

The base task report's OTLP client-retry gap is not closed safely: one-principal
byte-identical replay converges, but cross-principal attribution and UUIDv7
identity remain incorrect.
