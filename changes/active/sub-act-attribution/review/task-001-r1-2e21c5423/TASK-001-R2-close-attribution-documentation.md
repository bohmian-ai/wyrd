---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-sub-act-attribution
spec_revision: 6
requirements: [REQ-002, REQ-003, REQ-006, REQ-008, REQ-010, AC-006]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-8, FIND-TASK-001-9]
---

# Complete attribution documentation

Execute directly through `$wyrd-implement`. Correct the two independently
validated documentation gaps in one bounded task. The runtime behavior and
identity rules are already approved by spec r6; no new decision or runtime
implementation is required.

## Authority and immutable inputs

- Approved specification: [spec.md](../../spec.md), revision 6; r6 approval
  commit `785d8293e44afcac5e94be15210026c4855650a6`.
- Original task: [TASK-001-sub-act-attribution.md](../../tasks/TASK-001-sub-act-attribution.md).
- Original cumulative base: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f`.
- Reviewed R1 candidate/start point: `2e21c5423036460d5da2ca22a51efde63635e352`.
- R1: [TASK-001-R1-close-attribution-gaps.md](../task-001-518a5d8b8/TASK-001-R1-close-attribution-gaps.md).
- Original review: [verdict.md](../task-001-518a5d8b8/verdict.md) and
  [findings-validation.md](../task-001-518a5d8b8/findings-validation.md).
- Current verdict and source-validated correction boundaries:
  [verdict.md](verdict.md), [findings-validation.md](findings-validation.md).
- Governing rules: `AGENTS.md` §16, `architecture/agent-rules.md`, approved
  spec r6, and the architecture/reference hierarchy in
  `architecture/references/README.md`; apply maintainer-style and
  spec-driven-development references.

Keep all historical artifacts and finding IDs intact. Findings 1, 2 and 4–7
are closed in current source; finding 3 is superseded by approved r5, not a
missing implementation. This task carries only the remaining portions of
findings 8 and 9. Do not reopen already corrected items or change the code to
match superseded documentation.

## FIND-TASK-001-8: changed Rust items lack required documentation

### Diagnosis

`AGENTS.md` §16 and agent rules require substantive rustdoc on every new or
materially changed Rust item, including private/trait methods, constants and
tests, with panic conditions when applicable. The original enumerated
registration/conversion/replay and three test comments were corrected in R1.
The cumulative candidate still has these five genuinely changed items:

| Current location | Evidence and missing contract |
|---|---|
| `crates/vala/vala-bifrost-redux/src/tables/audit/audit_log.rs:57`, `AuditLogTable::arrow_fields` | Its changed schema produces the subject/kind/actor decision-content columns and all other audit fields. No method rustdoc explains why identity belongs to content under `CorrelationPolicy::None`, instead of being overwritten by managed transport identity. |
| Same file, line 75, `AuditLogTable::physical_layout` | Its changed layout uses decision time and subject/resource/operation pruning. No method rustdoc describes the daily decision-time layout, sort or pruning responsibility. |
| `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:3255`, new authority-reload proof | The test documents returned errors, but its model-reference construction retains an `expect` at line 3362. It has no required `# Panics` contract. |
| Same file, line 3256, inner `MODEL_ROLE` constant | This new private constant represents the model permission added after acceptance and used by the positive control. It has no rustdoc. |
| `crates/wyrd/wyrd-server/tests/integration/platform_admin_e2e.rs:5240`, extended built-in provisioning proof | New provenance assertions and the existing fixture/request/parse expectations can panic. The materially extended item lacks `# Panics`. |

The schema methods feed the existing canonical table/catalog and audit
projection. The tests are actual provisioning and recovered-authority proofs.
These omissions fail an explicit repository completion rule; they do not
establish a runtime defect. Green formatting/Clippy and successful test runs
cannot establish the missing comments' contents.

### Selected correction

Document these five items accurately in their existing owners. Explain audit
content versus transport identity and actual layout/pruning in the table
methods, the model-reference invariant in the new test's panic contract,
the constant's role, and provisioning-test panic conditions. Reuse the nearby
documented `DomainTable` and error/panic test conventions. Read each complete
body so descriptions match its actual behavior.

Preserve every body, schema field, layout, role, expiry setting, expectation
and assertion. No test, fixture, wrapper, scanner, suppression or unrelated
rustdoc sweep is justified. Keep revised permanent prose free of task IDs.

## FIND-TASK-001-9: authoritative prose still describes removed contracts

### Diagnosis

REQ-002/003/006/008/010, AC-006 and the original task explicitly require
architecture/security documentation to agree with the new attribution and
accepted-authority contract. R1 corrected the public result description and
Wyrd/Bifrost result/peer sections. Four remaining passages contradict the
actual producers and consumers:

1. `architecture/wyrd-security-posture.md:70–80` says capture rows carry
   `GATEWAY_CAPTURE_PRINCIPAL`, describes result-only Verifier peer attribution,
   and says the peer service never accepts the reserved system tenant.
   `GatewayInvocation`/`CallCapture` retain the operation pair; the shared
   `ScribeWrite`/`ScribeBatch` path and `IngestCaptureRequest` carry the pair.
   `ScribeCapturePeerGrpc::batch` admits the fixed capture/result/audit
   destinations and permits `SYSTEM_OWNER` specifically for audit.
2. The same file at lines 203–218 describes internal result/read tokens,
   Verifier scopes, and tokenized Drift versus tokenless Eval. Both readers
   use `SystemReadAuthority::resolve/serving`, which constructs a credentialless
   SYSTEM context with exact input table-UID grants. Results use the process
   outbox: direct results retain the caller pair; queued results use run
   subject/SYSTEM. No internal SYSTEM token is minted.
3. Line 224 calls accepted Workflow work the only exception to request-scoped
   authority, although durable accepted Verifier authority is also approved,
   persisted on the existing queue and restored into `Caller::accepted`.
4. `architecture/wyrd-design.md:134–135` still names managed `principal_id` for
   generic telemetry. `tables/managed_columns.rs` and Scribe's managed arrays
   expose the subject/actor pair instead. This occurrence is an obsolete work
   column, not a retained identity/ownership key, and violates AC-006.

The result is a false security/row contract: a reader expects token and peer
admission mechanisms the implementation intentionally does not provide, or
queries a removed managed field. The explicitly tasked security authority
also contradicts its own corrected tokenless section. A green docs build is
not semantic proof. No runtime exploit or authorization bypass is asserted.

### Selected correction

Finish reconciliation in the existing two architecture documents. Delete or
replace the superseded token description using the already-correct nearby
account of tokenless result staging and narrow SYSTEM reads. Align capture/
peer payload, destination and reserved-owner statements with the existing
outbox, wire and peer receiver. Remove Workflow-only exclusivity while keeping
both accepted-authority lifetimes and all existing Workflow bounds. Name the
subject/actor pair in the generic telemetry sentence.

Reuse the corrected Wyrd/Bifrost and observation descriptions; do not add
another parallel explanation when deletion or a short amendment suffices.
Preserve the public Gate refusal, fixed destinations, admitted mutual-TLS
boundary and compromised-peer threat assumption. Preserve audit payload
identity separately from its transport sentinel, credentialless/narrow reads,
and unchanged Workflow graph/deadline/downstream permission checks. No runtime
code or contract needs to change to fit the old wording.

## Scope, constraints and non-goals

The expected write set is the three Rust source files and two architecture
files identified above. Locations identify existing items, not permission to
restructure their modules. No implementation, schema, wire, permission,
identity, admission, lifecycle, fixture, dependency, feature, config, migration,
alias or test-assertion change is part of this correction.

Keep r5 creator provenance and pre-identity refusal behavior, r6 local
Workflow Card correlation/scope refusal, and all other closed attribution,
authority, tenant, sampling, trace and nonblocking-outbox behavior intact.
No new product/security decision or broad repository cleanup is needed.

## Acceptance and focused proof

| Finding | Acceptance | Focused proof |
|---|---|---|
| FIND-TASK-001-8 | All five listed changed items have accurate substantive rustdoc and required panic contracts; bodies/assertions remain unchanged. | Compare comments to full item bodies and existing schema/layout/test purpose; inspect a documentation-only diff and run Rust formatting. |
| FIND-TASK-001-9 | All four listed passages agree with actual operation pairs, audit transport/content, peer admission, tokenless reads/results, accepted Verifier/Workflow authority and current managed fields. | Compare prose to the source owners below and corrected neighboring sections; inspect that the superseded claims are removed. |

For finding 9, read the full relevant bodies of gateway `CallCapture`,
`ScribeWrite::rows`, `ScribeBatch::into_frame`, `ScribePeers::submit`,
`ScribeCapturePeerGrpc::batch`, `IngestCaptureRequest`,
`SystemReadAuthority::resolve/serving`, direct `stage_result`, queued result
construction, accepted-authority capture/claim, and the managed column/array
owner. These are comparison evidence, not implementation changes. The source
traces and current locations are preserved in `findings-validation.md`.

This task changes prose only, so use static proof rather than manufacturing
RED/GREEN runtime churn. Record before/after source-backed closure for each
finding, the actual write set and commands, and that executable behavior was
preserved. Run from repository root:

```bash
mise run fmt
git diff --check
```

Inspect the remediation diff against `2e21c5423036460d5da2ca22a51efde63635e352`
to verify it contains only the required documentation. If docsite content under
`docs/` is also necessarily edited, run `mise run docs:check`; the identified
architecture-only correction does not require that extra write set.

Do not run broad runtime suites, full journey suites, `gate`, new tests or a
permanent text check for these documentation-only findings. Those checks
cannot prove semantic documentation completeness. If an executable correction
appears necessary, stop and identify the conflict with this validated boundary
rather than adding runtime work to a prose remediation.

Append evidence to this task and hand a new immutable candidate to
`$wyrd-task-review`. Preserve the original base, r6, original task, historical
verdicts/ledgers and R1/R2 so review reassesses the complete cumulative change.
Implementation does not close its own findings or rewrite a prior verdict.

## Implementation evidence

Start point `2e21c5423`. Commits: `7a2bdb66b` (FIND-8), `8d8c134e2` (FIND-9).
The write set is exactly the five expected files, with documentation-only
hunks (`git diff --stat 2e21c5423 8d8c134e2`). No body, schema field, layout,
role, expiry, expectation, assertion, runtime code, wire, permission, fixture
or test was changed.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-8 `AuditLogTable::arrow_fields` | Rustdoc: 13 content columns. The subject/kind/actor columns are decision content that `CorrelationPolicy::None` keeps from being overwritten by the transport identity. The catalog appends only system columns. | Compared to the body and to `ScribeBatch::into_frame` (audit transport sentinel); `mise run fmt` | PASS |
| FIND-TASK-001-8 `AuditLogTable::physical_layout` | Rustdoc: daily on decision `created_at`, sorted newest first, Bloom-pruned on `sub_principal_id`/`resource`/`operation`; the catalog unions the managed Bloom floor | Compared to the body and to `daily_layout` | PASS |
| FIND-TASK-001-8 `reloaded_authority_cannot_widen_gateway_access` | `# Panics`: runtime build, the static `openai/gpt-test` model-reference `expect`, and indexing of the two projected audit columns | Full body read: the only `expect` and the `row[0]`/`row[1]` indexing | PASS |
| FIND-TASK-001-8 inner `MODEL_ROLE` | Rustdoc: held by A, granted to B only after acceptance, so A∩B lacks it and B's fresh token (the positive control) has it | Compared to `bootstrap_user`, `grant_role` and the fresh-token call | PASS |
| FIND-TASK-001-8 `new_and_existing_tenants_receive_every_builtin` | `# Panics`: start/restart, route responses, id parsing, row insert, tenant reads, and the inventory and creator-pair assertions | Full body read against each `expect` and `assert_eq!` | PASS |
| FIND-TASK-001-9 security posture, capture/peer | Rows carry the verified pair. The peer RPC carries the pair and accepts the six fixed destinations. Only audit may name the reserved system tenant, and audit content travels under the `PLATFORM_AUDIT_PRINCIPAL` transport. The `GATEWAY_CAPTURE_PRINCIPAL` token refusal, the public Gate refusal, mTLS and the compromised-peer assumption are kept. | `ScribeCapturePeerGrpc::batch`, `ScribeTable::ALL`/`fqn`, `ScribeBatch::into_frame` | PASS |
| FIND-TASK-001-9 security posture, result/read tokens | The token bullet is replaced: no token is minted. Results go on the outbox under the direct caller pair or the queued subject/SYSTEM. Drift/Eval reads use the credentialless SYSTEM authority on input table UIDs only and fail closed. | `SystemReadAuthority::resolve`; the tokenless SYSTEM paragraph earlier in the same file | PASS |
| FIND-TASK-001-9 security posture, Workflow exclusivity | "the one … exception" now reads "like the accepted Verifier run described above, is a bounded exception". The Workflow graph, deadline and live per-call checks are kept. | The accepted-Verifier-authority paragraph in the same file | PASS |
| FIND-TASK-001-9 `wyrd-design.md` generic telemetry | `principal_id` becomes the managed `sub_principal_id`/`act_principal_id` pair | `tables/managed_columns.rs` | PASS |

Commands, run from the repository root:

- `mise run fmt`: PASS
- `git diff --check`: PASS
- `mise run docs:check`: PASS. Only architecture Markdown changed, so this run was not strictly required.

A stale-term grep for `result tokens`, `read token` and `result-write scope` in the security posture returns nothing.

Per the task, no runtime suite, journey, gate or new check was run.

Status: IMPLEMENTED. Findings are not marked closed; this candidate goes to `$wyrd-task-review`.
