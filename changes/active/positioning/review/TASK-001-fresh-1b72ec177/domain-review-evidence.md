# Evidence-domain review: FAIL

## Subject and boundary

Repository `/Users/stevenforrester/Documents/GitHub/wyrd`; immutable full range
`443a79337fe33dfbfe9ccc64e01d10369d469131...1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`.
This is a fresh review against SPEC-positioning revision 5 and the original
TASK-001 at the candidate. No prior acceptance or findings were read or carried
forward. HEAD remained `1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`.

Reviewed the finance evidence from its synthetic PDF/input producers through
declaration/publication, pinned Agent workspace selection, authorized reads,
provider messages and citations, returned judgments, stored results, and public
execution details. Also reviewed the three-language and YAML journey consumers.
Application formatting/error scaffolding is outside this domain; the user's
explicit current direction to keep examples simple is respected.

## Authority and owner coverage

| Contract | Authority and inspected owner/source |
|---|---|
| Synthetic report/purpose/recipient/destination, actual PDF use | SPEC REQ-002–003, INV-002; `finance_service.{py,rs,ts}`, both policy PDFs, provider fixtures, actual-program journeys |
| Fixed versioned policy, historical identity | SPEC REQ-005, AC-004; `Cards::declare` in `crates/shared/wyrd-client/src/cards/handle.rs:293–323`, `Artifact::extract`, declaration graph publication and `bind_published`; original/revised Rust story |
| Extraction/storage ownership | `architecture/wyrd-design.md` extraction/verification sections; `Cards::upload_artifact` documentation/implementation at `handle.rs:343–384`, `wyrd-loader/src/extract.rs:207–227`, existing saga upload owner |
| Tool and Artifact authority | `architecture/wyrd-security-posture.md:130–159`; `VerificationEngine::attach` in `vala-verify/src/blocks/engine.rs:418–488`, `mount` at `:1013–1055`, `TenantEvidence::{artifact,artifact_file,inventory}` at server `hosted/evidence.rs:251–343`, `Invocation::call` at `hosted/tools.rs:192` |
| Durable evidence and separate readback | `architecture/bifrost-design.md:417–453`; `VerificationDetails`/`RequirementDetails`, existing `Execution::details`, Quickstart step 4, `stored-verdicts.sql` |
| Real SDK proof for each taught language | Root AGENTS Testing rule 1; SDK nested AGENTS journey rules; SPEC AC-001–005 and INV-005; finance journeys plus existing authoring journeys and inspection helpers |

Read root and relevant SDK AGENTS, shared review scope and task-review skill,
maintainer style, architecture map and the applicable architecture sections.
The root `.codegraph/` directory is absent, so navigation used Git and rg.

## Trace and acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| REQ-002/INV-002: supplied synthetic facts include purpose | All three Release shapes include purpose; report/recipient/destination are supplied application objects; prompts bind `inputs.purpose`, report and recipient | All three program fixtures distinguish external audit, bank marker, marketing and regulator purposes; public Quickstart discloses supplied facts and OpenAI exposure | PASS |
| REQ-003/AC-003: one Service, one Verifier, two assertions and policy Agent | Three code declarations construct `finance-release` → `release-check` with `recipient-authorized`, `follows-policy`, `destination-restricted`; Agent uses gateway `openai`, only `fs.read`, maximum three iterations | Rust journey inspects graph; all language stories inspect tool-bearing model requests and their policy read | PASS |
| REQ-003: actual stored PDF extraction | All declarations call existing `.extract(true)` and select `extracted/company-sharing-policy.pdf.json` at `/inputs/company-sharing-policy.pdf.json`; shared loader uses existing lopdf extraction | Provider fixtures refuse to return a proper verdict if the read lacks the extraction's `external-audit` line; report and policy citations are returned; program stdout assertions therefore depend on real extraction/read | PASS |
| INV-001/002: existing publication/read/grant owners | Existing Cards declaration publishes Artifact then binds its exact returned ref; engine rejects unpinned refs, selects exact files, and creates citable evidence slots; TenantEvidence checks Card read and stored digest; tool dispatch checks exact `tool:invoke` | Existing real-server journeys drive this path; no new permission, publication, storage, transport or identity resolver is added | PASS |
| YAML judges same policy/input scenario | YAML exact Artifact 0.1.0, same path, three checks, Agent and purpose binding; json slots intentionally omit code model schemas | Python `test_yaml_release_check_judges_like_the_code` uploads/extracts PDF, declares YAML, asserts four outcomes and three non-pass reasons | PASS |
| REQ-005/AC-004: reuse and changed exact identities | One state/declaration precedes four requests in each program; shared existing publication/version selection pins dependencies; update reuses normal declaration path | Rust `finance_release_policy_versions_remain_attributable` compares read sources, substantive regulator outcome change, exact old/new policy and Service/Verifier versions and old execution details | PASS for shared mechanism and Rust surface |
| INV-003: publication remains separate | Direct programs add no persistence wait; public query explicitly allows eventual visibility; details uses existing read route | Rust history test polls only explicit inspection, outside verify; public SQL selects execution and exact subject/Verifier UIDs | PASS |
| AC-005/INV-005 and AGENTS Testing 1: taught update procedure proved on Python and TS | Quickstart step 5 is common to all three language tabs and promises auto-versioned 0.1.1 plus changed regulator verdict | Python only tests explicit-version mutation refusal; TS never installs revised PDF/reruns with it; older authoring journeys use different two-Verifier/explicit-version graphs and inert `% revision 2` changes | FAIL, EVID-001 |

## Proposed finding

### EVID-001 — Python and TypeScript omit the taught policy-update journey

- **Classification:** MISSING (mandatory journey/acceptance proof).
- **Violated obligation:** SPEC REQ-005/006, AC-005 and INV-005, together with
  root AGENTS Testing rule 1 requiring real SDK journeys on every surface a
  user-facing capability ships. AC-005 specifically requires the actual-program
  journeys to reproduce the brief update follow-on.
- **Exact locations:** `sdks/wyrd-sdk-python/tests/integration/test_docsite_quickstart.py:214–228`;
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/finance-release.test.ts:68–102`;
  published procedure `docs/src/content/docs/get-started/quickstart.md:141–160`.
- **Source-proven scenario:** The Quickstart reader runs Python or TypeScript,
  overwrites the policy with the revised PDF, and reruns the same authored
  program. The page promises a new exact policy/Verifier/Service selection and
  a regulator request changing from failed to passed, with earlier executions
  retaining their original policy. Python's only revised-PDF case calls
  `WyrdState.declare(Artifact.from_path(..., "0.1.0", ...))` and expects a drift
  refusal. It does not rerun `run_quickstart`, accept automatic versions or
  inspect old/new executions. TS copies only the original PDF and its two
  tests have no revised-PDF or execution-details path.
- **Sibling/existing proof checked:** The Rust actual-program journey at
  `examples/tests/finance_release.rs:489–580` proves substantive update behavior
  and original execution details. Existing Python/TS authoring journeys prove
  explicit versions on a different multi-Verifier graph; their revised file
  appends `% revision 2` and does not change the policy judgment. They do not
  reproduce this published auto-versioned actual-program procedure.
- **Observable consequence and materiality:** The Python/TS green counts do
  not establish the required supported onboarding step or history readback.
  This is a missing explicitly required journey gate, not a claim that shared
  server pinning is broken or a speculative request for extra coverage. It
  affects two of the three first-class language paths the same page teaches.
- **Minimum correction:** Extend the existing finance actual-program journey
  in each missing language to replace the PDF using the existing revised asset,
  rerun its actual program, verify the changed regulator result and exact
  policy/Verifier/subject selection, and inspect a retained old execution after
  the update to establish its original selection. Reuse existing registry reads,
  SDK `Execution.connect(...).details()` and bounded test inspection helpers;
  do not change SDK/runtime behavior or normal example output to support tests.
  No new harness/dependency, error middleware or request-path wait is needed.
- **Correction burden:** Two bounded additions to already-running language
  journey owners. Deleting the update would remove approved behavior; Rust
  coverage cannot prove the language surfaces; a separate test framework or
  production change is unnecessary.
- **Focused proof:** Each extended existing journey runs against its real
  WyrdTestServer/Postgres with the existing policy-dependent provider fixture;
  it fails if the revised extraction is not used, the new policy does not
  change the regulator outcome, or historical identities retarget. Select only
  those journey tests plus narrow language checks for the changed test files.

## Limits and result

No Cargo/Postgres tests were executed by this reviewer; execution was reserved
to the orchestrator. Reported green counts were not treated as proof of absent
scenarios. This is source inspection of the complete boundary, not a live
provider-quality assessment. The release/tag is an explicit delivery
prerequisite, not an additional acceptance finding under revision 5.

No changed server authorization, SQL/RLS, persistent-state or concurrency owner
was found. The evidence boundary reuses existing owners rather than adding new
read/wait/persistence mechanisms; no added or moved hot-path wait originates
from this domain's changes. A separate sensitive-runtime specialist is not
required by the inspected paths.

**FAIL** on EVID-001. All other reviewed evidence-boundary obligations have
source-backed implementation/proof evidence as recorded above.

Skipped: runtime execution and live model accuracy. Risk: the published Python
and TypeScript update follow-on lacks its required actual-program journey proof.
