# Skald workflow runtime change re-review, round 2

## Verdict

**PASS**

Target `3d2295f1a0804d388592518054968f95739a3d01` satisfies approved
`SPEC-skald-workflow-runtime` Revision 14 and closes all twelve findings named
by the human-directed follow-up task. The remediation introduces no material
regression in the integrated workflow runtime.

This was a source- and recorded-evidence-only review. It ran no build, test,
lint, formatter, generator, package, server, or executable command. No required
reviewer was unavailable. The reviewer was fresh relative to implementation
and integration, so the review skill did not require delegation.

## Immutable subject

- Branch: `wyrd/skald-workflow-runtime/followups`
- Base: `7d96c30066425e0cde2290842d5801307843283d`
- Target: `3d2295f1a0804d388592518054968f95739a3d01`
- Base relationship: supplied base is the target's merge-base with `main`
- Approved specification: Revision 14 at
  `395a3ad91:changes/active/skald-workflow-runtime/spec.md`
- Round-1 review: commit `079794988`,
  `changes/active/skald-workflow-runtime/review/change-r1/`
- Follow-up authority and evidence:
  `changes/active/skald-workflow-runtime-followups/task.md`
- Delivery reference: not supplied

`.codegraph/` is absent, so review used immutable Git objects, repository
search, the complete follow-up diff, and direct source/caller inspection.

## Material findings

None.

## Twelve-finding closure

| # | Finding | Target closure | Recorded proof | Result |
|---|---|---|---|---|
| 1 | `FIND-TASK-004-25` | Deleted unused `CacheKey::from_request`, its tests/helpers, now-unproducible cache errors and runtime wrapper, and unused dependencies. No request-only derivation remains. | `test:skald` 332/332 and dependency hygiene recorded in the follow-up evidence. | PASS |
| 2 | `FIND-TASK-004-24` | `Prompt::bind_media_mut` captures `Prompt::provider()` before the mutable request borrow and supplies it to the shared GenerateContent binder. | Exact `google_media_matrix_and_rejections` test records Google and Vertex refusal assertions. | PASS |
| 3 | `FIND-TASK-004-17` | Completion removed the conflicting active Revision 13 task authority; the lasting record identifies approved Revisions 13 and 14 correctly. | Historical packet and current completed record inspected. | PASS |
| 4 | `FIND-TASK-004-26` | `ProviderResponse::provider` now documents schema default; `invoke_agent_span` documents effective Prompt dispatch destination. | Recorded lint lane and direct source inspection. | PASS |
| 5 | `FIND-TASK-004-27` | `Prompt::new`, `vertex`, `google_prompt`, and `finalize_prompt` have substantive rustdoc and `# Errors`; destination save/restore is explicit. | Recorded lint lane and direct source inspection. | PASS |
| 6 | `FIND-TASK-005-1` | `wyrd workflow run/status/cancel` expose no `--server`; every client is built through ambient configuration. Other CLI command flags remain unchanged. Architecture and workflow how-to agree. | Four compiled CLI journeys and docs check recorded. Parser rejection is present in the journey. | PASS |
| 7 | `FIND-TASK-005-2` | Execution/file/detach compatibility is decided before `self.input()` is called. | Compiled CLI contract records both invalid modes with a missing input file. | PASS |
| 8 | `FIND-TASK-005-3` | Both Unix `PermissionsExt` imports moved to module import blocks under `#[cfg(unix)]`. | CLI and Rust SDK journeys plus lint evidence recorded. | PASS |
| 9 | `FIND-TASK-005-4` | The existing `ctrl_c()` select arm distinguishes `Ok(())` from listener failure; failure maps through `WyrdError::WorkflowInternal` with boundary `signal_listener`. | Existing SIGINT journey remains green; compile/lint evidence is recorded for the static error branch. | PASS |
| 10 | `CHANGE-R1-DRIFT-001` | Deleted the successful-response credential substring scanner, constants, recursive JSON scan, refusal behavior, and scanner-specific tests. Added nothing in its place; ordinary sensitive headers and error/decode redaction remain. | Exact decode-redaction test and `test:skald` recorded. | PASS |
| 11 | `CHANGE-R1-INCORRECT-001` | The incorrect `new Cards()` example existed only in the retired active specification. Current TypeScript source contains only the SDK's private constructor implementation; public loading uses `Cards.connect()`. | Repository search and completion history inspected. | PASS |
| 12 | `CHANGE-R1-MISSING-001` | Served-OpenAPI integration test asserts POST create, GET run, and POST cancel under `/v1/workflow-runs*`. | `test:principals:integration` records `pg_openapi_contract` 20/20. | PASS |

## Integrated regression review

- The follow-up write set is limited to the twelve corrections, their focused
  tests/docs, dependency cleanup, current architecture, and the completion
  record. It adds no parallel owner, transport, parser, validator, cache
  derivation path, signal abstraction, credential scanner, option, setting,
  checker, or lifecycle protocol.
- Provider schema/destination separation remains Revision 14: one request
  variant per wire schema, optional `Prompt.provider`, Vertex as
  GenerateContent with destination `vertex`, and local Wyrd gateway refusal of
  Vertex.
- CLI endpoint resolution follows the human decision exactly: Workflow
  commands are ambient-only; other commands retain their existing endpoint
  flags. Invalid-mode refusal precedes input-file IO.
- Standard secret handling remains: sensitive request headers, screened and
  pinned egress, withheld refusal bodies, and withheld decode details. The
  deleted success-response scanner has no replacement.
- Removing the unused cache derivation also removes only errors that can no
  longer be produced. `CacheKey`, `CacheValue`, `PromptCache`, and
  `InMemoryCache` remain intact.
- The original shared Cards hydrator, one Skald executor, bounded accepted-job
  host, live authorization/audit owners, prepared deadline binding, and
  first-class SDK journeys are unchanged by the remediation.
- Oracle drain polling and idle refusal remain absent; follower release remains
  grant-stream close; the foreign-tenant harness is unchanged; Running still
  reserves attempt 1 and interruption settles Cancelled; query tools still
  bind the prepared run deadline once and honor a shorter `deadline_ms`.
- Rust and TypeScript journeys still register through Cards, the compiled CLI
  journey owns apply proof, TypeScript retains the Skald package cone, and the
  approved dev-only wiremock allowlist is unchanged.

## Task-review closure

| Work item | Closure at target | Result |
|---|---|---|
| TASK-001 through TASK-005 | Round 1 established target-bound closure for the original tasks at `395a3ad91`. The round-2 comparison confirms the follow-up commits do not alter their unrelated accepted behavior. | PASS |
| TASK-004 r6 retained findings | All five retained findings are closed by rows 1–5 above. | PASS |
| TASK-005 r1 retained findings | All four retained findings are closed by rows 6–9 above under the explicit no-`--server` decision. | PASS |
| Round-1 change findings | All three findings are closed by rows 10–12 above. | PASS |
| Follow-up remediation task | Its evidence table is complete, source-bound, and consistent with the target. | PASS |

## Verification limits

- Per instruction, this review ran no executable verification and relies on
  the recorded evidence in the follow-up task plus immutable source reading.
- The Ctrl-C listener-registration error is closed by source inspection and
  normal compile/lint evidence; no bespoke OS-failure injection mechanism is
  required or justified.
- The TypeScript example finding closed through retirement because the only
  bad example lived in the removed active packet; no TypeScript source changed,
  so no new TypeScript lane was required.

## Completion payload

- Intent: retain the shipped Skald Workflow runtime while closing every
  accepted round-1, TASK-004 r6, and TASK-005 r1 finding.
- Externally observable follow-up behavior: ambient-only Workflow CLI endpoint
  selection, pre-IO invalid-mode refusal, correct signal-listener failure,
  correct Vertex media attribution, standard external-gateway response
  handling, and served Workflow OpenAPI assertions.
- Lasting constraints: the original Revision 14 constraints and human
  decisions remain unchanged; no nonstandard mechanism was added.
- Material decisions: the twelve closure decisions in the table above and the
  human-directed Workflow CLI no-`--server` contract.
- Deviations: none remaining from the twelve-item follow-up ledger.
- Delivery reference: not supplied.

This PASS immediately routes to `$wyrd-complete` after the review directory is
committed.
