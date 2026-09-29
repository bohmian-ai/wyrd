# TASK-006 R3 verdict

## Verdict

**SPEC_REVISION_REQUIRED.** The user confirms no official image has ever been published. Approved AC-034 and AC-037 require proof from a pinned published image, so `FIND-TASK-006-13` cannot be closed by the prescribed pull-and-run correction. The approved acceptance criterion must change for pre-release review, or publication must be separately authorized and completed. Nine other bounded findings, `FIND-TASK-006-14` through `-22`, remain. The R2 remediation closes `FIND-TASK-006-11` and `FIND-TASK-006-12` and preserves closure of `-1` through `-10`.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t006`
- Original cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- R2 remediation base, excluded from its diff: `f500ea38bc749f36b3ee8d88893dcf7c0161435c`
- Candidate: `2fbe90cd880936d8e4f25173839e4cc4fa18f0b4`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 40
- Original task: `changes/active/verified-change-contract/tasks/TASK-006-continuous-eval-verifier.md`
- Prior verdicts and findings: `changes/active/verified-change-contract/review/TASK-006-r1/` and `TASK-006-r2/`
- Remediation reviewed: `changes/active/verified-change-contract/review/TASK-006-r2/TASK-006-R2-continuous-eval-closure.md`

The candidate stayed at the stated commit through both review waves. Only this new review directory was written.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Continuous Eval activation, sampling, result settlement, trace and media behavior; AC-014/016/020/027 and prior findings 1–12 | Existing Eval owners and `eval_verification.rs` terminal matrix; deleted tail tickets and raw-pool forwarder | Recorded exact Eval test and server journey 22/22; Eval domain review | PASS |
| Official image serves Rust, live Node BFF, nginx, public gRPC, first-use setup, durable restart; REQ-153/155, AC-034 | `docker/official`, shared client URL resolution, startup journey | Recorded `test:server:startup`, Python and TypeScript journeys | **SPEC REVISION REQUIRED: published-image proof `-13`** |
| External Postgres, exactly two serving roles, owner-only migration, fail-closed security readiness; REQ-154/156/157/158, AC-035 | SQL bootstrap, migrations, `SchemaCheck`, `wyrd-server migrate` | Recorded startup and SQL lanes | **FAIL: `-17`, `-18`, `-19`, `-20`** |
| Local calls and peer mTLS, receiver-owned trust checks, reachable discovery, no partial remote results; REQ-159/160/161/162/163, AC-036 | Peer listener, authority, membership and tail code | Recorded peer 9/9 and Oracle 28/28 | **FAIL: `-15`, `-16`, `-22`** |
| Bounded kind HPA proof from pinned published image; AC-037 | Kind manifests, read-rate adapter, script | Recorded local HPA one-to-two run and remote counter evidence | **SPEC REVISION REQUIRED: published-image proof `-13`** |
| Deleted tickets and policy hook; active architecture matches approved trust model; REQ-164/165/166, AC-038 | Deleted hook/route/ticket code; updated docs | Startup 404/OpenAPI checks and delegated journeys | **FAIL: stale authority `-14`** |
| Setup error-log writer follows mandatory Rust documentation rule | `Tee<W>` in server main | Rust lints recorded passing; rule requires per-item docs | **FAIL: `-21`** |
| Explicit task non-goals remain excluded | Cumulative diff and current owners | Task and domain inspections | PASS |

## Wave results and validated findings

| Review | Result | Source findings |
|---|---|---|
| Task implementation | FAIL | `TASKREV-006-R3-001` |
| Repository standards | FAIL | `STDS-001`–`STDS-005` |
| Security domain | FAIL | `SEC-01`–`SEC-03` |
| Data domain | FAIL | `DATA-R3-1`, `DATA-R3-2` |
| Peer domain | FAIL | `PEER-1` |
| Deployment domain | FAIL | `DEPLOY-1` |
| Eval domain | PASS | None |
| Structured Ponytail validation | COMPLETE | Ten retained findings, duplicates merged; `-13` requires a spec decision |

The authoritative ledger is [findings-validation.md](findings-validation.md). It records the source IDs, reachability and caller traces, exact locations, consequences, smallest safe corrections, and focused proof for `FIND-TASK-006-13` through `FIND-TASK-006-22`. No rejected finding is carried into remediation.

## Prior-finding closure and verification limits

`FIND-TASK-006-1` through `-10` remain closed by the cumulative candidate and prior R1/R2 evidence. `FIND-TASK-006-11` closes with removal of the test fixture's raw-pool forwarder; `FIND-TASK-006-12` closes with real-server negative media records. The original 256-entry tail-ticket replay failure no longer occurs in the recorded full server journey.

This audit read source, the complete diff and recorded gate results; it did not rerun Docker, kind, Postgres or broad journey lanes. No published digest exists to exercise. The reported passing lanes also do not exercise wrong client certificate identity, foreign-tenant pre-binding audit, wrong database logins, mutated grants/policies, concurrent migration lease, or the pre-bind membership window. `git diff --check f500ea38b..2fbe90cd8` passed.

**Decision needed:** Revise AC-034/037 to accept the official-recipe image pinned by an immutable local image ID for pre-release task acceptance, with published-digest proof at release; or separately authorize image publication and retain the current criterion. Do not substitute a local tag for published-image evidence without that decision. Package the nine bounded corrections after the approved criterion is settled.

**Post-verdict resolution (2026-09-25):** The user approved the local-image-ID criterion. Approved [spec revision 41](../../spec.md) records it, and [TASK-006-R3-startup-trust-closure.md](TASK-006-R3-startup-trust-closure.md) packages findings 13–22 plus the three requested server startup documentation journeys. The verdict above records the immutable revision-40 review; its spec blocker is now resolved for implementation.
