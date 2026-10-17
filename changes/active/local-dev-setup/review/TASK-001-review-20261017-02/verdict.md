# TASK-001 cumulative re-review verdict

**FIX_REQUIRED** — six prior findings are closed; FIND-TASK-001-2 and
FIND-TASK-001-7 remain, and FIND-TASK-001-9 is newly confirmed. Execute
[TASK-001-R2](TASK-001-R2-identity-and-restart-offers.md) with `$wyrd-implement`.

## Subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Original base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
- Candidate: `38dbc023cd71292e1badd3abe90e87c69b76fa96`.
- Prior candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c`.
- Approved [spec revision 1](../../spec.md); original [TASK-001](../../tasks/TASK-001-guided-local-development.md).
- Prior [verdict](../TASK-001-review-20261017-01/verdict.md), [validated ledger](../TASK-001-review-20261017-01/findings-validation.md) and [R1 task/evidence](../TASK-001-review-20261017-01/TASK-001-R1-local-development-corrections.md).
- Cumulative subject: `git diff bc3bf25e3...38dbc023c`; the prior-candidate fix range only located remediation owners.
- User scope is the full implemented change in this worktree. Sibling repository state is excluded and is not a blocker. Initial working tree was clean; only current review reports were added.

AGENTS, agent-rules, approved spec/task and applicable design/doctrine/security/
operations/reference authorities govern. The immutable source was not changed.
No new product decision, specification revision or external review input is
required to make the three bounded corrections.

## Independent reports and reconciliation

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL, retained finding 2 |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL, finding 2 and proposed repeated offers |
| Standards | [standards-review.md](standards-review.md) | FAIL, residual mandatory rustdoc |
| Maintainer | [maintainer-review.md](maintainer-review.md) | PASS |
| System resilience | [system-review.md](system-review.md) | PASS |
| Security/RBAC/secrets | [domain-review-security.md](domain-review-security.md) | PASS |
| Data integrity/tenancy/concurrency | [domain-review-data.md](domain-review-data.md) | PASS |
| Focused identity follow-up | [followup-review.md](followup-review.md) | RESOLVED, identity path remains reachable |
| Independent Ponytail validation | [findings-validation.md](findings-validation.md) | COMPLETE, three retained findings |

The identity follow-up was required because discovery reports materially
disagreed about finding 2. It traced both same-origin different-tenant keys
without dev selection and saved-login precedence masking the retained key.
The validator resolved the remaining correction-boundary uncertainty using
existing local selection and exact-credential interfaces, avoiding a new
identity API or platform-authority prerequisite. Closure was decided from
source, not reviewer counts. [Claim comparison](claim-comparison.md) preserves
the routing; unique proposals received the same validation as shared claims.

## Validated finding ledger

Full exact producer/caller evidence, disposition, correction and focused proof
are in [findings-validation.md](findings-validation.md) and packaged
self-contained in [R2](TASK-001-R2-identity-and-restart-offers.md).

| ID / status | Sources / obligation | Location, consequence and selected correction |
|---|---|---|
| FIND-TASK-001-2 — REVISED / INCORRECT | BEH-R2-1, INV-R2, FUP-R2-IDENTITY; REQ-002/008, INV-003/004 | `wyrd-cli/src/server/dev.rs:307-327,535-551,1269-1304`. Same-origin no-dev selection can authenticate B before writing A settings/issuing an unsavable A key; normal saved-login precedence can mask the retained key. Refuse an unproven no-dev protected-key selection, then explicitly preflight the exact key for saved-dev resumes with no tenant selector and in-memory caching. Preserve normal post-setup precedence and supported partial-save/recovery flows. |
| FIND-TASK-001-7 — CONFIRMED / VIOLATION | REPO-R2-001; AGENTS §16 and explicit rustdoc gate | `wyrd-server/src/mcp/gateway.rs:633-685`. Materially changed cumulative catalog test lacks its panic contract. Add accurate documentation without changing assertions; batch with substantive corrections. Prior enumerated items are fixed, but the cumulative omission remains. |
| FIND-TASK-001-9 — CONFIRMED / INCORRECT | INV-R9; REQ-004/008, AC-006 | `wyrd-cli/src/server/dev.rs:341-345` and gateway/host consumers. HTTP listener inequality marks an otherwise valid resumed deployment fresh, repeating interactive setup offers. Derive resumed-setup offer policy from retained dev selection/key at the one shared producer, preserving first use/recovery and explicit gateway/host requests. |

No rejected proposal or non-blocking naming/import preference is retained.
Findings 2 and 7 preserve prior IDs; 9 is the next unused ID for the independent
offer-policy root cause. No further findings are assigned.

## Prior finding closure

| Prior ID | Independently validated current source and proof | Outcome |
|---|---|---|
| 1 | Resolved interactive flag reaches host owner; real tty unattended case and explicit-host restart preserve both behaviors. | CLOSED |
| 2 | Origin refusal/default-key-only empty-database rejection improve coverage; exact-key substitution and same-origin no-dev selection remain reachable. | OPEN, revised above |
| 3 | Generated/file/inline effective key retained owner-only; no-export cross-directory restart actually invokes the local provider and preserves tenant key; different supplied bytes refuse without replacement. | CLOSED |
| 4 | Generic CLI/docs/503 uncertainty now matches commit-before-response behavior; proxy loses a real committed answer, then proof checks settlement, durable readback and reapply. | CLOSED |
| 5 | Biased signal polling precedes input resolution; PTY INT/TERM proof checks exit 130, restored echo and no setup persistence. | CLOSED |
| 6 | Real later provider FK rejection follows earlier managed rotation; whole rows/envelopes/deployments match and original secret decrypts afterward. | CLOSED |
| 7 | Specifically enumerated helpers/constants are documented; another materially changed cumulative catalog test still lacks mandatory panic docs. | OPEN, bounded residual |
| 8 | Shared fixture now uses standard loopback bind/address discovery with no procfs read. Linux focused runs recorded; macOS execution not claimed. | CLOSED in source |

The guided-reader assertion fix preserves the assertion and follows its
recorded incomplete-summary diagnosis. Direct utils dependency reuses the
existing protected secret reader. Neither produced a separately validated
finding. Six closures remain source-backed; none is inferred solely from
implementation completion claims or green checks.

## Reconciled acceptance matrix

PASS means source/proof-body acceptance within stated execution limits, not an
independent runtime rerun. Rows combine the complete original task with the
validated remediation findings; sibling-only obligations are outside scope.

| Requirement, criterion, invariant or non-goal | Source / available proof | Result |
|---|---|---|
| REQ-001 / AC-001/002: shared input path, usable concealed input, unconditional unattended behavior | DevArgs/Terminal and host policy; P1/P2/P9 plus tty signal and explicit-host cases | PASS |
| REQ-002: protected DSNs, stable paths, unrelated selection preserved | GlobalConfig/CredentialsFile and local roots; finding 2 remains | FAIL |
| REQ-003: existing installer/migrate/setup, retained keys, foreground ownership | Existing owners, actual generated/file/inline provider restart, early cancellation; finding 2 still violates safe issuance for unproven selection | FAIL — finding 2 |
| REQ-004 / AC-003: optional approved known providers, no repeated automatic offer on valid restart | Existing authoring/apply/provider selection; finding 9 on listener-only restart | FAIL |
| REQ-005 / AC-005: local-env document and retained single/array/two-step/source semantics | DeploymentFile/AuthoringApply/typed validation and P3/P5/P6/P7 | PASS |
| REQ-006 / AC-004: typed atomic tenant operation, reference safety, redaction, nonblocking audit, uncertainty | Shared transaction/FK/lock/sealing owners, P4 true SQL abort plus CLI/MCP/OpenAPI/lost-response proof | PASS |
| REQ-007: fresh saved-client readiness, readback and honest failure guidance | Ordinary post-setup saved-client bounded read, P1/P2/P7; identity preflight gap is under REQ-002/008 | PASS |
| REQ-008 / AC-006/007: preserved identity, corresponding overrides, restart and recovery | P8/P9 and sealing/cancellation proof; findings 2 and 9 | FAIL |
| REQ-009 / AC-008/009: in-tree docs/examples and SDK/MCP consumers | Shipped gateway document and saved consumer proof, docs/served contracts; sibling implementation excluded | PASS within scope |
| INV-001: single owners, pure contracts, server durable logic | Current startup/config/client/gateway/SQL/sealing/audit owners retained | PASS |
| INV-002: no plaintext disclosure or unprotected persistence | Security and standards source coverage; protected keys/DSNs, redacted transport/results and restart proof | PASS |
| INV-003/004: authority and honest durable identity/persistence | No authorization bypass found; retained selection/issuance gap remains under finding 2 | FAIL |
| INV-005: existing supported features and optional gateway | Existing methods/sources retained and real regression proof mapped above | PASS |
| INV-006: portable workflow/proof source | Fixture procfs removed; both supported OS families covered by source facilities, execution evidence Linux only | PASS in source; platform execution limit |
| AC-010: repository rules/contracts/typing/format/lint evidence | Recorded exact checks, served OpenAPI/runtime MCP source proof; mandatory rustdoc residual finding 7 | FAIL |
| Non-goals: no DB installer/role creation, UI/daemon/profile/vault/receipt/recovery framework, SDK secret mutation, arbitrary providers or hidden app start | Complete cumulative owner/domain/standards coverage | PASS |

## Verification, remaining work and limits

`git diff --check bc3bf25e3...38dbc023c` passed during re-review. Implementation
records exact focused Linux x86_64 results and relevant format/lint/docs/
dependency/Python checks in R1. Reviewers inspected those proof bodies and
source paths; runtime suites/builds/checks were not rerun in this source audit.
No macOS/arm64 execution is claimed. These execution limits are not external
blockers or new product requirements. See [verification](verification.md).

The existing passing proof does not exercise the newly confirmed identity and
interactive changed-listener cases. R2 requires focused regression proof for
those gaps and batches the rustdoc correction. It does not require broad
journey suites, sibling commits or another specification decision. The next
review must read the complete original base-to-corrected range, original task,
both prior verdicts/ledgers and both remediation tasks.

Final result: **FIX_REQUIRED**, with the three IDs above and
[TASK-001-R2-identity-and-restart-offers.md](TASK-001-R2-identity-and-restart-offers.md).
Review made no implementation, staging, commit, merge, push or deployment.
