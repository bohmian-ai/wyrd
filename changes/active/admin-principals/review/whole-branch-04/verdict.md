# Admin principals whole-branch review 04 — verdict

## Immutable subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `96a1bd81b1d028fa81d6e0f385e3cd9b62080e30`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 10,
  status `approved`, SHA-256
  `05982825655110b7a514f16ffdd953e4b1e1df4b7422e9e77461f2e514a10837`
- Original tasks: `changes/active/admin-principals/tasks/TASK-001-*.md`
  through `TASK-008-*.md`
- Prior remediation:
  `review/whole-branch-03/TASK-001-008-R3-close-validated-findings.md`
- Validated ledger: `findings-validation.md` in this directory

The candidate and approved-spec checksum remained pinned through both review
waves. The dirty `README.md`, approved spec, and whole-branch-03 verdict are
owner state outside the immutable candidate and were preserved. The owner has
waived `FIND-TASK-001-10` in full and approved the bundled
verified-change-contract content; neither is drift.

## Verdict

**FIX_REQUIRED**

The remediation closed the twelve R3 implementation roots, including the
machine/human renewal split, replay durability, exact platform resources,
canonical issuer storage, secret redaction, narrow SQL capabilities, conflict
redaction, and duplicate OpenAPI deletion. It did not earn cumulative
acceptance. Three prior closure obligations remain open and thirteen new,
reachable roots were independently validated. No retained correction requires
a specification revision or a new subsystem.

## Acceptance matrix

| Obligation | Cumulative result |
|---|---|
| Principal/credential model, two administrative planes, tenancy, Card-free machine principals, and stable credential rejection | PASS |
| Initialization, provisioning, recovery, and operator lifecycle | FAIL — stdout disclosure ordering and incomplete all-stage provisioning proof (`R4-6`, `R4-8`) |
| Revocation and authorization epochs | FAIL — resolver uncertainty admits tokens, User refresh survives revocation, and changed OIDC roles do not revoke old claims (`R4-1`–`R4-3`) |
| Human refresh rotation and replay containment | FAIL — durable containment works, but replay audit loses the consumed row id (`R4-4`) |
| Canonical audit and attribution | FAIL — tenant and platform token/session issuance omit canonical same-grant audit (`R4-5`) |
| Retained audit compatibility | FAIL — implementation exists, but generic validation was widened and the required historical upgrade/restart journey is absent (`R3-5`, `R4-7`) |
| One complete `utoipa` OpenAPI contract at `/openapi.json` | FAIL — live routes/errors are absent and parallel route knowledge/source-only proofs remain (`FIND-admin-principals-13`) |
| Shared client transport and machine re-exchange | FAIL — the general shared client passes, but the first-party MCP adapter owns a second raw HTTP path without reactive renewal (`R4-9`) |
| CLI/operator projection | FAIL — ordering passes, but the journey bypasses the returned credential at the shipped CLI boundary (`R4-11`) |
| Architecture and public documentation match the five-kind, two-plane, split-renewal model | FAIL — active authority and public docs retain the removed model (`R4-10`) |
| Mandatory Rust documentation | FAIL — cumulative touched test/private items remain undocumented (`R4-12`) |
| Exact named verification and final diff hygiene | FAIL — exact command records are incomplete and the candidate diff has seven EOF whitespace errors (`R3-6`, `R4-13`) |
| Provenance and approved verified-change-contract scope | PASS by explicit owner decisions |

## Wave results

| Review | Result |
|---|---|
| Task implementation | FAIL |
| Repository standards | FAIL |
| Security/RBAC/auth | FAIL |
| Data/tenancy/durability | FAIL |
| Public contracts/CLI/MCP/SDK/docs | FAIL |
| Structured Ponytail validation | Completed; 16 retained roots, no specification revision required |

## Validated findings

| Finding | Classification | Required outcome |
|---|---|---|
| `FIND-admin-principals-13` | INCORRECT / VIOLATION | Make the one runtime `utoipa` document complete and exact; delete parallel route knowledge and source-only proof. |
| `FIND-admin-principals-R3-5` | MISSING | Add the single historical audit upgrade, restart, replay, and continuous-history journey already required. |
| `FIND-admin-principals-R3-6` | MISSING | Record reproducible exact commands for every named Rust proof. |
| `FIND-admin-principals-R4-1` | INCORRECT | Fail closed on revocation-store uncertainty for cache hits and misses. |
| `FIND-admin-principals-R4-2` | INCORRECT | Revoke a User's active refresh family with principal revocation. |
| `FIND-admin-principals-R4-3` | INCORRECT | Advance the User epoch when OIDC role replacement removes or changes authority. |
| `FIND-admin-principals-R4-4` | INCORRECT | Attribute refresh replay containment to the consumed refresh row. |
| `FIND-admin-principals-R4-5` | MISSING | Canonically audit tenant and platform access-token/session issuance in the grant transaction. |
| `FIND-admin-principals-R4-6` | INCORRECT | Keep initialization retryable when the one-time stdout disclosure fails. |
| `FIND-admin-principals-R4-7` | REGRESSION | Restore strict generic schema matching; isolate the exact audit evolution. |
| `FIND-admin-principals-R4-8` | MISSING | Prove failure/retry at every durable provisioning stage. |
| `FIND-admin-principals-R4-9` | VIOLATION | Route MCP HTTP/auth through the existing shared client behavior, including one 401 re-exchange/replay. |
| `FIND-admin-principals-R4-10` | DRIFT | Update active architecture and public docs to the shipped identity and renewal model. |
| `FIND-admin-principals-R4-11` | MISSING | Use the returned tenant credential through the shipped CLI in the real operator journey. |
| `FIND-admin-principals-R4-12` | VIOLATION | Complete mandatory Rustdoc for every cumulative new/materially modified item. |
| `FIND-admin-principals-R4-13` | VIOLATION | Remove the seven extra EOF blank lines so the cumulative diff passes `git diff --check`. |

## Prior-finding closure

Closed at this candidate: `FIND-admin-principals-1`, `-2`, `-3`, `-4`, `-8`,
`FIND-004-3`, `FIND-005-1`, `FIND-003-2`, `FIND-004-5`,
`FIND-admin-principals-R2-2` through `R2-6`, and
`FIND-admin-principals-R3-1` through `R3-4` for their previously named
boundaries. `FIND-admin-principals-13`, `R3-5`, and `R3-6` remain open in the
narrowed forms above. The new R4 roots are distinct cross-boundary failures,
not reversals of the verified R3 fixes.

## Verification and limits

The implementation packet records all seventeen required lanes green, both
platform journey runs green, strict `wyrd-sql` rustdoc green, and the three
additional Bifrost integration lanes green. Reviewers independently reran the
tenant-isolation, client-tier, unwrap, and clippy-allow static checks
successfully. Those results are credible for what they select but do not prove
the missing scenarios or contracts above. The validator also reproduced the
seven base-to-candidate `git diff --check` failures.

Per approved `VER-003`, `mise run gate` was neither required nor substituted.
This review did not modify implementation source, merge, push, or deploy.
