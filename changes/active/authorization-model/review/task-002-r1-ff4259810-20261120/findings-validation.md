# TASK-002 R1 independent findings validation

Recommendation: **FIX_REQUIRED**. Account suspension is closed. Alice's connected
review membership/input proof is substantially improved, but produced review
results/evidence remain unproved. The newly field-typed Oracle mask also lacks
credible peer Variant compatibility proof at a concretely lossy codec seam.
Neither retained evidence gap establishes a data leak.

## Subject and independent scope

Immutable remediation base `29a9d276f5e668df56f5f16e898a73f7aefc46a0`, candidate
`ff42598100b0edabad5cad2272a3c7735f2eef14`; cumulative original base
`300542877cd2a58cbe5f061d5075616e4cc4397b`. Approved supplied working-tree
SPEC-authorization-model revision 2 and original TASK-002 hashes still match
[subject.md](subject.md). The maintainer's dirty packet remains context, not
candidate implementation. HEAD remained the candidate during validation.

This fresh validator independently read the approved requirements, original task,
prior findings/verdict, R1 task/evidence, actual remediation diff and affected
source; then reconciled all three selected reports:
[remediation](task-review-remediation.md), [Oracle](task-review-oracle.md) and
[domain](task-review-domain.md). CodeGraph preceded source exploration; exact
reads completed areas it did not return. Applicable root/SQL instructions,
task-review/UI skills, spec-driven development, maintainer style, TESTING and
owning security/Bifrost references informed the checks.

The primary reviewer covers closure/UI/connected consumers and residual repository
rules. The Oracle specialist covers the changed plan/Variant/storage/peer seam;
the domain specialist covers all status-independent principal-lookup consumers,
credential usability, lifecycle and tenancy. These are the material newly affected
boundaries. Additional repository/maintainer/system reviewers would duplicate
these assignments. Unchanged source-backed TASK-001/TASK-002 proof remains usable
for unchanged enforcement, freshness, ceilings, roles/groups, provider flows and
language contracts; it cannot replace missing consumers of Alice's UI-created
grant or qualify a newly relied-on physical-expression representation.

## Proposal dispositions

| Proposal | Disposition | Independent reason |
| --- | --- | --- |
| Primary: partial closure of FIND-TASK-002-1 | **CONFIRMED**, narrowed remainder retained | All new data queries use execution_inputs and the expected member stays pending; REQ-006 explicitly includes code-review results and retained evidence. Inputs/membership/counts and connected identities are now proved, but not produced review results/evidence. |
| Primary: closure of FIND-TASK-002-2 | **CONFIRMED CLOSED** | Active selected-account form forwards to existing revoke API; production journey covers User/Service state, preserved bindings, real browser/key refusal, non-admin direct-post denial and unchanged state on rejected reason. |
| Oracle ORA-1 | **REVISED**, retained as FIND-TASK-002-3 | Narrow to whole-Variant field/schema and value parity across the native peer codec. Pinned source loses explicit target Field metadata; no Variant codec assertion covers it. Do not assert that scalar key functions necessarily fail or that every query is dispatched. |
| Unverified shredded-leaf pushdown | **No finding retained** | Existing CASE masking predates this range; no new cast-caused supported-use regression was demonstrated. Disclose the limit without inventing an optimization remediation. |

The domain specialist proposes no blocker. Independent diff/source checks agree:
SQL keeps TenantConn and SYSTEM exclusion; the issuer now checks machine status
explicitly before identity construction, preserving its former active-only
semantics. Metadata listing distinguishes existence from usability, and issuance
refuses suspended User/Service before minting. The maintainer-authorized 404→200
listing and 404→409 issuance changes are scope, not independently discovered
findings. No migration or parallel lifecycle is introduced.

## Retained findings

### FIND-TASK-002-1 — EVIDENCE_GAP: produced review results and evidence remain unproved

**Authority:** REQ-006 first paragraph (`spec.md:140–147`), AC-001, original
TASK-002 cycle 1 and prior validated FIND-TASK-002-1/R1 closure scenario.

**Location:** `crates/wyrd/wyrd-server/wyrd-ui/src/lib/ui-pages.integration.test.ts:2771–2834`,
particularly REVIEWS/MEMBERS (`:2772–2773`), expected pending member (`:2793`) and
browser pending assertion (`:2834`); supplied R1 acceptance/evidence explicitly
states no Verifier executes and results/evidence/events remain empty.

**Starting conditions and trigger:** Access creates Alice with viewer on payments
family, two Repository PR deliveries are admitted, and Alice signs in and redeems
her real CLI login. She opens payments reviews/results/evidence with that grant
while ledger remains ungranted.

**Expected proof:** at least one actual produced payments review result and its
retained evidence are consumed under this created identity; populated ledger
outputs contribute no returned data, counts/pages or direct disclosure. Existing
input/member/definition checks remain valid proof and need not be repeated.

**Actual proof:** admin and Alice read real execution_inputs containing the PR
review-set/member/head data; CLI, TS, Python and MCP reuse Alice's redeemed login;
Alice's browser opens the exact pending member. These close the previously absent
identity-to-retained-admission seam. Every new SQL query still targets
execution_inputs, and no Verifier executes, so no assertion exercises a produced
review result/evidence value or excludes a populated ledger counterpart. An empty
store cannot establish positive result access or populated-result exclusion.
Separate backend fixture accounts remain supporting enforcement evidence, not
proof of this task's connected output.

**Consequence:** required connected code-review result/evidence behavior remains
unproved. This is not a production filtering defect, a requirement to populate
every table, or a demand for additional language-lifetime scenarios.

**Smallest testable correction:** extend the existing Alice/PR deployment and
Verifier fixtures enough to produce real review results/evidence for payments and
ledger through existing owners. Reuse Alice's already connected browser/login and
public consumers to inspect actual visible outputs and ledger exclusion before
counting/pagination. Keep the passing membership/input/definition assertions.
Run the focused Alice production BFF/server/Postgres selector. No authorization
production change is required unless the strengthened proof exposes a defect.

### FIND-TASK-002-3 — EVIDENCE_GAP: the field-typed mask is not qualified across the peer codec

**Authority:** REQ-006 requires managed-row visibility across peers and retained
sources (`spec.md:156–164`); `architecture/bifrost-design.md` §§ Variant SQL and
Distributed analytical execution require consistent pinned-codec execution and
`arrow.parquet.variant` on every Variant result; security posture § Resource and
retained-data visibility applies the same boundary regardless of source/peer.

**Location:** new explicit Field cast in
`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3996–3997`; native peer encoding in
`oracle/codec.rs:76–83` and production follower decoding in `oracle/follower.rs:991`;
existing scoped roundtrip assertion at `oracle/codec.rs:754–836` selects only
`result_id`.

**Starting conditions and trigger:** a Card-scoped reader queries a managed
verification Variant column and a peer-bound physical subtree contains its masked
whole-Variant projection. The local expression obtains its extension metadata
from the newly explicit target Field. Native peer serialization must preserve the
same field semantics, visible values and hidden-row masking.

**Expected proof:** the masked Variant projection's schema, extension metadata and
values/null behavior survive the actual physical codec roundtrip and remain usable
by the existing Variant consumer. A string-only projection cannot establish that.

**Actual source and proof:** the local O1 regression exercises four key reads and
the Alice journey exercises a local retained-input key read. The supplied record
explicitly leaves field-typed cast peer serialization unverified. Independent
inspection of the exact pinned DataFusion revision
`5f4b1713af4f0394c7c564b6bc6de42e92640186` establishes a concrete seam:

- `datafusion/physical-expr/src/expressions/cast.rs:409–429` encodes child plus
  ArrowType only, omitting the explicit target Field; `:432–461` decodes through
  `CastExpr::new`, making a type-only cast.
- The same file's `resolved_target_field` at `:214–249` retains explicit metadata
  but strips extension keys from a type-only cast.
- `datafusion/physical-plan/src/projection.rs:634–724` serializes expressions and
  aliases, then reconstructs through `ProjectionExec::try_new`; no separately
  serialized output schema restores the omitted extension.
- Wyrd's production follower uses native physical decoding; the pinned distributed
  dependency's `src/codec/physical_plan.rs:26–42` also uses native conversion. The
  Wyrd extension codec handles source placeholders, not native CastExpr metadata.

These pinned sources were inspected in the local Cargo checkout, not inferred
from an arbitrary upstream version. The change's explicit-field guarantee is not
represented by that codec, and no supplied assertion establishes whole-Variant
peer parity. This is a material changed-representation qualification gap, not a
blanket request to retest every peer operator. It does not establish that every
`->>` expression fails: scalar-function decoding retains its own return field and
may evaluate the array without rechecking argument extension fields. No leak or
exhaustive end-to-end failure is asserted.

**Consequence:** a supported peer-bound masked Variant may differ in output field
semantics and consumer decoding despite the local fix. Local happy-path execution
cannot qualify that serialized boundary.

**Smallest testable correction:** extend the existing scoped native codec
roundtrip test to return a whole Variant and read it by key; assert local/decoded
schema extension, actual values and exclusion/null behavior match. Use the existing
fixture and real codec, without a new policy engine or Wyrd Variant wrapper. If
that executable seam demonstrates loss, correct it at the native codec owner and
update the pinned revision/fingerprint together; run the narrow production peer
journey needed to establish consumer behavior after such a fix. Do not demand
production changes merely to address absent assertions. Exact existing test runner:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::codec::tests::a_masked_verification_plan_executes_identically_on_a_peer)'
```

This new proposal came from the Oracle discovery specialist and was independently
source-validated here; it is not a self-validated new validator finding.

## Reconciled acceptance matrix

| Existing promise | Strongest applicable proof and scope | Result |
| --- | --- | --- |
| FIND-1 / REQ-006, AC-001: UI-created account reaches retained review set, member and input | Alice `:2771–2834`; actual admin 2 / Alice 1, empty offset page, retained head/member/generation; real redeemed login on CLI/TS/Python/MCP and production browser | PASS for these closed subclaims |
| FIND-1: separate referenced Card definition | Admin can read code-review; Alice member page/direct Card/TS reads refuse at `:2835–2841` | PASS |
| FIND-1: actual produced review results and evidence | Deployment never executes a Verifier; pending member and execution_inputs only | FAIL — FIND-TASK-002-1 |
| FIND-2 / REQ-008: User/Service suspension and refusal | Existing revoke contract via new BFF action/form; S2 assertions `:3058–3121`, K1 suspended metadata/issuance `pg_browser_sessions.rs:2088–2113` | PASS; FIND-TASK-002-2 CLOSED |
| Changed principal lookup / credential invariants | All production consumers traced; active issuer predicate retained; tenant/SYSTEM exclusion; real-server listing preserved and issuance conflict | PASS within supplied focused evidence |
| Changed Oracle local Variant/masking | O1 unit collects real optimized Variant expressions and existing hidden-row negatives; Alice reads retained head by key | PASS locally |
| Changed Oracle peer Variant representation | Native codec omits explicit target Field; existing peer assertion selects only string result_id | FAIL — FIND-TASK-002-3 |
| Prior REQ-002/003/007/008/009 accepted rows | Unchanged local/provider/device, runtime family/releases, GitHub separation, scoped roles/groups/keys, safe copy and same-session access-change proof retained from prior review | PASS carried forward; no unrelated reopening |

## Execution, waits, exclusions and process history

No tests, Cargo builds or broad journeys were rerun by this validator. Supplied R1
record gives exact S1/S2/K1/O1 selectors and passing execution; inspected assertions
establish what those runs can prove. Owner `ui:check`, fmt/lints, codegen and the
reported surrounding test sets are supplied results, not fresh validator runs.
Reviewed-range `git diff --check 29a9d276f ff4259810` is clean. The prior broad
Bifrost result remains **64/66** and belongs to integrated change verification;
it is not represented as green or repaired by a future unmerged commit.

External waits added/moved on repository-defined hot paths: PostgreSQL **none**,
Scribe **none**, object storage **none**, Vault **none**. The Oracle change is a
synchronous expression/Field rewrite; issuer change is a synchronous status
predicate over its existing lookup result. New Access suspension uses the existing
administrative transaction, outside those hot paths. No new catalog, audit,
storage or secret await is introduced. Benchmark qualification and historical
comparison were expressly excluded by the maintainer in prior subject/R1 and the
approved supplied spec; no measured-performance PASS is claimed.

Behavioral recommendation is FIX_REQUIRED for two evidence gaps. TDD remains
separate: original UI-cycle RED history is unverified; Alice assertions were
written after exploratory probes and retain the disclosed test-first violation.
O1, S2 and K1 have supplied RED-before-fix records, not independently witnessed
historical logs. No reconstructed RED or production churn is requested.

This is the first TASK-002 remediation review: one original finding closes and
the other is reduced to produced-result/evidence proof; one bounded changed-code
codec gap is newly validated. No two-unsuccessful-round convergence threshold has
been reached. The next task should preserve closed seams and target only these
remaining proof boundaries.

Skipped runtime reruns, screenshots, benchmarks and implementation. Residual risk
is unqualified produced-review consumption and peer Variant compatibility; no
credential leak, authorization bypass or filtering regression is established.
