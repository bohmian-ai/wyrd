# TASK-001-008 R9 and R10 cumulative re-review verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Review base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Cumulative candidate: `41e60be61958c92f562fceb5b7a03f40f611bbc8`
- Approved specification: `changes/active/admin-principals/spec.md`, revision
  14, SHA-256
  `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Reviewed remediation tasks:
  - `changes/active/admin-principals/review/whole-branch-09/TASK-001-008-R9-close-validated-findings.md`
  - `changes/active/admin-principals/review/whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`

The candidate remained unchanged through both review waves. The approved
five-minute stateless-JWT revocation window was excluded from review findings.

## Verdict

**FIX_REQUIRED**

R9 closes CLI secret arguments, successful delegation credential attribution,
and the principal issue/list/revoke UUID surfaces, but one issue-key response
still weakens its credential ID to a string and one live CLI guide still uses a
removed secret argument. R10 has the correct RFC 8693 `sub=A`, `act.sub=B`
claim model, audience binding, attenuation, and database-free verification,
but delegation remains disabled in production, its audit and proving journeys
have material gaps, and its Python and TypeScript results cannot yet perform a
delegated Bifrost operation.

Remediation task:
`changes/active/admin-principals/review/whole-branch-11/TASK-001-008-R11-close-r9-r10-findings.md`.

## Acceptance matrix

| Obligation | Result | Evidence or remaining finding |
|---|---|---|
| R9: no shipped CLI secret argument or debug exposure | PASS in code | Root parser and ambient-source proofs pass; stale live guide is `FIND-admin-principals-R11-6` |
| R9: successful delegation audit names the authenticating credential while delegated JWT has no `cid` | PASS | Existing exact Postgres proof passes |
| R9: credential IDs remain UUID typed across issue/list/revoke and public contracts | FAIL | One card-bound issue-key response remains an unconstrained string: `FIND-admin-principals-R9-2` |
| R10: RFC 8693 subject/actor claims and nested actor order | PASS | Current Rust proofs establish `sub=A`, outer `act.sub=B`, and nesting |
| R10: production-usable token exchange | FAIL | Preview gate makes exchange unreachable in production: `FIND-admin-principals-R11-1` |
| R10: truthful, canonical, fail-closed delegation audit | FAIL | Policy permission is mislabeled and native-ingest actor attribution is dropped: `FIND-admin-principals-R11-2`, `FIND-admin-principals-R11-5` |
| R10: directed A-to-B invoke policy | FAIL | Current journey installs an allow-all hook and never proves reverse denial: `FIND-admin-principals-R11-3` |
| R10: Rust, Python, and TypeScript expose one useful Rust-owned helper | FAIL | Foreign delegated clients cannot reach Bifrost and lack public journeys: `FIND-admin-principals-R11-4` |
| R10: audience binding and database-free request verification | PASS | Bifrost HTTP/gRPC and Wyrd-route evidence passes |
| R10: authority attenuation | PASS | Exact, schema, wildcard, object, and disjoint intersection proofs pass |
| R10 removal/non-goals | PASS except stale preview surface | No delegation role/table/permission, second issuer/verifier, audit path, migration, compatibility route, or duplicated foreign exchange exists |
| Repository Rust documentation contract | FAIL | Diff-scoped R9/R10 private/test contracts remain incomplete: `FIND-admin-principals-R11-7` |
| Repository authority synchronization | FAIL | TypeScript guidance contradicts the approved shared-client projection: `FIND-admin-principals-R11-8` |
| Recorded task verification | PASS for covered behavior | Exact selectors and required narrow lanes are recorded; they do not cover the retained findings |

## Wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | `TREV-WB11-1` through `TREV-WB11-5` |
| Repository standards | FAIL | `REPO-R9R10-1` through `REPO-R9R10-4` |
| Auth/security domain | FAIL | `AUTH-1` |
| SDK/contract domain | FAIL | `SDK-1` |
| Bifrost domain | FAIL | `BIFROST-01` |
| Credential/CLI domain | FAIL | `CRED-CLI-1` |
| Ponytail validation | COMPLETE | Nine retained findings; aggregate-gate proposal rejected |

## Validated finding ledger

| Finding | Status and class | Required outcome |
|---|---|---|
| `FIND-admin-principals-R9-2` | REVISED · INCORRECT | Keep `IssueKeyResponse.key_id` UUID typed and prove the served schema |
| `FIND-admin-principals-R11-1` | REVISED · MISSING | Remove the obsolete preview gate so standard exchange works in production |
| `FIND-admin-principals-R11-2` | CONFIRMED · INCORRECT | Record `invoke` as the evaluated permission while retaining the exchange operation |
| `FIND-admin-principals-R11-3` | REVISED · MISSING | Prove allowed A-to-B and denied B-to-A with two valid token pairs |
| `FIND-admin-principals-R11-4` | REVISED · MISSING/VIOLATION | Let Python and TypeScript delegated clients compose with their existing Bifrost facades and prove both journeys |
| `FIND-admin-principals-R11-5` | REVISED · INCORRECT | Preserve the delegated actor chain in native-ingest authorization audit |
| `FIND-admin-principals-R11-6` | REVISED · REGRESSION | Remove the obsolete `--token` example and document ambient credentials |
| `FIND-admin-principals-R11-7` | REVISED · VIOLATION | Complete the R9/R10 diff-scoped Rust documentation contracts |
| `FIND-admin-principals-R11-8` | CONFIRMED · VIOLATION | Align TypeScript guidance with the approved thin shared-client projection |

Full reachability traces, rejected alternatives, exact locations, and closure
proofs are preserved in `findings-validation.md`.

## Prior-finding closure

- `FIND-admin-principals-R8-2` is closed in production code; its stale guide is
  the new documentation regression `FIND-admin-principals-R11-6`.
- `FIND-admin-principals-R9-1` is closed: successful delegation retains the
  actor credential only in audit and not in the delegated JWT.
- `FIND-admin-principals-R9-2` remains open only for the independently
  reachable card-bound issue-key response.
- `FIND-admin-principals-R10-1` is substantially corrected, but R11-1 through
  R11-5 identify reachable gaps in production availability, audit, directed
  policy proof, SDK usability, and native-ingest attribution.
- The retained audit-publisher `FOR UPDATE NOWAIT` correction remains in scope
  and unchanged.

## Verification limits

This was a read-only acceptance audit. Reviewers inspected the recorded R9/R10
evidence and reran only narrow checks where noted in their reports; the long
Postgres, identity, and Bifrost lane set was not repeated by the orchestrator.
The candidate and base diff are whitespace-clean. `mise run gate` is not
required because approved `VER-003` explicitly prohibits substituting or
requiring that aggregate.
