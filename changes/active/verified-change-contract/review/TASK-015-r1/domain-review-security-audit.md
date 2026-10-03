# Domain Review — Security, RBAC, and Audit Merge Resolution

## Subject and reviewed boundary

- Candidate: `9b560d00569656ee7fdde19a9c76fa55c8d55fbb`.
- Merge resolution: `c5527627a50dd66a9f53d760d80f59bcb59609f9`, compared with parents `ca2950856a37786a5cad25c73c1786c5fa7a1822` and `52e1144b5c186ccacd85d9c779a4e60c3cce5ac2`.
- Scope: only the security/RBAC/audit intent joined by that merge, plus whether TASK-015's later wiring regressed it. Earlier TASK-013/TASK-014 behavior, the audit-outbox implementation, its benchmark, merge `3f8767a5f`, and generic-outbox internals were not reopened.
- Required joined intent: permission decisions remain blocking and stage through the one non-blocking process `AuditOutbox`; no reachable audit-unavailable error remains; SYSTEM-token issuance and acceptance remain removed while the credentialless tenant SYSTEM identity remains available for internal attribution and narrowly scoped in-process authority.

## Authority coverage

| Boundary | Governing authority | Result |
|---|---|---|
| Authorization decision audit | `AGENTS.md` current decisions; `architecture/agent-rules.md:12-14`; `architecture/wyrd-security-posture.md:21-24,358-398`; applicable audit sections of `architecture/wyrd-design.md` and `architecture/bifrost-design.md` | PASS |
| SYSTEM identity and token prohibition | Approved spec revision 60, including the internal SYSTEM writer definition and AC-023; `architecture/wyrd-security-posture.md:91-104`; applicable identity and verification sections of `architecture/wyrd-design.md` and `architecture/bifrost-design.md` | PASS |
| Removed public audit-unavailable contract | `architecture/agent-rules.md:14`; audit-outbox authority merged by parent `52e1144b5`; protobuf compatibility rules | PASS |
| TASK-015 composition seam | TASK-015's prescribed `ObservationRunSink` wiring, inspected only for preservation of the shared audit owner | PASS |

## Source coverage and evidence

### Non-blocking canonical audit path

- `crates/vala/vala-bifrost-redux/src/gate/mod.rs:255-297` defines Gate's audit capability as synchronous staging and implements it directly for `AuditOutbox`. The event retains verified tenant, principal, principal kind, resource, permission, outcome, Card attribution, and delegated-actor detail.
- `crates/vala/vala-bifrost-redux/src/gate/mod.rs:490-519` completes the RBAC verdict first, stages exactly the allowed or denied decision without awaiting database IO, then enforces the verdict. A missing composition capability maps to an internal defect rather than an audit-unavailable public error.
- `crates/wyrd/wyrd-server/src/boot/mod.rs:1101-1133` composes the same `audit_outbox` into Gate and retains it in `BifrostComposition`. TASK-015 changes only the neighboring observation-run hook and leaves `.with_audit(Arc::clone(&audit_outbox))` intact.
- `crates/wyrd/wyrd-server/src/state.rs:1574-1608,1614-1648,2197-2203,2231-2238` retains that one process outbox through Bifrost and reuses it as `AppState::audit_outbox`; only the explicitly ownerless test shell creates its own outbox.
- `crates/wyrd/wyrd-server/src/app/server.rs:873-886` drains the Eval run-request outbox and then the shared audit outbox during shutdown, after the corresponding producers have stopped. TASK-015 does not replace or bypass the audit owner.
- The conflict resolution in `crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs:965-995` retains TASK-013/014's public result-write denial expectations while adding the audit-outbox settlement barrier required by non-blocking audit. It no longer expects or fabricates a SYSTEM token.

### Audit-unavailable removal

- Repository search found no reachable `AuditUnavailable`, `QueryAuditUnavailable`, or `AUDIT_UNAVAILABLE` error in Gate, server, auth, public error catalogs, SDK error mappings, or query conversion.
- `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto:23-35` reserves both the removed enum number and name rather than exposing the old terminal code or reusing its wire identity.
- `crates/wyrd-spec/src/vala/audit_detail.rs:432-440` retains `AuditErrorCode::AuditUnavailable` solely to decode retained historical audit evidence and documents that it is no longer recorded. It is not a reachable operation-failure code and therefore does not violate the exact removal obligation.
- Schema occurrences and historical migration comments likewise do not provide a runtime error path.

### SYSTEM-token removal with legitimate attribution preserved

- `crates/wyrd/wyrd-auth/src/issuance.rs` contains no `issue_system_token` path after the merge. Its surviving test at `:1248-1305` exercises every public grant shape against the stored SYSTEM writer and requires `PrincipalInactive`.
- `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:372-382` deliberately omits `system` from stored machine kinds eligible for token issuance.
- `crates/shared/wyrd-auth-verify/src/lib.rs:751-765,825-837` rejects every signed `PrincipalKindTag::System` claim set, even if an internal signing key were used to manufacture one.
- `crates/wyrd/wyrd-server/tests/pg_verification_routes.rs:740-842` preserves the credentialless UUIDv7 SYSTEM row as an internal attribution identity while proving it is unreachable through public principal and credential routes.
- `crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs:1074-1137` proves every public writer, including a wildcard administrator, is refused at all verification result tables and audited under the caller's true kind. The same file's SYSTEM-read journey beginning at `:1139` uses the stored identity only for tokenless, table-scoped in-process authority. This is the required distinction between removing SYSTEM tokens and retaining SYSTEM attribution.
- The merge retains the peer capture protobuf attribution needed for tokenless internal result writes while also retaining the reserved removal of the query audit-unavailable code.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None. No optional hardening is required by this task's approved boundary.

### Positive Controls

- Public result writes fail closed before Scribe admission, including wildcard administrators.
- Allowed and denied Gate decisions retain verified-principal attribution and delegation detail while audit persistence remains off the request path.
- Production composition shares one `AuditOutbox` across Gate and `AppState`; TASK-015 adds a separate typed Eval run-request outbox without shadowing the audit sink.
- SYSTEM is denied both at issuance and verification, so a mistakenly signed SYSTEM claim cannot become runtime authority.
- Removed protobuf error identity is reserved rather than recycled.

## Findings

No material security, RBAC, tenancy, secret-exposure, injection, token-handling, or audit-path finding was identified in the user-directed scope.

## Verification limits

- Per assignment, no Cargo or mise command was run and no production code or test was modified.
- Review used `git diff-tree --cc`, direct merge-to-parent diffs, commit blobs from both parents, current source/callers, and the implementer's recorded verification evidence. `git show --remerge-diff` could not materialize its temporary object directory in this worktree, so the equivalent two-parent comparison was used.
- The two Eval journey edits and the permanently failing Eval-item backlog risk are outside this security/RBAC domain pass and are left to the behavior, invariant, and resilience reviewers.
- Generic-outbox retry, ordering, commit-resolution, and durability mechanics were intentionally not re-reviewed.

## Overall result

**PASS** — the conflict resolution retains both sides' required security intent, and TASK-015's later wiring does not regress it. The scoped finding ledger is empty.
