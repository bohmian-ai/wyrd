# Source alignment and existing-read boundaries

## Current alignment — revision 8, 2026-11-23

The approved [spec revision 8](../spec.md) and
[resumption design](resume-005-007.md) supersede the older permission/readiness
assumptions below. Consume authorization-model approved revision 9 and current
retained-parent architecture: Repository access covers its PR sets/executions/
evidence, while referenced definitions and GitHub configuration stay independently
authorized. TASK-003 is implemented. TASK-005/006 extend its inspector/navigation
and existing approval operations; TASK-007 first corrects the existing queue-free
PR/delivery status projection under REQ-021/AC-014. No new wire field or endpoint.

The resumption audit inspected HEAD `63ee31a0205c9b59a11877d5cf571d7bd0632cd6`;
the planning recheck confirmed the queue-free status limitation still exists.
Neither source inspection nor approval is fresh runtime qualification. The
implementation records its actual integrated revision and focused proof.

## Historical TASK-001A alignment

Updated 2026-11-16 with the TASK-001A integrated revision. The selected approach is
in [notes](notes.md#reopened-ui-only-contract-alignment-2026-11-12) and the
[field-to-existing-read map](contract-alignment-research.md). This is inspected
source and planning evidence; the TASK-001A revision below is its historical baseline.

## Source revisions

- Integrated baseline: TASK-001A merge `d118ee49f` of upstream
  `task/github-verifier-pipelines/TASK-012-integrate-code-mode` at `f6fac677c`
  into this branch at `a6a482306`, with compatibility repair `0d337a006`. This
  checkout now contains the resource-scaling compact/details split and the
  existing status/details/invocation/publication/file/cancel/approval routes;
  the exact owners and upstream proof are in
  [TASK-001A evidence](../tasks/TASK-001A-merge-verifier-pipelines.md#upstream-inputs-for-task-002-in-the-integrated-baseline).
- Earlier inspection inputs (UI `9c38fdfc3`, resource-scaling `443a79337`,
  backend `54aa8f4a9`) are superseded by that baseline.
- Upstream TASK-012-R1 was still open at merge; uncommitted work in its worktree
  is not part of this baseline.

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
