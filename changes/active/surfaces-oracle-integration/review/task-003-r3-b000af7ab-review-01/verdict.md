# TASK-003-R3 review verdict

## Immutable subject

- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`.
- Candidate: `b000af7ab704f077a8a4ba3e29c2b968d47d344d`, tree `f087f7d395cd53386e4e2c6f011b31d8604fd791`.
- Last source commit: `f8e887b3038651d2ba82091d642915d6b856d4d6`, tree `eb98062dc854277a248f37d86afe55beb9c3db26`; the later candidate commit changes review/evidence Markdown only.
- Authority: approved `changes/active/surfaces-oracle-integration/spec.md` revision 9; original `TASK-002`, `TASK-004`, `TASK-003`; remediation `TASK-003-R1`, `TASK-003-R2`, `TASK-003-R3`; `AGENTS.md` and applicable architecture rules.

The full cumulative diff was reviewed. Candidate HEAD/tree stayed fixed. Unrelated uncommitted `verified-change-contract` edits were excluded. `.codegraph/` is absent, and `git diff --check 861f8d86..b000af7ab` passed.

## Acceptance matrix

| Obligation | Evidence | Result |
|---|---|---|
| Preserve cumulative client, storage-root, Forge, SDK, cloud-workflow, and documentation corrections | Cumulative source inspection, prior reviews, R3 same-tree focused and cloud results | PASS on available evidence |
| Bound a connected silent remote Oracle through its first response by the captured query deadline, with typed timeout, cancellation, and no successor | `route_remote_once` uses the ingress `Instant`; focused unit and role-separated HTTP journeys passed; forwarding and peer-security domain reviews passed | PASS — R2-3 closed |
| Preserve protected HTTP edge, local Oracle, peer authentication/tenancy, and terminal stream behavior | Unchanged edge handoff and peer acceptance paths; test hook is `test-support`-gated | PASS |
| Complete missing `EdgeTimeout` rustdoc without behavior change | Associated-type docs and fallible-method `# Errors` are present | PASS — R2-2 closed |
| Run six previously missing owner lanes and prove `verify:bifrost` on the corrected source | Six lanes recorded nonzero and passing; the R3-authorized exact-child Bifrost map is present | PASS for those lanes — R2-1 narrowed |
| Pass the broad repository gate on the final corrected candidate | Final-tree `gate` attempts were killed; child results are grouped and R3 permits child equivalence only for `verify:bifrost` | FAIL — `FIND-TASK-003-R3-1` |
| Meet mandatory rustdoc for every new Rust item | Two new tuple fields and the new assertion-bearing unit test have documentation gaps | FAIL — `FIND-TASK-003-R3-2` |
| Use top-level imports and bare types in new/materially changed fields and signatures | Qualified type paths remain in `forwarding.rs` and the new state accessor | FAIL — `FIND-TASK-003-R3-3` |
| Keep local real-cloud proof and non-goals | S3/GCS/Azure tasks each report two passing tests on tested source; no public deadline/schema change, production test hook, push, merge, or deploy | PASS on available evidence |

## Independent results and validated findings

| Review | Result |
|---|---|
| Task implementation | FAIL — one broad-gate evidence finding |
| Repository standards | FAIL — rustdoc and import-style findings |
| Oracle forwarding domain | PASS — no material finding |
| Peer security/tenancy domain | PASS — no material finding |
| Structured Ponytail validation | Three findings confirmed; both domain ledgers validated empty |

| Finding | Classification | Required correction |
|---|---|---|
| `FIND-TASK-003-R3-1` | MISSING | Obtain a passing final-tree `gate` result; do not treat the killed aggregate as passed. |
| `FIND-TASK-003-R3-2` | VIOLATION | Document the two new tuple fields and the new test's panic conditions. |
| `FIND-TASK-003-R3-3` | VIOLATION | Use module-top imports and bare types in the identified fields, signatures, and bound. |

`findings-validation.md` records each finding's reachability, exact locations, minimal correction, and focused proof. R2-3 and R2-2 are closed. R2-1's six omitted lanes passed, but its broader final-tree gate obligation remains open as R3-1.

## Verification limits and verdict

The review reran only the two focused forwarding unit tests (both passed). It did not rerun the memory-heavy gate, Postgres journeys, or credentialed cloud tasks. The reported same-tree child runs provide substantial functional coverage, but neither the approved specification nor R3 authorizes their grouped record as a passing `gate`. No product behavior failure is inferred from the host-memory kill. The candidate remains local; this review is not merge approval.

**FIX_REQUIRED.** The bounded corrections are packaged in `TASK-003-R4-close-r3-review-findings.md`. No specification revision is needed to run the existing gate and meet the existing Rust rules.
