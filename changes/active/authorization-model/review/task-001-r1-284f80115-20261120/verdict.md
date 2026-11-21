# TASK-001-R1: FIX_REQUIRED

Eight of ten original findings are closed. The authority cache still loses invalidations during cold/in-flight loads, and affected onboarding guidance remains incorrect. Independently validated consumer, CLI-proof and mandatory fixture issues also require bounded corrections. No specification revision or new auth mechanism is needed.

## Immutable subject and authority

- Candidate: `284f801159a391e92493c3072878fd644c9d173b`.
- Remediation base: `3139c08cb0b7c0194583a66af3ab31bef28dc14f`.
- Prior reviewed candidate: `c78ad7b5241fbbfb54e85e2390c6a694787debfc`.
- Cumulative base: `eeb3bbd4f891ae88b00ce1979810eb2a11d04871`.
- Reviewed diff: `git diff 3139c08cb 284f80115`, with cumulative source and prior validated evidence available.
- Contract: `SPEC-authorization-model`, approved revision 2; original `TASK-001-scoped-authorization.md` and `TASK-001-R1-scoped-authorization-corrections.md`.

The prior candidate and remediation base have identical implementation/documentation source in `crates`, `sdks`, `scripts`, `examples`, `mise.toml` and `docs`; later governing-document commits require no rebased patch correspondence. Supplied packet/evidence edits and the prior review directory remain uncommitted. See [subject.md](subject.md) for repository identity, navigation and constraints.

Governing authorities: root/nested AGENTS.md; `changes/spec-driven-development.md`; maintainer style and TESTING.md; `architecture/wyrd-security-posture.md` principal lifecycle, authorization/freshness, retained visibility and tenant boundaries; applicable `wyrd-design.md` runtime identity/Card lifecycle and `bifrost-design.md` public/distributed query boundaries. TASK-002 UI remains outside this review.

## Independent review and scope

- Fresh `r1_closure` performed the remediation closure/affected-path review and reconciled prior evidence in [findings-validation.md](findings-validation.md).
- Fresh `r1_source_validation` independently validated the shared-state/security failure topology and production cold-load trace in [focused-validation.md](focused-validation.md).
- Fresh `r1_consumers_validation` independently validated documentation, SDK approval consumers, CLI proof and fixture obligations in [consumer-validation.md](consumer-validation.md).

The additional reviewers were selected for the concrete unresolved concurrency boundary and newly proposed consumer/rule findings. Prior complete-task security, repository, lifecycle and behavior coverage carries forward for unaffected areas. No additional system, maintainer or broad domain pass is necessary: failure topology is traced by the focused cache validator and affected owner/consumer rules by the remediation reviewer and consumer validator. The orchestrator assembled these reports and did not validate its own findings. All selected reports are present.

## Reconciled acceptance matrix

| Existing contract / scenario | Implementation and strongest evidence | Result |
| --- | --- | --- |
| REQ-001 / AC-001–003 | Common compile_binding/PermissionSet; previous group/custom/exact/provider journeys; corrected target and scoped mutation assertions | PASS |
| REQ-002 / AC-001 | Existing local device/password anchor/refresh; real saved-login journey; scoped invitations | PASS behavior; prose failure under REQ-010 |
| REQ-003 / AC-002 | Unchanged workload family/root and two-release principal/actor attribution proof; dependencies remain valid | PASS, carried forward |
| REQ-004 / AC-003 | Settled-token correction tested; first-load and unknown-invalidation observation still lost | FAIL, FIND-2 |
| REQ-005 / AC-002–003 | Unchanged identity-only versioned claims, ceilings, delegation and accepted-work recovery proof; credential settlement preserves key bounds | PASS, cache enforcement qualified by REQ-004 |
| REQ-006 / AC-001 | Corrected retained direct reads, prior SQL/published/download/approval proof and new real-peer returned-data assertions; Python/TS approval consumers stale | FAIL consumer/proof, FIND-11 |
| REQ-007 / AC-001 | Existing GitHub config isolation/provider identity and retained disconnected-read proof unaffected | PASS, carried forward |
| REQ-008 backend / AC-001–003 | Corrected provisioning/MCP/target validation and prior access summaries; CLI new access dispatch lacks journey | FAIL evidence, FIND-12 |
| REQ-010 / AC-001–003 | Existing owners retained and typed projections updated; affected guidance and language consumers remain inconsistent | FAIL, FIND-8/11 |
| Mandatory changed-item rustdoc | Original concrete omissions corrected | PASS |
| Mandatory public test fixture rule | New scoped Card and peer journeys rewrite YAML | FAIL, FIND-13 |

REQ-009 presentation and new Access UI remain TASK-002 scope. Prior unchanged evidence is carried forward only where the reviewer checked that the correction did not invalidate its dependencies.

## Closure and retained findings

Closed: **FIND-TASK-001-1, -3, -4, -5, -6, -7, -9, -10**. Their production corrections, assertions and execution-evidence qualifications are recorded in the validation report's closure table.

Retained findings are recorded once, with source traces, requirement, trigger, consequence, classification and smallest correction, in [findings-validation.md](findings-validation.md#retained-findings):

- **FIND-TASK-001-2:** a notification during the first tenant load is discarded; an older snapshot can authorize after newer state was observed. Unknown invalidation can also be overwritten by an in-flight load. Incomplete correction of the original finding.
- **FIND-TASK-001-8:** local onboarding guidance still denies passwords/invitations; updated SDK snippets bypass required tested-source embedding. Incomplete correction of the original finding.
- **FIND-TASK-001-11:** existing Python/TypeScript approval journeys still expect approver-only success without parent visibility. Production refusal is correct; repair the consumers.
- **FIND-TASK-001-12:** the new CLI access-administration adapter lacks its required real-server journey. This is an evidence gap, not an assertion that dispatch is incorrect.
- **FIND-TASK-001-13:** the new scoped-editor and public peer journeys rewrite Card YAML instead of using mandated checked-in inputs.

The user supplied the stale guidance, stale language assertions and missing CLI-proof notes; these were independently validated against existing obligations, not presented as newly invented scope. The fixture violation was discovered during review. Requiring another same-family second-version peer test was rejected: prior exact-containment proof plus the real two-parent peer journey closes the original FIND-10. Its fixture correction remains FIND-13.

## Verification and wait accounting

Fresh exact authority tests passed **3/3**: the warm-entry observed-revision unit, same-second settled-token unit, and repository-Postgres settled-token outage test. Exact commands, run IDs, assertions and limits are in [Executed verification and limits](findings-validation.md#executed-verification-and-limits). The passing warm test does not cover the retained cold-load race. The outage test asserts unknown-identity refusal; successful human-refresh memory behavior is supported separately by the inspected recorded journey and unit assertions. Other supplied closure results and checks were inspected and carried forward, not all rerun here. `git diff --check` passed; final HEAD remained the immutable candidate. An independent comprehension check of this verdict and R2 found no new auth obligations; the returned packet preserves deletion/reuse options for examples and records exact existing journey selectors.

The [wait inventory](findings-validation.md#wait-inventory) accounts for affected permission acquisition, mutation target lookups, retained identity lookup and MCP reuse. Warm authorization remains memory-only; observation fencing adds no wait; administration/Card mutation lookups stay outside the defined hot paths; audits remain staged. The remaining correction must preserve these boundaries.

Benchmark qualification and TDD compliance reporting are excluded by explicit maintainer instructions. Neither is an acceptance requirement imposed by this review. Behavioral verification is **FIX_REQUIRED** for the independently validated issues above.

This is the first remediation re-review. Two original fixes are incomplete, two consumer/proof obligations were missed originally, and the new journey construction introduced one mandatory-rule violation. The two-unsuccessful-round convergence threshold has not been reached. The next bounded scope is the five findings above; do not restart discovery across closed areas without a specific affected defect.

## Returned remediation

[TASK-001-R2-close-observation-and-consumer-gaps.md](TASK-001-R2-close-observation-and-consumer-gaps.md) contains the existing requirements, correction owners, preserved behavior and focused closure proof. No implementation or next review is launched.

Skipped broad suite reruns and implementation changes. Risk: cold/in-flight permission loads can still serve known-obsolete grants until corrected; no new live exploit reproduction is claimed.
