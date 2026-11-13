# Source alignment and existing-read boundaries

Updated 2026-11-12 for the user-agreed UI-only revision. The selected approach is
in [notes](notes.md#reopened-ui-only-contract-alignment-2026-11-12) and the
[field-to-existing-read map](contract-alignment-research.md). This is inspected
source and planning evidence, not runtime verification or branch integration.

## Source revisions

- UI source: `9c38fdfc3`; existing shared graph/catalog/loaders/executors and Card
  routes. TASK-001's completion and prior review proof remain historical evidence.
- Resource-scaling: `443a79337`; compact results and explicit details, with full
  retained outputs/inputs still in Bifrost.
- Integrated backend inspected at `54aa8f4a9` in the
  `github-verifier-pipelines-task-004` worktree includes that contract split and
  existing status/invocation/publication/file/cancel reads. This does not establish
  their integration into this UI checkout.
- Approved backend contract revision 7 describes nonblocking computation versus
  background retention; its proposed implementation task is not delivery proof.

These replace the older 2026-11-10 research revisions as planning inputs. A
worktree name or task status never substitutes for the actual integrated inputs.

## Reuse and task ownership

| Existing owner | Input/output and task use |
|---|---|
| Card registry APIs | Load exact Verifier/Repository declarations from the Postgres-backed registry. TASK-002 reuses TASK-001's graph; browser components hold no database connection. |
| UI contract/loaders/catalog/view | Extend existing typed UI operations/props and component catalog. TASK-002 delivers missing runtime panels, not another transport/loader family. |
| Node/MCP executors and BFF | Adapt existing reads and host actions; keep credentials/transport with hosts. Existing UI status and Bifrost query operations are already available. |
| Bifrost verification results | Bounded result identity, verdict, correlations and timing queries for TASK-002 history and TASK-003 discovery. A result row alone does not prove selected outcome or unfinished execution state. |
| Bifrost execution inputs/outputs/events | Existing retained input context, judgment detail and accepted progress under current query/payload permissions. No new table, payload classification or browser event replay. |
| Existing execution status/details | Status and server-selected compact judgment; explicit completed check/provenance/step/evidence read. Read availability is independent; details is not a wait/progress API. |
| Existing invocation/publication/File operations | Bounded exact producer instances and output metadata; selected JSON/manifest/file access; normal host-held download. |
| Existing cancel/approval/delivery operations | Consume delivered authority/receipts/state only. Backend lifecycle, same-execution continuation and provider writes remain outside this packet. |

Running definition lookup can use scalar `verifier_card_uid` in retained execution
metadata and the existing HTTP/MCP Card UID read. The TypeScript Cards.get wrapper
accepts a versioned reference, not a bare UID: use a supported existing host
operation or retained exact CardRef. Missing host adapter/input is not a reason
for a new server endpoint. If no record is visible yet, show truthful availability.

## Required dependencies and limits

TASK-003 must establish existing authorized data for unfinished as well as terminal
rows and matching counts. TASK-005 must consume actual frozen admission membership
and history, including confirmed same-commit request semantics. Neither task may
add domain query endpoints or derive authority/membership in browser code. If the
existing surfaces lack the required data or scope enforcement, retain the exact
external dependency and proposed status rather than weaken the spec or implement
backend work here. Current tenant/Verifier restrictions and independent Bifrost
payload grants remain unchanged.

Session membership uses the selected execution's recorded snapshot, never group
text. Drift detail/history comes from recorded judgments/windows through existing
reads; current Card status cannot manufacture a past baseline. Exact citations
retain current trace permissions. Result success does not certify retained
visibility, and an approval decision receipt does not certify later completion.

## Visual and verification references

`mock/` remains the sole approved combined visual target, supported by the
preserved original canvases. Its existing checks are:

```bash
mise exec -- node changes/active/verification-workflow-ui/design/mock/build.mjs
mise exec -- node changes/active/verification-workflow-ui/design/mock/selfcheck.mjs
```

The earlier 28 browser checks are prototype evidence, not a new execution in this
planning pass. Revised tasks keep the existing UI unit, real-host/built-artifact
and production browser test owners, public setup and explicit visual acceptance.
No new tests/selectors or runtime success are claimed by this source map.
