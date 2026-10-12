# TASK-004 verdict

**FIX_REQUIRED**

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/local-server-bootstrap-task-004`.
- Base: `cb5656b2b913dbd88a8f6736bf4a0baaf998960c`.
- Candidate: `741e42ed97ac3ff9638217bbcd4f0c0583e6d464`.
- Complete range includes initial implementation `9fae5e368` and cleanup `741e42ed9`.
- Approved spec: `changes/active/local-server-bootstrap/spec.md`, revision 7; TASK-004 bootstrap obligations are unchanged by later independent tasks.
- Original task and corrected evidence: `changes/active/local-server-bootstrap/tasks/TASK-004-local-dev-journey.md`.
- Separate draft-docsite range: `2044e161e1a2020f5bb87cc6a36ee893ab311340..aecdaa05229d3571dedd362e8344fc28da5808a5`.
- No prior independent task verdict or FIND ledger; the cleanup is part of this initial cumulative acceptance audit.

User calibration: this is local development setup; do not overcomplicate findings. Retained corrections use existing owners and protect concrete task behavior. No hostile local endpoint-attestation protocol, restart machinery, new durable fixture owner, or production orchestration is required.

## Independent reports

| Role | Report | Discovery result | Reconciliation |
| --- | --- | --- | --- |
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL | Shared-database regression confirmed; revise correction to existing lane routing. |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | PASS | Protected persistence and issuance pass; specific prompt-exit and gRPC paths missed by broad acceptance are retained after validation. |
| Repository standards | [standards-review.md](standards-review.md) | FAIL | Required rustdoc gaps consolidated. |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL | Same rustdoc boundary; no blocking complexity finding. |
| System resilience | [system-review.md](system-review.md) | FAIL | Child exit during host prompt confirmed. |
| Security/RBAC/tenancy | [domain-review-security.md](domain-review-security.md) | FAIL | Two effective gRPC configuration claims consolidated; hostile listener proposal rejected. |
| Local persistence/durability/concurrency | [domain-review-persistence.md](domain-review-persistence.md) | PASS | Existing shared lock, atomic replacement, key/cache preservation and partial commit behavior pass. |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED | Listener-substitution possibility qualified by actual setup path and local threat scope. |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED | Four independently confirmed/revised findings; decision-complete corrections and focused proof. |

The security specialist's role prohibited file edits; the orchestrator preserved its full final report verbatim at its assigned path. Every other reviewer wrote its assigned report. No role was simulated or shared. See [subject.md](subject.md) for navigation/authority inputs and [claim-comparison.md](claim-comparison.md) for grouped claims.

Follow-up was required to resolve SEC-001 reachability and resource-ownership uncertainty. Its source audit acknowledges the first-use platform exchange path, but rejects an unapproved hostile co-user assurance expansion. Independent validation agrees. No additional follow-up or material authority decision remains unresolved. Rejected proposals are omitted from the retained ledger and remediation.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001 / AC-001: installed CLI shares Rust behavior, no checkout needed | Python/Node entrypoints → `ServerCommand::Dev` | Recorded production wheel; fresh focused packed Node and Python cases | PASS for installed dispatch |
| REQ-004: retain compatible installed release; no silent update | `DevArgs::run`, `DevServer::new`, existing installer | CLI retry/current-link assertions; compatibility source ordering | PASS |
| REQ-005 / INV-001: server-owned migration/setup; ordinary serving | `DevServer::{start,migrate,provision}` → installed server modes | Fresh unmigrated-database CLI scenario; recorded Linux startup seam | PASS |
| REQ-007: external database, local storage and macOS memory | `local_bind`, `detect_memory`, ordinary inherited storage owner | First-use, missing-input and migration-refusal proof | PASS except gRPC boundary below |
| REQ-007 / INV-003: effective local public listeners remain loopback | HTTP validated; public gRPC override inherited without local validation | Source-validated non-loopback configured-input path | FAIL — FIND-TASK-004-3 |
| REQ-008 / INV-003–004: server-issued key, distinct root, protected files and terminal disclosure | `ProvisionedTenant`, `disclose_root`, existing file writer/lock | Credential mode/output assertions, PTY and shared preservation test | PASS |
| REQ-009 / AC-001 / AC-004: saved deployment usable by fresh SDK processes | HTTP saved/verified; existing explicit gRPC destination remains active | HTTP Card journeys pass; actual shared gRPC precedence/transport trace exposes mismatch | FAIL — FIND-TASK-004-3 |
| REQ-009–010 / INV-004: preserve unrelated config, keys, cache, safe retries | Locked document reread, expected-key comparison, equal-key no rewrite | CLI retry and cached-credential preservation proof | PASS |
| REQ-010 / AC-007: partial progress, explicit recovery, no replacement issuance | Credential-first persistence, active-tenant setup branch, static recovery | Partial-persistence CLI scenario; recorded explicit recovery commands | PASS |
| REQ-005 / REQ-010: foreground exit reporting and truthful readiness through host offer | Child wait deferred until offer returns | Source-proven unanswered-prompt child-loss path; existing interruption test does not cover it | FAIL — FIND-TASK-004-2 |
| REQ-005: interruption cancels downloads/prompts/serving and settles child | Outer signal select, cancellable tty reader, existing stop/reap and language adapters | Fresh native and installed interruption cases; recorded production wheel | PASS for tested cancellation paths |
| REQ-011 / REQ-014 / AC-005: selected MCP host reads; decline remains usable | Existing installer/proxy composed by local offer | Fresh MCP read and packed Node decline/write/read | PASS apart from child observation above |
| REQ-015 / AC-007: stable redacted failures | Derive-backed `ServerDev`, static guidance | CLI negative-flow assertions, source failure-path audit | PASS |
| AC-001 / AGENTS §11–12: fixture cleanup retains owning verification | Four drivers selected in incompatible shared-database lanes as well as isolated dedicated lane | Traced combined two-case reproduction: one passes, next fails outside active warehouse | FAIL — FIND-TASK-004-1 |
| AGENTS §16: touched-item rustdoc contracts | Four helper error/panic sections missing | Standards/maintainer proposals independently validated against full bodies | FAIL — FIND-TASK-004-4 |
| AC-008 / original task docsite obligation | Current docs and separate draft Setup/Troubleshooting; future Quickstart prerequisite task | Recorded docs checks, independent current/draft source review | PASS |
| Cleanup and non-goals | Focused lifecycle methods; standalone runner absent; existing durable/installer/host/file owners retained | Complete cumulative diff and caller audit | PASS, subject to fixture routing correction |

## Independently validated finding ledger

| Stable ID | Discovery IDs | Status/classification | Concrete consequence and selected correction |
| --- | --- | --- | --- |
| FIND-TASK-004-1 | BEH-001 | REVISED / REGRESSION | Ordinary Python integration/Bifrost story selectors execute fresh bootstrap drivers against shared durable state and fail. Route only these drivers to their existing dedicated per-command database lane; keep all four required cases and aggregate dependencies. |
| FIND-TASK-004-2 | SYS-001 | CONFIRMED / INCORRECT | Server exit during an unanswered host prompt goes unreported and can precede a false ready message. Observe the owned child during the offer in DevServer and use its existing cleanup/error path. |
| FIND-TASK-004-3 | SEC-002, SEC-003 | REVISED / INCORRECT, VIOLATION | HTTP setup leaves saved SDK gRPC destination inconsistent and permits an inherited remote plaintext gRPC listener. Select a loopback public gRPC bind at CLI preflight, supply it to children, and save its actual destination through the existing protected config owner. |
| FIND-TASK-004-4 | MR-001, REPO-004-1 | CONFIRMED / VIOLATION | Required helper error/panic documentation is absent. Document the four existing contracts without runtime changes. |

Exact locations, producer/consumer and sibling traces, correction boundaries, and focused closure proof are preserved in [findings-validation.md](findings-validation.md). No prior findings need closure and no human-directed addition has been assigned a reviewer FIND ID.

## Verification and limits

Fresh `WYRD_LOG=info mise run test:local-dev:journey`: PASS, exit 0, 140.92 seconds; three exact CLI scenarios and four separately wrapped direct pytest drivers, including packed Node. `git diff --check` for the full range: PASS. See [verification.md](verification.md).

Behavior reviewer additionally ran the tracing-enabled Python/interrupt driver pair under one wrapper: one passed, one failed in 20.68 seconds, with retained first-warehouse catalog state rejected by the second deployment. This explains how the focused lane passes while shared-lifecycle selection fails.

Other static, production wheel, Linux startup, and draft documentation checks are assessed from the candidate's recorded final evidence, not claimed as fresh reruns. Live official-release downloads, forced terminal-write failure, power-loss/concurrency fault injection and hostile listener exploitation were not performed. Draft accessibility does not render draft pages. The prompt child-loss and gRPC corrections require the focused additional proof specified in the remediation; HTTP success alone does not close them.

HEAD remained the candidate and tracked source remained unchanged. Only review artifacts were added. No implementation, commit, merge, push or deployment was performed. Review does not approve TASK-004 yet.

Remediation: [TASK-004-R1-local-setup-corrections.md](TASK-004-R1-local-setup-corrections.md), routed directly to `$wyrd-implement`. A later independent review must audit the full cumulative candidate against the original task and this ledger.
