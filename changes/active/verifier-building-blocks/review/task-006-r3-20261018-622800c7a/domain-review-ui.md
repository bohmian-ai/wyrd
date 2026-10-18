# UI and reference-consumer domain review

Result: **PASS**. No material proposed findings in this domain. FIND-TASK-006-4 is closed on the reviewed source.

## Immutable subject and authority

Reviewed detached source S `/tmp/wyrd-task006-review-622800c7a`, exclusive base `b323327f4cbb690628303d3a6b5249a67af50c9b` through candidate `622800c7ad18c8b28cb5a2ea84cd568e2a085cb6`. Read cumulative `git diff BASE...HEAD`, using the R2-to-candidate range only to locate the final reference deletions. No `.codegraph` exists in S; used Git, rg and direct source reads. No peer report from this round was read.

Authorities inspected: AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development and maintainer-style guidance; Wyrd design doctrine/client/Verifier boundaries, doctrine Verifier/public-surface boundaries, domain/evaluation.md; wyrd-task-review and wyrd-ui skills, UI SvelteKit/testing guidance and brand DESIGN.md. Applied approved spec r11 REQ-017–022, INV-008 and AC-009/011/016, original TASK-006, prior R1 and R2 diagnoses/remediation/evidence. The product README's styling is superseded, while route/IA/state coverage remains active; that distinction governs this review. No external technical/version claim or live source lookup was necessary.

The relevant owners are the existing Svelte server seam `src/lib/server/wyrd.ts`, Observe routes/features/mock projectors, Cards/Service fixture owners, and `brand/renders/product/`. No new owner or framework is introduced. The independently inspected runtime change removes the old product; the final remediation is five reference files only.

## Coverage and result matrix

| Obligation/boundary | Source and consumer evidence | Proof assessment | Result |
|---|---|---|---|
| Retire evaluation routes and projection ontology without aliases | Both inventory/detail `observe/evaluations` route pairs deleted; ObserveNav, SIGNAL_KEYS, Observe types and mock fixtures/projectors remove Eval rows/tasks/details. WyrdClient imports/methods no longer expose observeEval(s). Cumulative source search finds only the intentional negative assertion `/observe/evaluations` in ObserveJourney.test.ts. | Source inspection; supplied original UI check (0 errors/724 files) and 178-test run, not freshly rerun. | PASS |
| Keep surviving navigation and current Card destinations | Observe overview drops obsolete tile/attention/recent links; home fixture uses current Drift; Service investigations are Logs/Metrics/Traces/Drift. checkSignal and composition/definition/relationship references agree on `card_verifier_03`, present as checkout-agent-check in Cards fixtures. Changes fixture key/name consumers and tests use the same verification gate. | Traced fixture producer, Service view model and rendering/test consumers; runtime route removal is structural, not inferred from a search alone. | PASS |
| Preserve authorization and tenant projection | WyrdClient.observeAccess now selects bifrost_query:read or drift:read, checks permission before mock availability, and gates populated fixture data to acme. observeDrift clears report for other tenants. Telemetry list/detail paths retain their no-match/notFound behavior. LocalSessions authorizes membership by tenant key plus immutable tenantId and intersects issued/current grants; both dev memberships replace evals:read with drift:read. Tenant layout returns selected tenant identity. | Source/call-path review, including session and tenant layout; existing ObserveJourney negative/other-tenant assertions inspected. No new client-side durable or tenancy owner. No live auth/session execution claimed. | PASS |
| Prior R1 reference retirement stays closed | C-10, O-08/O-09 and M-06 absent; golden-O-09 removed. Retained Service and mobile investigation links use four signals and current Verifier Card. Cumulative reference SVG/XML review found no recreated evaluation inventory/detail/task drawer. | Current XML/source inspection plus original R1 supplied evidence. | PASS |
| R2 removes the producer of the unsupported declaration/result model | cards.svg has no C-08 either theme; Experiment outputs has no E-17 either theme. README removes their ledger/state/route/pattern entries. No retained product SVG contains section=evaluations, scored-task/evaluation-summary/canonical-Evaluation-run instructions, C-08 or E-17 references. | Fresh XML parse and semantic text review across all product SVGs, README and current VerifierSpec/design. Absence alone was not used to establish semantic correctness. | PASS |
| E-07 and its direct destination consumers close together | E-07 retains sessions/tool calls/tokens/cost/p95/errors, labeled stage times, files/metrics/traces and current Verifier quality PASS; removes six-task/aggregate-task result summary, score and canonical evaluation record instruction. E-04/E-07/E-16/EM-02 retain Summary/Metrics/Files/Provenance/Traces destinations with no Evaluations destination. E-16 still links canonical trace detail. | Source plus fresh rendered light/dark pairs of all four retained artboards. | PASS |
| Counts/IDs and sheet compaction remain coherent | cards24 artboards, Experiment outputs16, Experiment package40, general76, all154 (77 pages), mobile32 artboards/16 pages and desktop122/61 pages. README agrees. Comparing R2 source IDs, only C-08-light/dark and E-17-light/dark disappear; no new IDs, runs/mobile IDs unchanged. | Fresh ElementTree parse of every product SVG and actual group IDs; golden duplicates excluded from page totals. | PASS |
| Retained artboards remain readable after deletion/reflow | E-04/E-07/E-16/EM-02 both themes: labels/sections fit, stage chart and output links remain readable, no orphan controls observed. First shifted cards page C-09 and final C-14 both themes retain full content and annotation strips within sheet. | Six fresh offline Chrome screenshots viewed, paths/procedure below. | PASS |
| Scope exclusions held | General C-01 kind/filter, C-14/Service Workflow dispatch and E-07 review-loop Workflow link are not dependencies of the deleted evaluation panel; expressly excluded by R2. Separate UI-foundation packet is excluded. Ordinary offline_evaluation run kind, evaluation descriptive prose, metrics/provenance/output content remain. | Source semantics checked against explicit R2 preservation/exclusions. No proposal to perform general Workflow cleanup or add replacement UI. | PASS |

A retained C-04 read-only linked-verification summary says “3 of 5 tasks passed” and links the current Verifier Card; it does not declare the deleted Workflow/task schema or promise a removed result route. I do not turn that generic summary wording into a new blocking ontology finding. The actionable producer and its direct consumers identified in FIND-4 have been removed.

## Fresh checks and visual evidence

All executions used explicit S workdir. Fresh successful checks:

- `git diff --check b323327f4cbb690628303d3a6b5249a67af50c9b...HEAD`.
- Python standard-library ElementTree parsed every product SVG, enumerated actual `*-light` / `*-dark` artboard group IDs, asserted per-sheet uniqueness, summed sheet/package totals, and compared final edited sheet IDs with `git show d77214d8eb119fc26ff114f4ef4a403bc3d3e426:<path>`.
- Independently inspected XML text and README for retired model instructions, links, ledger/state/pattern entries and surviving current destinations. This was review exploration, not a permanent scanner or lexical acceptance gate.

Created only temporary cropped SVG copies in `/tmp/task006-r3-ui-fresh/` using ElementTree to change the outer width/height/viewBox; all original child content remained unchanged. Crop origins came from actual artboard transform coordinates: E04 `(96,1284)`, E07 `(96,4868)`, E16 `(96,8452)`, EM02 `(64,1204)`, C09 `(96,9266)`, C14 `(96,14586)`. Desktop paired viewBoxes are 3040×1220; mobile pair 860×1000. Fresh rendering command, repeated per named file:

```bash
mise exec -- /usr/bin/google-chrome --headless --no-sandbox --disable-gpu \
  --disable-background-networking --host-resolver-rules='MAP * ~NOTFOUND' \
  --hide-scrollbars --window-size=3040,1220 \
  --screenshot=/tmp/task006-r3-ui-fresh/E07.png \
  file:///tmp/task006-r3-ui-fresh/E07.svg
```

Mobile uses `--window-size=860,1000`. All six commands exited 0. Viewed current candidate screenshots `/tmp/task006-r3-ui-fresh/{E04,E07,E16,EM02,C09,C14}.png`, each containing the retained light and dark artboards. No source artifact/image was written into R or S; R contains this report only from this reviewer.

## Findings and limits

Material findings: **none**. Verdict **PASS** for this coherent UI/reference boundary, not an integrated task/change approval.

Skipped fresh runtime builds, UI interaction/accessibility suites and backend/journey aggregates per the immutable review scope; supplied historical GREEN results are not fresh proof. Offline screenshots use fallback fonts because remote font loading is blocked, and do not certify every pixel of every sheet, production typography, keyboard operation or runtime interaction. Candidate HEAD remained pinned to `622800c7ad18c8b28cb5a2ea84cd568e2a085cb6`; final `git status --porcelain` in S was empty.
