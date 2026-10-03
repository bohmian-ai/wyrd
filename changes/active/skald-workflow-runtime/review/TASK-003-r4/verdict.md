# TASK-003-r4 verdict

**FIX_REQUIRED**

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `58d07d7260df1f022a721e720a28ea48e5096e35`
- Candidate: `cfd60f6442e4dbdfcbc57cfec0bde494a225ae89`
- Approved specification:
  `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task:
  `changes/active/skald-workflow-runtime/tasks/TASK-003-remote-client-and-public-gateway.md`
- Prior remediations and reviews: `TASK-003-r1`, `TASK-003-r2`, and
  `TASK-003-r3`
- Review directory:
  `changes/active/skald-workflow-runtime/review/TASK-003-r4/`

The human-approved 2026-10-03 addition of `pub model: ModelRef` to
`WyrdGatewayCall`, the R1 same-spec native-`401` wording correction, and the
approval of `spawn_blocking` for run-start configuration, client, and secret
reads are authoritative. Run-start preparation remains required because
Workflow loading must read no execution secrets.

The complete base-to-candidate range was reviewed. The candidate remained the
stated commit through discovery and independent validation. CodeGraph was not
available because the repository has no `.codegraph/` index.

This review was strictly source-only. No build, compile, test, lane, Cargo,
mise, pnpm, pytest, formatter, linter, test-listing, or other verification
command was run. The implementer's recorded evidence was assessed only for the
source paths it exercises. Review reports, this verdict, the remediation task,
and the requested review-directory commit are the only review mutations.

## Required independent reports

| Role | Report | Result |
|---|---|---|
| Behavior | `task-review-behavior.md` | FAIL |
| Invariants | `task-review-invariants.md` | FAIL |
| Repository standards | `standards-review.md` | FAIL |
| Maintainer | `maintainer-review.md` | FAIL |
| System resilience | `system-review.md` | FAIL |
| Security/credentials | `domain-review-security.md` | FAIL |
| Concurrency/cancellation | `domain-review-concurrency.md` | FAIL |
| Independent Ponytail validation | `findings-validation.md` | One bounded retained finding |

All required roles and reports are complete. No focused follow-up was needed:
the discovery claims did not conflict, and the repeated-remediation source and
both live call chains were fully traced. Six discovery reviewers identified
the same producer defect. The standards and invariants reviewers additionally
identified missing post-R3 language proof; validation correctly folded that
proof obligation into the same stable finding rather than creating a second
proof-only finding.

## Reconciled acceptance matrix

| Obligation | Source and recorded evidence | Result |
|---|---|---|
| REQ-024/031/045/046 and AC-019: exact shared create/get/cancel/wait facade, stable idempotency, direct snapshots, fixed polling, and drop semantics | `wyrd-client/src/workflow/remote.rs`; original and R3 recorded focused transport evidence | PASS |
| Approved model amendment and REQ-036A/038/039/043/INV-020: Prompt-derived model and immutable fallback, timeout, cancellation, and trace correlation on each public gateway call | Skald plan/route owner; `workflow/gateway.rs`; recorded dialect, isolation, timeout, and cancellation evidence | PASS |
| Corrected native `401` contract: one model POST, refresh before pending body collection, no replay, renewal-error precedence | `transport/http.rs::post_native`; R3 recorded pending/complete/truncated-body evidence | PASS — `FIND-TASK-003-2` closed |
| Safe native-error normalization and redaction | `workflow/gateway.rs`; recorded catalog/category and canary evidence | PASS — `FIND-TASK-003-3` closed |
| AC-011A/012: authenticated, bounded, typed fallback-header consumption and served OpenAPI contract | gateway policy, ingress, routes, PG and served-document records | PASS |
| REQ-058: route-first shared local dependency assembly, selected-secret-only resolution, retained Cards client, explicit native injection, and run-start blocking boundary | `workflow/{mod,local}.rs`, shared secret reader, Python retained owner; original/R1/R3 records | PASS except for the single-snapshot failure below |
| R3 `FIND-TASK-003-10`: one ambient `GlobalConfig` snapshot supplies selected external bindings and any client-less public-gateway client | `load_local_setup` loads once for `.workflow`, then `WyrdClient::from_global` loads again through `ClientConfig::from_global`; mixed client-less routes are reachable | **FAIL — `FIND-TASK-003-10` reopened/revised** |
| R3 closure proof after changing shared `Workflow::run_with` | Rust focused evidence is recorded; the required existing Python retained-client integration was not rerun or recorded | **FAIL — closure proof under `FIND-TASK-003-10`** |
| Repository owner, async, PyO3, import, rustdoc, secret, error, dependency, generated-artifact, and non-goal boundaries | Cumulative source and prior recorded evidence | PASS apart from the retained finding |
| No unapproved mechanism, check, file, setting, option, compatibility path, ingress, transport, cache, watcher, or harness entered the change | Cumulative diff and callers | PASS |

## Validated finding ledger

The complete independent dispositions, producer-to-consumer trace, rejected
alternatives, correction boundary, and closure proof are in
`findings-validation.md`.

| ID | Status/classification | Required outcome |
|---|---|---|
| `FIND-TASK-003-10` | REOPENED / REVISED — INCORRECT / VIOLATION | In the existing run-start setup owner, load at most one ambient `GlobalConfig` and derive both selected Workflow bindings and any client-less Wyrd client from it through existing `ClientConfig::from_global_with_env` and `WyrdClient::with_config`; retain the approved blocking boundary and record the existing Rust and Python closure proofs. |

No additional finding survived validation. `FIND-TASK-003-1` through `-9` and
`-11` through `-12` are closed in source. The missing post-R3 Python rerun is
mandatory closure evidence for `FIND-TASK-003-10`, not a new stable finding.

## Verification limits and prior-finding closure

The implementer records green focused and broader evidence for the remote
handle, public gateway caller, fallback ingress and served OpenAPI, native
error normalization, selected dependencies, Python context retention before
R3, native `401` renewal, boundary checks, formatting, and lints. Source
supports the closed prior findings.

R3's selected-dependencies test bypasses the failing producer path: its
external route receives a pre-parsed configuration and its gateway case uses a
retained client. The R3 command record also omits the required public Python
retained-client integration after changing `Workflow::run_with`. Those are
specific deficits attached to the retained finding, not generic verification
limits and not authority for this review to execute commands.

## Remediation route

`TASK-003-R4-use-one-run-start-config-snapshot.md` packages the independently
validated correction for `$wyrd-implement`. It reuses existing owners and adds
no product, public API, architecture, security, compatibility, cross-service,
persistent-data, concurrency-semantics, or proof-infrastructure decision.

The next task review must reassess the complete original base-to-new-candidate
range, the original task, all four remediation tasks, and all prior verdicts.

