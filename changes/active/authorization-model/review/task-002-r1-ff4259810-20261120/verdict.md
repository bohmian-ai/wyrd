# TASK-002 R1: FIX_REQUIRED

Access suspension and readable suspended accounts are accepted. Alice now consumes
the UI-created grant through the requested consumers, but the journey produces no
review results/evidence. The Oracle fix also lacks peer Variant compatibility
proof across a concretely lossy native codec. No data leak is established.

## Immutable subject and authority

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Branch: `task/authorization-model/TASK-002`.
Remediation base: `29a9d276f5e668df56f5f16e898a73f7aefc46a0`.
Candidate: `ff42598100b0edabad5cad2272a3c7735f2eef14`.
Commits: `568b89b9a`, `c8fdad70d`, `ff4259810`.
Cumulative original base: `300542877cd2a58cbe5f061d5075616e4cc4397b`.
Reviewed the direct remediation diff; ancestry is preserved.

Approved contract: supplied working-tree SPEC-authorization-model revision 2;
original TASK-002 and untracked R1/evidence remain context. Spec/task hashes match
the prior subject; identities and navigation are in [subject.md](subject.md).
Maintainer packet edits were preserved. Candidate source and HEAD remained fixed.

Authority: root/SQL nested AGENTS; task-review and UI skills; spec-driven workflow;
maintainer style and applicable Rust/error guides; TESTING; architecture authority
map and owning principal/credential, authorization, retained-data, Variant SQL and
peer execution rules. The maintainer expressly authorized readable suspended
Services, including listing 404→200 and issuance 404→409. This is scope, not a finding.

## Independent assignments

Fresh ponytail-rev covered closure, UI/BFF, connected consumers, requirements and
remaining repository rules in [task-review-remediation.md](task-review-remediation.md).
Oracle specialist covered masking, field metadata, storage/optimizer/peer consumers
and waits in [task-review-oracle.md](task-review-oracle.md). Domain specialist
traced every production lookup consumer, SQL capabilities, authentication,
credentials and revocation in [task-review-domain.md](task-review-domain.md).
A separate fresh validator independently checked coverage and every proposal in
[findings-validation.md](findings-validation.md). No retained blocker originated
with the validator. The orchestrator assembled this verdict without discovering
or validating findings itself.

Additional invariant/repository/maintainer/system reviewers were omitted because
these concrete assignments cover the affected lifecycle, security, Oracle and UI
boundaries. No uncovered broad abstraction or recovery change required another
specialist. All selected reports are present.

## Reconciled coverage matrix

| Existing requirement / scenario | Implementation, assertions and supplied execution | Result |
| --- | --- | --- |
| FIND-TASK-002-1 / REQ-006, AC-001: connected identity and retained membership/input | Alice journey `ui-pages.integration.test.ts:2771–2853`: actual UI-created grant/login on browser, CLI, TS, Python and MCP; payments member/input/head, admin 2 versus Alice 1 and empty offset page. S1 passing recorded. | PASS for these seams |
| REQ-006: referenced definition checked independently | Admin can read Verifier; Alice member/direct Card/SDK definition refused at 2835–2841. | PASS |
| FIND-TASK-002-1: produced results/evidence | New queries target inputs; member remains pending; no Verifier runs and results/evidence/events empty. | FAIL: FIND-TASK-002-1 |
| FIND-TASK-002-2 / REQ-008: suspension | Form → existing BFF/session action → existing revoke owner. S2 real User/Service suspension, reflected state, bindings/keys preserved, browser/key refusal. | PASS; finding closed |
| REQ-008 refusal/presentation | S2 non-admin direct POST 403 and rejected reason without mutation, safe error, keyboard/focus/light/dark/narrow viewport assertions. | PASS |
| Changed principal lifecycle | TenantConn/SYSTEM boundary retained; active-only issuer predicate, readable suspended metadata, 409 before issuance, idempotent revoke and cancellation. K1/S2 plus supplied owner/CLI runs. | PASS within supplied evidence |
| REQ-006 local Variant repair | Shared field-typed CASE mask; four optimized key-expression cases collect expected values. O1 and 221 Oracle units supplied passing. | PASS locally |
| REQ-006 / Variant wire/peer contract | Native cast codec drops target Field metadata; projection reconstructs schema; scoped roundtrip checks string result_id only. | FAIL: FIND-TASK-002-3 |
| Unchanged original TASK-002 promises | Carry forward prior REQ-002/003/007/008/009 local/device/provider login, runtime-family, scoped role/group forms, GitHub isolation, safe copy/current-session gain/loss and docs/projections evidence. Changed consumers separately traced above. | PASS within prior limits |

Detailed source and execution provenance is in the independent reports and
validation. This matrix records existing obligations; it creates no requirements.

## Validated findings and handoff

The retained findings are recorded once in [findings-validation.md](findings-validation.md):

- **FIND-TASK-002-1 — EVIDENCE_GAP, partially closed:** consume actual produced
  payments review results/evidence and exclude populated ledger counterparts.
  Preserve accepted identity/member/input/count/definition proof.
- **FIND-TASK-002-3 — EVIDENCE_GAP:** qualify whole-Variant field/schema/value
  parity through the actual peer codec, correcting demonstrated loss at its owner.
  This does not establish that key functions necessarily fail remotely or rows leak.

**FIND-TASK-002-2 is closed.** Shredded pushdown remains unverified, but the CASE
barrier predates this range and no new cast-caused regression was demonstrated;
no separate optimization finding is retained.

One cohesive follow-up is [TASK-002-R2-prove-produced-reviews-and-peer-variants.md](TASK-002-R2-prove-produced-reviews-and-peer-variants.md).
This is the first unsuccessful TASK-002 remediation review, not a two-round
convergence escalation. The original proof gap is substantially narrowed; the new
peer gap concerns the changed Field-carrying expression. No lifecycle rework,
speculative hardening or historical RED reconstruction is prescribed. Evidence
gaps do not require production changes unless strengthened proof exposes a defect.

## Verification and process history

Reviewed-range `git diff --check`: clean. Reviewers inspected actual assertions
and supplied exact R1 selectors/results; no runtime checks were rerun. Supplied:
S1/S2 pass after TS/testing build; O1 and 221 Oracle units pass; K1/surrounding
browser/identity/platform set 48 pass; auth/SQL principal sets 71; CLI principal
journeys 4; fmt/lints/UI/codegen checks recorded clean. These are implementer
execution records, not fresh reviewer runs. Prior broad Bifrost **64/66** remains
an integrated-change limitation and is not reported green.

Added/moved hot-path waits: **PostgreSQL none; Scribe none; object storage none;
Vault none**. Oracle adds synchronous expression/Field construction; issuer adds
an in-memory status predicate. Administration uses its existing transaction and
is outside defined hot paths. No new pool/client/runtime/lock-across-await or
blocking audit was found. Benchmark qualification was explicitly excluded by the
maintainer in the prior subject/remediation and reflected in the supplied approved
spec. No benchmark or measured-performance acceptance is claimed.

Behavioral verdict: **FIX_REQUIRED**. TDD remains separate: original RED history
unverified; Alice assertions after exploratory probes preserve the acknowledged
**VIOLATED** test-first history. S2/K1/O1 RED-before-fix is supplied evidence, not
independently witnessed history. Acceptance of other rows does not erase it.

Skipped runtime reruns, broad suites, screenshots, benchmarks and deployment.
No source/authority edits, commits, merge or push. Remaining material risks are
unproved produced-output access and peer Variant field parity.

Independent bounded comprehension check of the final R2 packet passed: the
validator confirmed both focused selectors, preserved accepted seams and limits,
and found no unsupported scope or contradiction. Final HEAD, spec/task/R1 hashes
and tracked dirty-file set match the established subject.
