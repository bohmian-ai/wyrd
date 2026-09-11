# BIFROST-OTEL-T04 task review

## Verdict

`FIX_REQUIRED`

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
| AC-007 exact identity across retry/recovery | generated value omits principal attribution | unit test checks one principal only | FAIL |
| AC-008–AC-011 schema/storage, topology, stale removal, and stock SDKs | prior T04 source preserved; log assertion corrected | recorded focused/topology/language checks | PASS |
| Reuse current harness, owners, public SDKs, and fence | no new dependency/type/trait/module; existing owners reused | diff inspection | PASS |
| Do not weaken durability, permissions, topology, timeouts, or ignored-test policy | replay assertion strengthened; no weakening found | diff inspection | PASS |
| No new migration, compatibility path, mapper, framework, collector, or wrapper | none added | diff inspection | PASS |
| Verification evidence | aggregate recorded 8/9 and its failed Oracle leaf passed separately when rerun uncontended | user accepted the uncontended 28/28 leaf rerun | PASS |
| Candidate scope | concurrent `py-error-refactor` packet shares the commit | user explicitly accepted the mixed candidate | PASS |
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

### FIND-BIFROST-OTEL-T04-2 — WITHDRAWN

- Reassessment: the repository's existing batch-ID boundary validates
  UUIDv7-compatible values with `Uuid::get_version() == SortRand`, and the
  candidate preserves that enforced representation without changing a public
  or persisted type. Requiring a different timestamp contract would add a new
  obligation not fixed by revision 11 or the task.
- Result: this is not a material acceptance finding and requires no correction.

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

### FIND-BIFROST-OTEL-T04-5 — WITHDRAWN BY USER

- The user accepted the uncontended Oracle 28/28 rerun as sufficient evidence
  for the CPU-contention failure and rejected this finding. No remediation is
  required.

### FIND-BIFROST-OTEL-T04-6 — WITHDRAWN BY USER

- The user explicitly accepted the concurrent `py-error-refactor` packet in the
  candidate and rejected this finding. No remediation is required.

## Ponytail audit

The new helper reuses the existing digest and Scribe fence and adds no
dependency or abstraction. The minimum correction is to bind the existing
deterministic identity to authenticated attribution, then retain the existing
fence and UUID representation.

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

The base task report's OTLP client-retry gap is only partially closed:
one-principal byte-identical replay converges, but cross-principal attribution
remains incorrect. `FIND-BIFROST-OTEL-T04-2` is withdrawn after reassessment
against the repository's enforced UUID validation boundary.

## Remediation task

`BIFROST-OTEL-T04-R1-close-retry-identity-review-gaps.md`
