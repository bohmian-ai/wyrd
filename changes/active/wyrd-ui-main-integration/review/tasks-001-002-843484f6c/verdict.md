# Task review verdict

**SPEC_REVISION_REQUIRED** for TASK-001 and the combined subject. **TASK-002 is BLOCKED on required human acceptance**, with an independently validated empty implementation-finding ledger. Neither task is accepted.

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
- Base: `8651b96fd0c049ab5e63b0606c98326af1fc8848`.
- Candidate: `843484f6cceba1f48139349f66794966221c5d37`.
- Approved specification: [revision 9](../../spec.md).
- Original tasks: [TASK-001](../../tasks/TASK-001-distribution-login.md), [TASK-002](../../tasks/TASK-002-remove-old-ui.md).
- Scope: complete cumulative `git diff 8651b96fd...843484f6c`; approved planning/reference packet additions are context. This is the first implementation review, with no prior verdict or remediation IDs.

The candidate remained unchanged throughout review. Pre-existing working-tree changes to `crates/wyrd-spec/src/vala/trace/mod.rs` and deletion of `trace_summary.rs` were excluded and preserved. Review wrote reports only; no implementation, merge, push or deployment occurred.

## Authority and prerequisite resolution

REQ-006 requires retaining a configured confidential OAuth client and encrypted HttpOnly session cookies. Both `architecture/wyrd-design.md:660` and `architecture/wyrd-security-posture.md:237–242,257–263` require those properties. Baseline and candidate instead use `client.None()`, serialize the session credential directly into JSON cookies, and have a shared Rust client-identification owner that rejects Authorization headers. HttpOnly/Secure/SameSite attributes and browser-safe page data do not establish encryption or confidential client authentication.

This is inherited drift explicitly included in the task's acceptance obligation, not a newly introduced regression. The task assumes the prerequisite exists while restricting work to packaging, lifecycle and trusted auth forwarding. No existing compliant secret/key-lifecycle owner was found. Selecting cross-process client secrets, encryption keys and replica ownership inside a transport remediation would require a material security/scope decision. Return to `$wyrd-spec` to establish that prerequisite and its scope/dependency under the existing stronger authorities; do not silently weaken those authorities or turn this report into permission for an auth redesign.

The [focused follow-up](followup-review.md) resolved the apparent conflict between discovery reports, and [independent validation](findings-validation.md) corroborated it. This conflict determines SPEC_REVISION_REQUIRED even though two bounded findings also remain.

## Independent reports and completeness

| Fresh reviewer | Report | Discovery result |
|---|---|---|
| behavior-rev | [Behavior acceptance](task-review-behavior.md) | TASK-001 FAIL; TASK-002 human acceptance BLOCKED |
| invariant-rev | [Invariants acceptance](task-review-invariants.md) | TASK-001 FAIL; TASK-002 human acceptance BLOCKED |
| repo-rev | [Repository standards](standards-review.md) | Material standards PASS for both tasks |
| maintainer-rev | [Maintainer review](maintainer-review.md) | TASK-001 FAIL; TASK-002 PASS within scope |
| system-rev | [System resilience](system-review.md) | TASK-001 FAIL; TASK-002 PASS within scope |
| domain-rev, authentication/tenant trust boundary | [Security domain](domain-review-security.md) | TASK-001 FAIL; TASK-002 PASS within domain |
| followup-rev | [Focused authority resolution](followup-review.md) | RESOLVED |
| ponytail-rev | [Independent validation and final ledger](findings-validation.md) | Two retained findings; authority conflict confirmed; TASK-002 empty implementation ledger |

All required reports are present. [Claim comparison](claim-comparison.md) groups related causes and records why follow-up was necessary. The validator independently examined source and callers rather than voting on report agreement. No further discovery pass was required. The orchestrator has included only validated findings and the independently resolved authority conflict.

## Reconciled acceptance matrix

PASS denotes the stated bounded source/verification result, not proof of unexecuted deployments.

| Task / obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| TASK-001 REQ-001, AC-001: source-free native distribution/system Node | Existing build owner stages production UI/package/dependencies; selected native matrix, no bundled Node | Mac fixture uses checkout-linked UI; signed source-free launch/digests on three targets absent | FAIL: required acceptance evidence missing |
| REQ-002: valid signed atomic replacement including runnable payload | Signed staging retained; fresh eligibility accepts placeholders and cached eligibility bypasses UI | Existing installer assertions miss cached-invalid and meaningful payload cases | FAIL: FIND-TASK-001-1 |
| REQ-003, AC-002: readiness, first/repeat/failure/interrupt/child cleanup | Existing DevServer owns/readies/observes/settles both children; prerequisite checks precede launch | Supplied Mac first/repeat/partial and Python interrupt/UI-death checks pass; Linux native absent | PASS for demonstrated Mac paths; full native acceptance incomplete |
| REQ-004, native AC-003: actual BFF auth transport | Configured target, trusted origin, method/body/query/redirect/cookie/error preservation | Native identity now uses BFF public address; independent forwarder/session tests pass | PASS for transport; does not waive REQ-006 conflict |
| REQ-005, AC-004: official distribution/routing/failure handling | Debian amd64/arm64 and Rocky amd64, existing nginx/entrypoint/build/release owners extended | Static checks pass; no selected image/startup runtime identity; provider journey only native | FAIL: runtime proof absent and FIND-TASK-001-2 |
| REQ-006: confidential/encrypted existing login | Baseline/candidate lack required prerequisite; retained native session behavior otherwise exercised | Native green tests assert existing behavior, not required encryption/client authentication | FAIL: SPEC_REVISION_REQUIRED |
| INV-001/002: contracts/server ownership, tenancy and credentials | No introduced durable contract/client-authority migration; safe page metadata retained | Existing auth/refusal regression evidence | PASS within changed boundaries; security authority conflict separately unresolved |
| INV-003–006, AC-005: build/target/scope exclusions | One build/lifecycle/session owner; no deferred domain integration, analytical addon, prohibited target or test-only selector | Diff/manifests, supplied dependency checks | PASS |
| REQ-020, INV-007, AC-010: provenance/credible proof/inspection | Reference packet preserved; task evidence states runnable checkpoints | Required native/nginx human inspection and platform evidence absent | FAIL: acceptance incomplete |
| TASK-002 REQ-032 delete inventory/mock removal | Specified old component/feature/mock/domain routes and switches removed, surviving consumers closed | Static inventory and independent retained unit suite | PASS |
| TASK-002 retained auth/chooser/login/recovery/home/settings/errors | Plain branded markup, safe relocated types, current session owners retained | Supplied check/build/identity regressions; independent unit run 27 pass | PASS for source/regression obligations |
| TASK-002 no new package/shell/domain integration | No replacement shared components or dependency change | Complete diff/manifests | PASS |
| TASK-002 REQ-020 human checkpoint | Pages runnable through retained/current launch owners | No recorded state/mode/responsive/keyboard/touch human inspection | FAIL: BLOCKED pending human acceptance |

Independent validation corrected the invariant report's suggestion that native child-death proof was wholly absent: the changed Python exit case kills the UI and checks both listeners, and supplied evidence records its pass. Repository and maintainer PASS language is bounded to their scopes and does not certify the stronger security or missing deployed evidence.

## Validated finding ledger

The complete producer/caller/sibling tracing, selected correction boundaries and focused closure commands are in [findings-validation.md](findings-validation.md).

| Stable ID | Sources | Status / classification | Obligation, source and consequence | Selected correction and proof |
|---|---|---|---|---|
| FIND-TASK-001-1 | BEH-001, INV-001, INV-002, MR-001, SYS-001 | REVISED / INCORRECT | REQ-002/AC-001; `server/install.rs:190–249,436–455`, `tests/server_install.rs:50–58`: binary-only cached versions bypass UI validation, and fresh signed empty markers pass; an unusable release can replace current and advertised reinstall loops | Existing installer-owned concrete production-payload eligibility must govern fresh/cached activation and installed lookup. Explicitly refuse incomplete cache before switching current, with truthful recovery and healthy cache reuse. Use concrete adapter-node contents and realistic positive fixture; keep Node compatibility/load/start at existing runtime/build proof. Test cached-invalid/current preservation, signed placeholders and missing support payload; separately prove source-free accepted UI launch. No install-time Node prerequisite, generic resolver, new lifecycle owner or rollback framework. |
| FIND-TASK-001-2 | SEC-001 | REVISED / MISSING | REQ-006/AC-004; `startup-common.sh:56–67`, `identity_ui_e2e.rs:112–145,485–490`: API-key/device-page probes and native fixed-ORIGIN identity do not exercise official nginx provider callbacks/session renewal/logout | Reuse existing managed identity fixture/driver and image/startup owners for one focused federated login/return/renewal/logout plus relevant refusal through actual official nginx ingress. Record image/source identity and run focused deployed proof on supported Linux. Do not duplicate the full suite, route around ingress or infer runtime breakage from missing proof. |

TASK-002 has an explicitly validated empty implementation-finding ledger. No FIND ID is assigned to inherited security drift, human-directed requirements, missing execution alone, task-title naming or packet whitespace. Prior-finding closure: N/A. Related installer symptoms are one cause and one correction boundary.

## Verification limits and disposition

Independent discovery execution: UI unit suite 8 files/27 tests passed; focused upstream/session tests 2 files/11 tests passed; changed shell syntax and Python formatting passed. Those results remain attributed to their reports. A focused Cargo invocation began a cold build and was stopped before execution; no test pass is claimed. Supplied Mac packaging/native/identity/fmt/lint/dependency/docs results were examined alongside their proof owners, not all rerun.

Unverified: Linux native x86_64/arm64; real signed source-free bundles/digests including Darwin; Debian amd64/arm64 and Rocky amd64 image startup/routing/cleanup and immutable runtime IDs; provider identity through nginx; human visual checkpoints. Full cumulative `git diff --check` also reports packet whitespace; implementation-only diff is clean. No cross-platform, security-property or visual pass is inferred from Mac fixtures or static checks.

No remediation task is emitted: the controlling verdict is SPEC_REVISION_REQUIRED, not FIX_REQUIRED, because the security prerequisite/scope must be resolved first. The two bounded recommendations remain recorded for subsequent authorized correction. This review does not implement them. TASK-002 remains unaccepted pending human evidence, without inheriting an installer or security regression finding.
