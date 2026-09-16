# Cumulative TASK-002-R3, TASK-004, and TASK-003 Review Verdict

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `bbfdf35e26212b2a831bda5e31e1ef4433e41900`
- Candidate tree: `58f5c2acfe8da01f3e4b58363f38ce761626df35`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 8
- Original tasks: `tasks/TASK-002-converge-client-and-sdks.md`,
  `tasks/TASK-004-unify-bifrost-data-root.md`, and
  `tasks/TASK-003-close-repository-integration.md`
- TASK-002 prior verdict: `review/task-002-r2-f345cd8a4-review-01/verdict.md`
- TASK-002 remediation: `review/task-002-r2-f345cd8a4-review-01/TASK-002-R3-close-r2-review-findings.md`

The complete cumulative base-to-candidate range was reviewed. `HEAD` and its
tree remained at the identities above throughout both waves. Unrelated
working-tree changes under `changes/active/verified-change-contract/` were not
used as evidence or modified.

## Acceptance matrix

| Obligation | Evidence | Result |
|---|---|---|
| TASK-002 shared client and thin Rust, Python, and TypeScript SDK convergence | Shared `wyrd-client` owner, public projections, recorded language and boundary lanes | PASS |
| TASK-002 Cards, WyrdState, HTTP/gRPC, CLI, MCP, errors, and language journeys | Cumulative source and recorded focused/integration evidence | PASS except stale generated schemas below |
| TASK-002 generated contracts expose only current public Bifrost authority | Four generator-only types and six stale schema families remain public | FAIL — `FIND-TASK-002-19` |
| TASK-002-R3 rustdoc, imports, cumulative diff hygiene, and direct-write shutdown ordering | R3 source plus focused lifecycle tests and exact cumulative `git diff --check` | PASS |
| TASK-002-R3 completed failed-drain settlement is non-reentrant | Focused completed-call proof passes; rare cancellation-during-cleanup proposal rejected under current user scope | PASS; `FIND-TASK-002-17` stays closed |
| TASK-002 historical trailers | Owner explicitly accepted the 22 earlier trailers without history rewrite at the candidate | PASS; `FIND-TASK-002-18` stays closed |
| TASK-004 one locked root, removed legacy settings, restart recovery, replica identity, and no Forge spill path | Root/config/deployment owners and recorded focused/journey evidence | PASS except managed-child usability below |
| TASK-004 rejects an unusable managed path before role activation | `prepare` write-opens only the root lock and does not prove child writes | FAIL — `FIND-TASK-004-1` |
| TASK-003 bounded non-blocking Oracle audit staging | Connection use is capped, but tasks/events are spawned before the cap | FAIL — `FIND-TASK-003-1` |
| TASK-003 affected-code PR selection runs credible owning tests | Generic Rust PR matrix excludes its sole Linux Rust-test entry | FAIL — `FIND-TASK-003-2` |
| TASK-003 nightly runs complete non-credentialed correctness | Existing real-server Card, CLI, WyrdState, and language journeys are omitted; storage already runs through `gate` | FAIL — `FIND-TASK-003-3` |
| TASK-003 generated artifacts, legacy removal, tracked native-artifact inventory, and workflow separation | Cumulative source, static checks, and recorded evidence | PASS |
| TASK-003 final candidate-local and credentialed live-cloud evidence | No complete final-candidate lane matrix or candidate-SHA live-cloud run is recorded | NOT PROVEN — mandatory closure evidence, not a source-remediation finding |
| Non-goals and current user materiality scope | No merge, push, deploy, compatibility restoration, new architecture, or rare/internal-only remediation retained | PASS |

## Review-wave results

| Review | Result | Proposed findings |
|---|---|---|
| Task implementation | FAIL | 4 |
| Repository standards | FAIL | 3 |
| Client/contracts domain | FAIL | 1 |
| Stream lifecycle domain | FAIL | 2 after one retraction |
| Persistent-data domain | FAIL | 2 |
| Repository-integration domain | FAIL | 6 |
| Structured Ponytail validation | FIX_REQUIRED | 5 retained after deduplication and material-production/90% filtering |

Wave 2 rejected the rare settlement-cancellation interleaving, style-only
qualified paths, the expressly allowed Python package profile, stale prose as
a production finding, the dead protobuf-snapshot ownership proposal, the
future final-change-review proposal, and missing hosted/local runs as source
defects. The full dispositions are in `findings-validation.md`.

## Validated finding ledger

| Stable ID | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-002-19` | CONFIRMED | VIOLATION | Delete generator-only removed Bifrost types and their six stale schema families; regenerate surviving docs/contracts. |
| `FIND-TASK-004-1` | REVISED | INCORRECT | Make the existing root preflight prove ordinary write usability for each managed directory before activation. |
| `FIND-TASK-003-1` | REVISED | VIOLATION | Bound total pending Oracle audit ownership before spawning while preserving non-blocking reads and the one staging path. |
| `FIND-TASK-003-2` | REVISED | INCORRECT | Make the existing Linux `ci` job run `test:rust` after `check` when it is not already running `gate`. |
| `FIND-TASK-003-3` | REVISED | MISSING | Schedule the existing required non-credentialed journey owners from nightly. |

## Prior-finding closure

- `FIND-TASK-002-1` through `-16` remain closed.
- `FIND-TASK-002-17` is not reopened under the owner's material-production and
  realistic-usage scope.
- `FIND-TASK-002-18` is closed by explicit owner acceptance without history
  rewrite.
- `FIND-TASK-002-19` is new. TASK-003 and TASK-004 had no prior stable finding
  IDs, so their retained ledgers begin at `1`.

## Verification limits

- Wave 1 reran the focused R3 client tests, queue deferred-error test, CI
  selection checks, proto drift, metadata, and cumulative diff hygiene as
  recorded in the individual reports.
- Wave 2 independently traced every proposed finding through its complete owner
  and relevant callers but did not rerun the long aggregate, Postgres,
  language-journey, nightly, performance, or live-cloud workflows.
- TASK-003 still requires a complete passing final-candidate local matrix and
  successful candidate-SHA live-cloud jobs under REQ-064 and AC-022. These are
  mandatory acceptance evidence after source remediation even though their
  current absence is not a production-source finding.
- Stale routed audit-WAL prose remains a documentation cleanup limit under the
  user's production-defect-only review scope.

## Verdict

**FIX_REQUIRED**

Five bounded material corrections remain. One combined remediation task is
provided at `TASK-003-R1-close-cumulative-review-findings.md`.
